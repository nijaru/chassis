#![allow(unsafe_code)]
//! Tail ABI ownership: concurrent main queries and synchronous audio re-entry.

use chassis_clap::{ClapComponent, SingleComponentEntry};
use chassis_runtime::{
    process::{ActivationConfig, ProcessBlock},
    runtime::{Component, LatencySamples, Process, Processor, TailLength},
    schema::{ComponentId, ComponentSchema, ComponentSchemaError, StateSchemaVersion},
};
use clack_extensions::{
    latency::{HostLatency, HostLatencyImpl, PluginLatency},
    log::{HostLog, HostLogImpl, LogSeverity},
    tail::{HostTail, HostTailImpl},
};
use clack_host::prelude::*;
use clap_sys::{
    audio_buffer::clap_audio_buffer,
    ext::tail::{CLAP_EXT_TAIL, clap_plugin_tail},
    plugin::clap_plugin,
    process::clap_process,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::{Cell, OnceCell},
    convert::Infallible,
    sync::{
        Arc, Barrier, OnceLock,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    },
};

thread_local! {
    static COUNT: Cell<bool> = const { Cell::new(false) };
    static AUDIO: Cell<bool> = const { Cell::new(false) };
}
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static DEALLOCATIONS: AtomicU64 = AtomicU64::new(0);
struct Allocator;
// SAFETY: all allocator arguments are forwarded unchanged to System.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNT.try_with(Cell::get).unwrap_or(false) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: unchanged valid allocation layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if COUNT.try_with(Cell::get).unwrap_or(false) {
            DEALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: unchanged valid allocation pair.
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static GLOBAL: Allocator = Allocator;

static NEXT_TAIL: AtomicU64 = AtomicU64::new(5);
static PAUSE: AtomicBool = AtomicBool::new(false);
static BARRIERS: OnceLock<(Barrier, Barrier)> = OnceLock::new();

#[derive(Default)]
struct TailProbe;
struct TailProcessor(u64);
impl Component for TailProbe {
    type Processor = TailProcessor;
    type ActivationError = Infallible;
    fn schema(&self) -> Result<ComponentSchema, ComponentSchemaError> {
        ComponentSchema::stereo_effect_with_state(
            ComponentId::new("test.tail").unwrap(),
            StateSchemaVersion::new(1),
            vec![],
        )
    }
    fn activate(
        &self,
        _: &ActivationConfig<'_>,
        _: &chassis_runtime::parameters::ParameterStore,
        _: &[chassis_runtime::state::StateEntry],
    ) -> Result<TailProcessor, Infallible> {
        Ok(TailProcessor(NEXT_TAIL.load(Ordering::Relaxed)))
    }
}
impl Processor for TailProcessor {
    fn latency(&self) -> LatencySamples {
        LatencySamples::new(3)
    }
    fn tail(&self) -> TailLength {
        TailLength::FiniteSamples(self.0)
    }
    fn reset(&mut self) {
        self.0 = 0;
    }
}
impl Process<f32> for TailProcessor {
    fn process(
        &mut self,
        _: &mut ProcessBlock<'_, f32>,
    ) -> Result<(), chassis_runtime::runtime::ProcessError> {
        self.0 = NEXT_TAIL.load(Ordering::Relaxed);
        if PAUSE.load(Ordering::Relaxed) {
            // Test-only synchronization, deliberately excluded from RT measurement.
            let (entered, release) = BARRIERS.get().unwrap();
            entered.wait();
            release.wait();
        }
        Ok(())
    }
}
impl ClapComponent for TailProbe {
    const CLAP_ID: &'static str = "test.tail";
    const CLAP_NAME: &'static str = "Tail";
    const CLAP_FEATURES: &'static [&'static std::ffi::CStr] = &[c"audio-effect", c"stereo"];
    const CLAP_PARAMETER_IDS: &'static [(&'static str, u32)] = &[];
}

// SAFETY contract: the initialized plugin remains live; caller is on main or
// audio thread. This intentionally uses the standard vtable because the pinned
// host convenience API incorrectly restricts get to an audio handle.
unsafe fn query(plugin: *const clap_plugin) -> u32 {
    // SAFETY: caller upholds instance lifetime; standard identifier selects its ABI.
    unsafe {
        let extension = ((*plugin).get_extension.unwrap())(plugin, CLAP_EXT_TAIL.as_ptr());
        assert!(!extension.is_null());
        let extension = &*extension.cast::<clap_plugin_tail>();
        (extension.get.unwrap())(plugin)
    }
}

#[derive(Default)]
struct Observations {
    changes: AtomicU32,
    last_tail: AtomicU32,
    activation_tail: AtomicU32,
    wrong_thread: AtomicU32,
    logs: AtomicU32,
}
struct Shared(Arc<Observations>);
struct Main<'a> {
    instance: OnceCell<InitializedPluginHandle<'a>>,
    observations: Arc<Observations>,
}
struct AudioHost<'a> {
    instance: InitializedPluginHandle<'a>,
    observations: Arc<Observations>,
}
struct Handlers;
impl SharedHandler<'_> for Shared {
    fn request_restart(&self) {}
    fn request_process(&self) {}
    fn request_callback(&self) {}
}
impl HostLogImpl for Shared {
    fn log(&self, _: LogSeverity, _: &str) {
        self.0.logs.fetch_add(1, Ordering::Relaxed);
    }
}
impl<'a> MainThreadHandler<'a> for Main<'a> {
    fn initialized(&self, instance: InitializedPluginHandle<'a>) {
        assert!(self.instance.set(instance).is_ok());
    }
}
impl HostLatencyImpl for Main<'_> {
    fn changed(&self) {
        // SAFETY: activation is on main; the initialized instance is live, even
        // before Clack has installed the newly constructed audio processor.
        let tail = unsafe { query(self.instance.get().unwrap().as_raw_ptr()) };
        self.observations
            .activation_tail
            .store(tail, Ordering::Relaxed);
    }
}
impl<'a> AudioProcessorHandler<'a> for AudioHost<'a> {}
impl HostTailImpl for AudioHost<'_> {
    fn changed(&mut self) {
        if !AUDIO.get() {
            self.observations
                .wrong_thread
                .fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: synchronous re-entry during a valid audio callback. No mutable
        // DSP borrow is taken by the tail snapshot callback.
        let tail = unsafe { query(self.instance.as_raw_ptr()) };
        self.observations.last_tail.store(tail, Ordering::Relaxed);
        self.observations.changes.fetch_add(1, Ordering::Relaxed);
    }
}
impl HostHandlers for Handlers {
    type Shared<'a> = Shared;
    type MainThread<'a> = Main<'a>;
    type AudioProcessor<'a> = AudioHost<'a>;
    fn declare_extensions(builder: &mut HostExtensions<Self>, _: &Shared) {
        builder
            .register::<HostTail>()
            .register::<HostLatency>()
            .register::<HostLog>();
    }
}

fn audio<R>(count: bool, callback: impl FnOnce() -> R) -> R {
    AUDIO.set(true);
    COUNT.set(count);
    let result = callback();
    COUNT.set(false);
    AUDIO.set(false);
    result
}
fn process(processor: &mut StartedPluginAudioProcessor<Handlers>, count: bool) {
    let mut inputs = [[0.0_f32; 8]; 4];
    let mut outputs = [[0.0_f32; 8]; 2];
    let mut input_channels = inputs.each_mut().map(|samples| samples.as_mut_ptr());
    let mut output_channels = outputs.each_mut().map(|samples| samples.as_mut_ptr());
    let table = input_channels.as_mut_ptr();
    let buffer = |data32| clap_audio_buffer {
        data32,
        data64: std::ptr::null_mut(),
        channel_count: 2,
        latency: 0,
        constant_mask: 0,
    };
    // Own stable pointer tables directly: the pinned host's multiport builder
    // reborrows its entire table while constructing successive port pointers.
    let input = [buffer(table), buffer(table.wrapping_add(2))];
    let mut output = [buffer(output_channels.as_mut_ptr())];
    let input_events = InputEvents::empty();
    let mut output_events = OutputEvents::void();
    let process = clap_process {
        steady_time: -1,
        frames_count: 8,
        transport: std::ptr::null(),
        audio_inputs: input.as_ptr(),
        audio_outputs: output.as_mut_ptr(),
        audio_inputs_count: 2,
        audio_outputs_count: 1,
        in_events: input_events.as_raw(),
        out_events: output_events.as_raw(),
    };
    let plugin = processor.plugin_handle();
    let raw = plugin.as_raw_ptr();
    let status = audio(count, || {
        // SAFETY: started processor, exclusive audio-domain invocation; owned
        // descriptors, pointer tables and disjoint sample buffers outlive it.
        unsafe { ((*raw).process.unwrap())(raw, std::ptr::from_ref(&process)) }
    });
    assert_eq!(status, clap_sys::process::CLAP_PROCESS_CONTINUE);
}

fn plugin() -> (PluginInstance<Handlers>, Arc<Observations>) {
    let entry = PluginEntry::load_from_clack::<SingleComponentEntry<TailProbe>>(c"").unwrap();
    let observations = Arc::new(Observations::default());
    let shared = Arc::clone(&observations);
    let plugin = PluginInstance::<Handlers>::new(
        |()| Shared(shared),
        |shared| Main {
            instance: OnceCell::new(),
            observations: Arc::clone(&shared.0),
        },
        &entry,
        c"test.tail",
        &HostInfo::new("tail-test", "", "", "").unwrap(),
    )
    .unwrap();
    (plugin, observations)
}

fn activate(plugin: &mut PluginInstance<Handlers>) -> StoppedPluginAudioProcessor<Handlers> {
    plugin
        .activate(
            |shared, main| AudioHost {
                instance: main.instance.get().unwrap().clone(),
                observations: Arc::clone(&shared.0),
            },
            PluginAudioConfiguration {
                sample_rate: 48_000.0,
                min_frames_count: 1,
                max_frames_count: 8,
            },
        )
        .unwrap()
}

fn concurrent_query(
    processor: StartedPluginAudioProcessor<Handlers>,
    raw: *const clap_plugin,
) -> StartedPluginAudioProcessor<Handlers> {
    let (entered, release) = BARRIERS.get_or_init(|| (Barrier::new(2), Barrier::new(2)));
    PAUSE.store(true, Ordering::Relaxed);
    NEXT_TAIL.store(9, Ordering::Relaxed);
    let (processor, during) = std::thread::scope(|scope| {
        let thread = scope.spawn(move || {
            let mut processor = processor;
            process(&mut processor, false);
            processor
        });
        entered.wait();
        // SAFETY: parent/main retains the initialized plugin while child holds
        // the exclusive live processor borrow. Release before asserting.
        let during = unsafe { query(raw) };
        release.wait();
        (thread.join().unwrap(), during)
    });
    PAUSE.store(false, Ordering::Relaxed);
    assert_eq!(during, 5, "query must see the preceding completed snapshot");
    processor
}

#[test]
fn tail_queries_and_notifications_are_shared_reentrant_and_realtime_bounded() {
    let (mut plugin, observations) = plugin();
    let raw = plugin.plugin_handle().as_raw_ptr();
    // SAFETY: initialized inactive plugin; main-thread query is legal and must
    // not enter the pinned deactivated-audio-processor logging path.
    COUNT.set(true);
    let inactive = unsafe { query(raw) };
    COUNT.set(false);
    assert_eq!(inactive, 0);
    let unstarted = activate(&mut plugin);
    plugin.deactivate(unstarted);
    // The deferred change must survive an activation that never reached audio.
    let processor = activate(&mut plugin);
    assert_eq!(
        observations.changes.load(Ordering::Relaxed),
        0,
        "activation must not notify on main"
    );
    assert_eq!(
        observations.activation_tail.load(Ordering::Relaxed),
        5,
        "publish before reentrant latency notification"
    );
    let mut processor = audio(true, || processor.start_processing()).unwrap();
    assert_eq!(observations.changes.load(Ordering::Relaxed), 1);
    processor = concurrent_query(processor, raw);
    assert_eq!(observations.last_tail.load(Ordering::Relaxed), 9);
    assert_eq!(observations.changes.load(Ordering::Relaxed), 2);
    let handle = plugin.plugin_handle();
    assert_eq!(
        handle
            .get_extension::<PluginLatency>()
            .unwrap()
            .get(&handle),
        3
    );

    for iteration in 0..1_000 {
        NEXT_TAIL.store(if iteration % 2 == 0 { 13 } else { 9 }, Ordering::Relaxed);
        process(&mut processor, true);
    }
    assert_eq!(observations.changes.load(Ordering::Relaxed), 1_002);
    process(&mut processor, true);
    assert_eq!(
        observations.changes.load(Ordering::Relaxed),
        1_002,
        "unchanged tail does not notify"
    );
    NEXT_TAIL.store(2_147_483_647, Ordering::Relaxed);
    process(&mut processor, true);
    NEXT_TAIL.store(u64::MAX, Ordering::Relaxed);
    process(&mut processor, true);
    assert_eq!(
        observations.last_tail.load(Ordering::Relaxed),
        2_147_483_647
    );
    assert_eq!(
        observations.changes.load(Ordering::Relaxed),
        1_003,
        "compare projected values"
    );
    audio(true, || processor.reset());
    assert_eq!(observations.last_tail.load(Ordering::Relaxed), 0);
    assert_eq!(observations.changes.load(Ordering::Relaxed), 1_004);
    plugin.deactivate(processor.stop_processing());
    // SAFETY: plugin is still initialized; retain the last completed snapshot.
    assert_eq!(unsafe { query(raw) }, 0);
    assert_eq!(observations.wrong_thread.load(Ordering::Relaxed), 0);
    assert_eq!(observations.logs.load(Ordering::Relaxed), 0);
    assert_eq!(ALLOCATIONS.load(Ordering::Relaxed), 0);
    assert_eq!(DEALLOCATIONS.load(Ordering::Relaxed), 0);
}
