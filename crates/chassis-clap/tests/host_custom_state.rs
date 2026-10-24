#![allow(unsafe_code)]
//! Actual-entry resource-backed complete state, including an in-flight DSP load.

use chassis_clap::{ClapComponent, SingleComponentEntry};
use chassis_runtime::{
    automation::ParameterEventValue,
    parameters::{ParameterDescriptor, ParameterIndex, ParameterStore},
    process::{ActivationConfig, ProcessBlock, ProcessBufferSource, ProcessChannel},
    runtime::{Component, ComponentStateError, Process, Processor},
    schema::{ComponentId, ComponentSchema, ComponentSchemaError, StateSchemaVersion},
    state::{StateDocument, StateEntry, StateMigration, StateValue},
};
use clack_extensions::{params::PluginParams, state::PluginState};
use clack_host::{
    events::event_types::ParamValueEvent, factory::plugin::PluginFactory, prelude::*,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    convert::Infallible,
    io::Cursor,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{Receiver, SyncSender, sync_channel},
    },
    time::Duration,
};

thread_local! { static COUNT: Cell<bool> = const { Cell::new(false) }; }
static ALLOC: AtomicUsize = AtomicUsize::new(0);
static DEALLOC: AtomicUsize = AtomicUsize::new(0);
struct Allocator;
// Test-only allocator; every operation forwards the original system contract.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNT.try_with(Cell::get).unwrap_or(false) {
            ALLOC.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: unchanged valid allocation layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if COUNT.try_with(Cell::get).unwrap_or(false) {
            DEALLOC.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: unchanged pointer/layout pair from the caller.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if COUNT.try_with(Cell::get).unwrap_or(false) {
            ALLOC.fetch_add(1, Ordering::Relaxed);
            DEALLOC.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: unchanged live allocation and requested size.
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static GLOBAL: Allocator = Allocator;

static INCOMPATIBLE: [AtomicUsize; 2] = [const { AtomicUsize::new(0) }; 2];
static DROPS: [AtomicUsize; 2] = [const { AtomicUsize::new(0) }; 2];
static BLOCK_ONCE: AtomicBool = AtomicBool::new(false);
static DSP_GATE: OnceLock<(SyncSender<()>, Mutex<Receiver<()>>)> = OnceLock::new();
const ID: &str = "test.custom-table";
#[derive(Default)]
struct Table<const RACE: bool>;
fn table(custom: &[StateEntry]) -> Option<&[u8]> {
    let [entry] = custom else { return None };
    match entry.value() {
        StateValue::Bytes(bytes)
            if entry.key() == "state/table" && (1..=2).contains(&bytes.len()) =>
        {
            Some(bytes)
        }
        _ => None,
    }
}
struct Rename;
impl StateMigration for Rename {
    type Error = ComponentStateError;
    fn source_schema(&self) -> u32 {
        1
    }
    fn to_schema(&self) -> u32 {
        2
    }
    fn migrate(&self, doc: StateDocument) -> Result<StateDocument, Self::Error> {
        assert_eq!(doc.product_id(), ID);
        let mut next = StateDocument::new(ID, 2).map_err(ComponentStateError::new)?;
        for entry in doc.entries() {
            let key = if entry.key() == "parameter/old_index" {
                "parameter/index"
            } else {
                entry.key()
            };
            next.insert(StateEntry::new(key, entry.value().clone()))
                .map_err(ComponentStateError::new)?;
        }
        Ok(next)
    }
}
impl<const RACE: bool> Component for Table<RACE> {
    type Processor = TableDsp<RACE>;
    type ActivationError = Infallible;
    fn schema(&self) -> Result<ComponentSchema, ComponentSchemaError> {
        ComponentSchema::stereo_effect_with_state(
            ComponentId::new(ID).unwrap(),
            StateSchemaVersion::new(2),
            vec![ParameterDescriptor::float("index", "Index", 0.0, 1.0, 0.0).unwrap()],
        )
    }
    fn default_custom_state(&self) -> Vec<StateEntry> {
        vec![StateEntry::new(
            "state/table",
            StateValue::Bytes(if RACE { vec![2, 3] } else { vec![2] }),
        )]
    }
    fn validate_state(
        &self,
        params: &ParameterStore,
        custom: &[StateEntry],
    ) -> Result<(), ComponentStateError> {
        let Some(ParameterEventValue::Float(index)) =
            params.get_event_value(ParameterIndex::new(0))
        else {
            unreachable!()
        };
        if table(custom).is_some_and(|bytes| usize::from(index >= 0.5) < bytes.len()) {
            Ok(())
        } else {
            Err(ComponentStateError::new(std::io::Error::other(
                "invalid table address",
            )))
        }
    }
    fn state_migrations(&self) -> &[&dyn StateMigration<Error = ComponentStateError>] {
        &[&Rename]
    }
    fn activate(
        &self,
        _: &ActivationConfig<'_>,
        _: &ParameterStore,
        custom: &[StateEntry],
    ) -> Result<Self::Processor, Infallible> {
        Ok(TableDsp(table(custom).unwrap().to_vec()))
    }
}
struct TableDsp<const RACE: bool>(Vec<u8>);
impl<const RACE: bool> Drop for TableDsp<RACE> {
    fn drop(&mut self) {
        DROPS[usize::from(RACE)].fetch_add(1, Ordering::Relaxed);
    }
}
impl<const RACE: bool> Processor for TableDsp<RACE> {}
impl<const RACE: bool> Process<f32> for TableDsp<RACE> {
    fn process<B: ProcessBufferSource<f32> + ?Sized>(
        &mut self,
        block: &mut ProcessBlock<'_, '_, '_, f32, B>,
    ) {
        if RACE && BLOCK_ONCE.swap(false, Ordering::Relaxed) {
            let (entered, release) = DSP_GATE.get().unwrap();
            entered.send(()).unwrap();
            // This intentionally blocking race fixture is never allocation-
            // measured. Disconnect also releases DSP if a host assertion panics.
            let _ = release.lock().unwrap().recv();
        }
        let Some(ParameterEventValue::Float(base)) =
            block.parameters().get_event_value(ParameterIndex::new(0))
        else {
            unreachable!()
        };
        let events = block.parameter_events();
        for mut channel in block.channels() {
            let Some(output) = channel.output_mut() else {
                continue;
            };
            let mut cursor = events.float_cursor(ParameterIndex::new(0), base).unwrap();
            for (offset, sample) in output.iter_mut().enumerate() {
                let index =
                    usize::from(cursor.value_at(u32::try_from(offset).unwrap()).unwrap() >= 0.5);
                if let Some(value) = self.0.get(index) {
                    *sample = f32::from(*value);
                } else {
                    INCOMPATIBLE[usize::from(RACE)].fetch_add(1, Ordering::Relaxed);
                    *sample = f32::from(self.0[0]);
                }
            }
        }
    }
}
impl<const RACE: bool> ClapComponent for Table<RACE> {
    const CLAP_ID: &'static str = "test.custom-table-export";
    const CLAP_NAME: &'static str = "Resource Table";
    const CLAP_FEATURES: &'static [&'static std::ffi::CStr] = &[c"audio-effect", c"stereo"];
    const CLAP_PARAMETER_IDS: &'static [(&'static str, u32)] = &[("index", 11)];
    const CLAP_MAX_PARAMETER_EVENTS: u32 = 2;
}
struct Handlers;
struct Shared(Arc<AtomicUsize>);
struct Main;
struct Audio;
impl SharedHandler<'_> for Shared {
    fn request_restart(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
    fn request_process(&self) {}
    fn request_callback(&self) {}
}
impl MainThreadHandler<'_> for Main {}
impl AudioProcessorHandler<'_> for Audio {}
impl HostHandlers for Handlers {
    type Shared<'a> = Shared;
    type MainThread<'a> = Main;
    type AudioProcessor<'a> = Audio;
}
fn instance(entry: &PluginEntry, restarts: Arc<AtomicUsize>) -> PluginInstance<Handlers> {
    let desc = entry
        .get_factory::<PluginFactory>()
        .unwrap()
        .plugin_descriptor(0)
        .unwrap();
    PluginInstance::new(
        move |()| Shared(restarts),
        |_| Main,
        entry,
        desc.id().unwrap(),
        &HostInfo::new("custom-state", "", "", "").unwrap(),
    )
    .unwrap()
}
fn activate(plugin: &mut PluginInstance<Handlers>) -> StoppedPluginAudioProcessor<Handlers> {
    plugin
        .activate(
            |_, _| Audio,
            PluginAudioConfiguration {
                sample_rate: 48_000.0,
                min_frames_count: 1,
                max_frames_count: 4,
            },
        )
        .unwrap()
}
fn document(schema: u32, index: f64, bytes: Option<&[u8]>) -> Vec<u8> {
    let mut doc = StateDocument::new(ID, schema).unwrap();
    doc.insert(StateEntry::new(
        if schema == 1 {
            "parameter/old_index"
        } else {
            "parameter/index"
        },
        StateValue::Float(index),
    ))
    .unwrap();
    if let Some(bytes) = bytes {
        doc.insert(StateEntry::new(
            "state/table",
            StateValue::Bytes(bytes.to_vec()),
        ))
        .unwrap();
    }
    doc.encode().unwrap()
}
fn save(plugin: &mut PluginInstance<Handlers>, state: PluginState) -> Vec<u8> {
    let mut bytes = Vec::new();
    state.save(&plugin.plugin_handle(), &mut bytes).unwrap();
    bytes
}
fn load(plugin: &mut PluginInstance<Handlers>, state: PluginState, bytes: &[u8]) {
    state
        .load(&plugin.plugin_handle(), &mut Cursor::new(bytes))
        .unwrap();
}
fn event(time: u32, value: f64) -> EventBuffer {
    let mut events = EventBuffer::with_capacity(1);
    events.push(&ParamValueEvent::new(
        time,
        ClapId::new(11),
        Pckn::match_all(),
        value,
    ));
    events
}
fn input_channel(samples: &mut [f32; 4]) -> InputChannel<'_, f32> {
    InputChannel::variable(samples.as_mut_slice())
}
fn run(
    processor: StoppedPluginAudioProcessor<Handlers>,
    input: &EventBuffer,
    count: usize,
    measure: bool,
) -> (StoppedPluginAudioProcessor<Handlers>, f32) {
    let mut processor = processor.start_processing().unwrap();
    let mut main = [[0.0_f32; 4]; 2];
    let mut side = [[0.0_f32; 4]; 2];
    let mut outputs = [[0.0_f32; 4]; 2];
    let mut inputs = AudioPorts::with_capacity(4, 2);
    let mut ports = AudioPorts::with_capacity(2, 1);
    let audio = inputs.with_input_buffers([
        AudioPortBuffer {
            channels: AudioPortBufferType::f32_input_only(main.iter_mut().map(input_channel)),
            latency: 0,
        },
        AudioPortBuffer {
            channels: AudioPortBufferType::f32_input_only(side.iter_mut().map(input_channel)),
            latency: 0,
        },
    ]);
    let mut output = ports.with_output_buffers([AudioPortBuffer {
        channels: AudioPortBufferType::f32_output_only(
            outputs.iter_mut().map(<[f32; 4]>::as_mut_slice),
        ),
        latency: 0,
    }]);
    let mut out_events = EventBuffer::with_capacity(0);
    COUNT.set(measure);
    for _ in 0..count {
        let result = processor.process(
            &audio,
            &mut output,
            &input.as_input(),
            &mut out_events.as_output(),
            None,
            None,
        );
        if measure {
            processor.reset();
        }
        assert!(result.is_ok());
    }
    COUNT.set(false);
    (processor.stop_processing(), outputs[0][0])
}

#[test]
fn custom_defaults_migration_deferred_restart_and_reopen_are_complete() {
    let entry = PluginEntry::load_from_clack::<SingleComponentEntry<Table<false>>>(c"").unwrap();
    let restarts = Arc::new(AtomicUsize::new(0));
    let mut plugin = instance(&entry, Arc::clone(&restarts));
    let state = plugin
        .plugin_handle()
        .get_extension::<PluginState>()
        .unwrap();
    let initial = document(2, 0.0, Some(&[2]));
    assert_eq!(save(&mut plugin, state), initial);
    let mut processor = activate(&mut plugin);
    (processor, _) = run(processor, &EventBuffer::new(), 1, false);
    load(&mut plugin, state, &document(1, 1.0, Some(&[7, 9])));
    assert_eq!(restarts.load(Ordering::Relaxed), 1);
    let accepted = document(2, 1.0, Some(&[7, 9]));
    assert_eq!(save(&mut plugin, state), accepted);
    for rejected in [
        document(2, 1.0, Some(&[7])),
        document(2, 0.0, None),
        document(2, 0.0, Some(&[])),
    ] {
        assert!(
            state
                .load(&plugin.plugin_handle(), &mut Cursor::new(rejected))
                .is_err()
        );
        assert_eq!(save(&mut plugin, state), accepted);
        assert_eq!(restarts.load(Ordering::Relaxed), 1);
    }
    // Revert before the first callback can observe either load. Equality of
    // custom bytes must not revive the old activation's automation.
    load(&mut plugin, state, &initial);
    let output;
    (processor, output) = run(processor, &event(0, 1.0), 1, false);
    assert_eq!(output.to_bits(), 2.0_f32.to_bits());
    assert_eq!(INCOMPATIBLE[0].load(Ordering::Relaxed), 0);
    load(&mut plugin, state, &accepted);
    ALLOC.store(0, Ordering::Relaxed);
    DEALLOC.store(0, Ordering::Relaxed);
    let mut output;
    (processor, output) = run(processor, &event(0, 1.0), 1000, true);
    assert_eq!(output.to_bits(), 2.0_f32.to_bits());
    assert_eq!(ALLOC.load(Ordering::Relaxed), 0);
    assert_eq!(DEALLOC.load(Ordering::Relaxed), 0);
    assert_eq!(DROPS[0].load(Ordering::Relaxed), 0);
    assert_eq!(save(&mut plugin, state), accepted);
    let params = plugin
        .plugin_handle()
        .get_extension::<PluginParams>()
        .unwrap();
    params.flush_active(
        &mut processor.plugin_handle(),
        &event(0, 0.25).as_input(),
        &mut EventBuffer::new().as_output(),
    );
    assert_eq!(save(&mut plugin, state), document(2, 0.25, Some(&[7, 9])));
    // Reversion and reset cannot unfreeze an already gated activation either.
    load(&mut plugin, state, &initial);
    (processor, output) = run(processor, &event(0, 1.0), 1, false);
    assert_eq!(output.to_bits(), 2.0_f32.to_bits());
    let desired = document(2, 1.0, Some(&[11, 13]));
    load(&mut plugin, state, &desired);
    (processor, output) = run(processor, &event(0, 1.0), 1, false);
    assert_eq!(output.to_bits(), 2.0_f32.to_bits());
    assert_eq!(INCOMPATIBLE[0].load(Ordering::Relaxed), 0);
    assert_eq!(save(&mut plugin, state), desired);
    plugin.deactivate(processor);
    assert_eq!(DROPS[0].load(Ordering::Relaxed), 1);
    let processor = activate(&mut plugin);
    let (processor, output) = run(processor, &EventBuffer::new(), 1, false);
    assert_eq!(output.to_bits(), 13.0_f32.to_bits());
    plugin.deactivate(processor);
    let mut reopened = instance(&entry, Arc::new(AtomicUsize::new(0)));
    let state = reopened
        .plugin_handle()
        .get_extension::<PluginState>()
        .unwrap();
    load(&mut reopened, state, &desired);
    let processor = activate(&mut reopened);
    let (processor, output) = run(processor, &EventBuffer::new(), 1, false);
    assert_eq!(output.to_bits(), 13.0_f32.to_bits());
    assert_eq!(save(&mut reopened, state), desired);
    reopened.deactivate(processor);
}

#[test]
fn load_during_dsp_defeats_old_completion_and_freezes_last_old_block() {
    let (entered, ready) = sync_channel(1);
    let (release, resume) = sync_channel(1);
    DSP_GATE.set((entered, Mutex::new(resume))).unwrap();
    let entry = PluginEntry::load_from_clack::<SingleComponentEntry<Table<true>>>(c"").unwrap();
    let mut plugin = instance(&entry, Arc::new(AtomicUsize::new(0)));
    let state = plugin
        .plugin_handle()
        .get_extension::<PluginState>()
        .unwrap();
    let processor = activate(&mut plugin);
    let desired = document(2, 0.0, Some(&[7]));
    BLOCK_ONCE.store(true, Ordering::Relaxed);
    let processor = std::thread::scope(|scope| {
        // Own the sender inside the scope closure: unwinding drops it before
        // scope joins, so a failed load/save assertion cannot strand the worker.
        let unblock = release;
        let worker = scope.spawn(move || run(processor, &event(0, 1.0), 1, false));
        ready.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(save(&mut plugin, state), document(2, 0.0, Some(&[2, 3])));
        load(&mut plugin, state, &desired);
        assert_eq!(save(&mut plugin, state), desired);
        unblock.send(()).unwrap();
        let (processor, output) = worker.join().unwrap();
        assert_eq!(output.to_bits(), 3.0_f32.to_bits());
        processor
    });
    assert_eq!(save(&mut plugin, state), desired);
    let (processor, output) = run(processor, &event(0, 0.0), 1, false);
    assert_eq!(
        output.to_bits(),
        3.0_f32.to_bits(),
        "the in-flight old-generation block may establish the frozen DSP baseline"
    );
    assert_eq!(save(&mut plugin, state), desired);
    assert_eq!(INCOMPATIBLE[1].load(Ordering::Relaxed), 0);
    plugin.deactivate(processor);
    let processor = activate(&mut plugin);
    let (processor, output) = run(processor, &EventBuffer::new(), 1, false);
    assert_eq!(output.to_bits(), 7.0_f32.to_bits());
    plugin.deactivate(processor);
}
