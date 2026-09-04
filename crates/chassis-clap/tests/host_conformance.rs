#![allow(unsafe_code)]

//! In-process CLAP host qualification for adapter realtime and lifecycle behavior.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    convert::Infallible,
    sync::{
        Arc,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
};

use chassis_clap::{ClapStereoEffect, SingleComponentEntryWithF64};
use chassis_core::{
    audio::{MAIN_INPUT, MAIN_OUTPUT},
    parameters::{ParameterDescriptor, ParameterValue},
    process::{ActivationConfig, ProcessBlock, ProcessBufferSource, ProcessChannel},
    runtime::{Component, LatencySamples, Process, Processor},
};
use clack_extensions::latency::{HostLatency, HostLatencyImpl, PluginLatency};
use clack_host::{
    events::event_types::ParamValueEvent,
    factory::plugin::PluginFactory,
    prelude::*,
};

const TRIM_CLAP_ID: u32 = 7;
const MAX_PARAMETER_EVENTS: u32 = 64;

struct CountingAllocator;

thread_local! {
    static COUNT_THIS_THREAD: Cell<bool> = const { Cell::new(false) };
}

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static DEALLOCATIONS: AtomicU64 = AtomicU64::new(0);

fn counting_this_thread() -> bool {
    COUNT_THIS_THREAD.try_with(Cell::get).unwrap_or(false)
}

// Test instrumentation only. Production Chassis crates remain safe Rust.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if counting_this_thread() {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: the layout is forwarded unchanged to the system allocator.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if counting_this_thread() {
            DEALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: the allocation/layout pair is forwarded unchanged.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if counting_this_thread() {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: the layout is forwarded unchanged to the system allocator.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if counting_this_thread() {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            DEALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: the allocation/layout pair and requested size are forwarded unchanged.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

struct ProbeComponent {
    parameters: Vec<ParameterDescriptor>,
}

impl Default for ProbeComponent {
    fn default() -> Self {
        Self {
            parameters: vec![
                ParameterDescriptor::float("trim", "Trim", 0.0, 1.0, 1.0)
                    .expect("probe parameter is valid"),
            ],
        }
    }
}

impl Component for ProbeComponent {
    type Processor = ProbeProcessor;
    type ActivationError = Infallible;

    fn parameter_descriptors(&self) -> &[ParameterDescriptor] {
        &self.parameters
    }

    fn activate(
        &self,
        config: &ActivationConfig<'_>,
    ) -> Result<Self::Processor, Self::ActivationError> {
        let latency = if config.process().sample_rate() >= 88_200.0 {
            128
        } else {
            64
        };
        Ok(ProbeProcessor {
            latency: LatencySamples::new(latency),
        })
    }
}

struct ProbeProcessor {
    latency: LatencySamples,
}

impl Processor for ProbeProcessor {
    fn latency(&self) -> LatencySamples {
        self.latency
    }
}

impl Process<f32> for ProbeProcessor {
    #[allow(clippy::cast_possible_truncation)]
    fn process<B>(&mut self, block: &mut ProcessBlock<'_, '_, '_, f32, B>)
    where
        B: ProcessBufferSource<f32> + ?Sized,
    {
        let trim_index = block.parameters().index("trim").expect("trim exists");
        let base = match block.parameters().get("trim") {
            Some(ParameterValue::Float(value)) => *value,
            _ => 1.0,
        };
        let events = block.parameter_events();
        for mut channel in block.channels() {
            if channel
                .input_endpoint()
                .is_some_and(|endpoint| endpoint.port() == MAIN_INPUT)
                && channel
                    .output_endpoint()
                    .is_some_and(|endpoint| endpoint.port() == MAIN_OUTPUT)
            {
                let mut trim = events
                    .float_cursor(trim_index, base)
                    .expect("trim cursor is valid");
                for (offset, sample) in channel
                    .make_in_place()
                    .expect("main channel has input and output")
                    .iter_mut()
                    .enumerate()
                {
                    *sample *= trim
                        .value_at(u32::try_from(offset).expect("test block fits in u32"))
                        .expect("offset is inside the block") as f32;
                }
            }
        }
    }
}

impl Process<f64> for ProbeProcessor {
    fn process<B>(&mut self, block: &mut ProcessBlock<'_, '_, '_, f64, B>)
    where
        B: ProcessBufferSource<f64> + ?Sized,
    {
        let trim_index = block.parameters().index("trim").expect("trim exists");
        let base = match block.parameters().get("trim") {
            Some(ParameterValue::Float(value)) => *value,
            _ => 1.0,
        };
        let events = block.parameter_events();
        for mut channel in block.channels() {
            if channel
                .input_endpoint()
                .is_some_and(|endpoint| endpoint.port() == MAIN_INPUT)
                && channel
                    .output_endpoint()
                    .is_some_and(|endpoint| endpoint.port() == MAIN_OUTPUT)
            {
                let mut trim = events
                    .float_cursor(trim_index, base)
                    .expect("trim cursor is valid");
                for (offset, sample) in channel
                    .make_in_place()
                    .expect("main channel has input and output")
                    .iter_mut()
                    .enumerate()
                {
                    *sample *= trim
                        .value_at(u32::try_from(offset).expect("test block fits in u32"))
                        .expect("offset is inside the block");
                }
            }
        }
    }
}

impl ClapStereoEffect for ProbeComponent {
    const CLAP_ID: &'static str = "org.nijaru.chassis.host-probe";
    const CLAP_NAME: &'static str = "Chassis Host Probe";
    const CLAP_PARAMETER_IDS: &'static [(&'static str, u32)] = &[("trim", TRIM_CLAP_ID)];
    const CLAP_MAX_PARAMETER_EVENTS: u32 = MAX_PARAMETER_EVENTS;
}

#[derive(Clone)]
struct TestHostShared {
    latency_changes: Arc<AtomicU32>,
}

struct TestHostMainThread {
    latency_changes: Arc<AtomicU32>,
}

struct TestHostAudioProcessor;
struct TestHostHandlers;

impl SharedHandler<'_> for TestHostShared {
    fn request_restart(&self) {}
    fn request_process(&self) {}
    fn request_callback(&self) {}
}

impl MainThreadHandler<'_> for TestHostMainThread {}
impl AudioProcessorHandler<'_> for TestHostAudioProcessor {}

impl HostLatencyImpl for TestHostMainThread {
    fn changed(&self) {
        self.latency_changes.fetch_add(1, Ordering::Relaxed);
    }
}

impl HostHandlers for TestHostHandlers {
    type Shared<'a> = TestHostShared;
    type MainThread<'a> = TestHostMainThread;
    type AudioProcessor<'a> = TestHostAudioProcessor;

    fn declare_extensions(builder: &mut HostExtensions<Self>, _shared: &Self::Shared<'_>) {
        builder.register::<HostLatency>();
    }
}

fn input_events() -> EventBuffer {
    let mut events = EventBuffer::with_capacity(
        usize::try_from(MAX_PARAMETER_EVENTS).expect("event bound fits usize"),
    );
    for offset in 0..MAX_PARAMETER_EVENTS {
        events.push(&ParamValueEvent::new(
            offset,
            ClapId::new(TRIM_CLAP_ID),
            Pckn::match_all(),
            0.5,
        ));
    }
    events
}

#[test]
fn full_adapter_process_path_is_allocation_free_at_event_bound_for_f32_and_f64() {
    let entry = PluginEntry::load_from_clack::<SingleComponentEntryWithF64<ProbeComponent>>(c"")
        .expect("static probe entry loads");
    let descriptor = entry
        .get_factory::<PluginFactory>()
        .expect("plugin factory exists")
        .plugin_descriptor(0)
        .expect("probe descriptor exists");
    let host_info = HostInfo::new("chassis-test", "", "", "").expect("host info is valid");
    let latency_changes = Arc::new(AtomicU32::new(0));
    let shared_changes = Arc::clone(&latency_changes);
    let mut plugin = PluginInstance::<TestHostHandlers>::new(
        move |_| TestHostShared {
            latency_changes: shared_changes,
        },
        |shared| TestHostMainThread {
            latency_changes: Arc::clone(&shared.latency_changes),
        },
        &entry,
        descriptor.id().expect("probe id is valid"),
        &host_info,
    )
    .expect("probe plugin instantiates");

    let processor = plugin
        .activate(
            |_, _| TestHostAudioProcessor,
            PluginAudioConfiguration {
                sample_rate: 48_000.0,
                min_frames_count: MAX_PARAMETER_EVENTS,
                max_frames_count: MAX_PARAMETER_EVENTS,
            },
        )
        .expect("probe activation succeeds");
    assert_eq!(latency_changes.load(Ordering::Relaxed), 1);

    let mut processor = processor.start_processing().expect("processing starts");
    let events = input_events();
    let mut output_events = EventBuffer::with_capacity(1);

    {
        let mut main_inputs = [vec![1.0_f32; 64], vec![1.0_f32; 64]];
        let mut sidechain_inputs = [vec![0.0_f32; 64], vec![0.0_f32; 64]];
        let mut outputs = [vec![0.0_f32; 64], vec![0.0_f32; 64]];
        let mut input_ports = AudioPorts::with_capacity(4, 2);
        let mut output_ports = AudioPorts::with_capacity(2, 1);
        let input_audio = input_ports.with_input_buffers([
            AudioPortBuffer {
                channels: AudioPortBufferType::f32_input_only(
                    main_inputs.iter_mut().map(InputChannel::variable),
                ),
                latency: 0,
            },
            AudioPortBuffer {
                channels: AudioPortBufferType::f32_input_only(
                    sidechain_inputs.iter_mut().map(InputChannel::variable),
                ),
                latency: 0,
            },
        ]);
        let mut output_audio = output_ports.with_output_buffers([AudioPortBuffer {
            channels: AudioPortBufferType::f32_output_only(
                outputs.iter_mut().map(Vec::as_mut_slice),
            ),
            latency: 0,
        }]);

        processor
            .process(
                &input_audio,
                &mut output_audio,
                &events.as_input(),
                &mut output_events.as_output(),
                None,
                None,
            )
            .expect("warmup f32 callback succeeds");

        ALLOCATIONS.store(0, Ordering::Relaxed);
        DEALLOCATIONS.store(0, Ordering::Relaxed);
        COUNT_THIS_THREAD.with(|counting| counting.set(true));
        for _ in 0..1_000 {
            processor
                .process(
                    &input_audio,
                    &mut output_audio,
                    &events.as_input(),
                    &mut output_events.as_output(),
                    None,
                    None,
                )
                .expect("measured f32 callback succeeds");
        }
        COUNT_THIS_THREAD.with(|counting| counting.set(false));

        assert_eq!(ALLOCATIONS.load(Ordering::Relaxed), 0);
        assert_eq!(DEALLOCATIONS.load(Ordering::Relaxed), 0);
        drop(output_audio);
        for channel in &outputs {
            assert!(channel.iter().all(|sample| *sample == 0.5));
        }
    }

    {
        let mut main_inputs = [vec![1.0_f64; 64], vec![1.0_f64; 64]];
        let mut sidechain_inputs = [vec![0.0_f64; 64], vec![0.0_f64; 64]];
        let mut outputs = [vec![0.0_f64; 64], vec![0.0_f64; 64]];
        let mut input_ports = AudioPorts::with_capacity(4, 2);
        let mut output_ports = AudioPorts::with_capacity(2, 1);
        let input_audio = input_ports.with_input_buffers([
            AudioPortBuffer {
                channels: AudioPortBufferType::f64_input_only(
                    main_inputs.iter_mut().map(InputChannel::variable),
                ),
                latency: 0,
            },
            AudioPortBuffer {
                channels: AudioPortBufferType::f64_input_only(
                    sidechain_inputs.iter_mut().map(InputChannel::variable),
                ),
                latency: 0,
            },
        ]);
        let mut output_audio = output_ports.with_output_buffers([AudioPortBuffer {
            channels: AudioPortBufferType::f64_output_only(
                outputs.iter_mut().map(Vec::as_mut_slice),
            ),
            latency: 0,
        }]);

        processor
            .process(
                &input_audio,
                &mut output_audio,
                &events.as_input(),
                &mut output_events.as_output(),
                None,
                None,
            )
            .expect("warmup f64 callback succeeds");

        ALLOCATIONS.store(0, Ordering::Relaxed);
        DEALLOCATIONS.store(0, Ordering::Relaxed);
        COUNT_THIS_THREAD.with(|counting| counting.set(true));
        for _ in 0..1_000 {
            processor
                .process(
                    &input_audio,
                    &mut output_audio,
                    &events.as_input(),
                    &mut output_events.as_output(),
                    None,
                    None,
                )
                .expect("measured f64 callback succeeds");
        }
        COUNT_THIS_THREAD.with(|counting| counting.set(false));

        assert_eq!(ALLOCATIONS.load(Ordering::Relaxed), 0);
        assert_eq!(DEALLOCATIONS.load(Ordering::Relaxed), 0);
        drop(output_audio);
        for channel in &outputs {
            assert!(channel.iter().all(|sample| *sample == 0.5));
        }
    }

    plugin.deactivate(processor.stop_processing());
}

#[test]
fn latency_reactivation_and_audio_thread_transfer_follow_clap_lifecycle() {
    let entry = PluginEntry::load_from_clack::<SingleComponentEntryWithF64<ProbeComponent>>(c"")
        .expect("static probe entry loads");
    let descriptor = entry
        .get_factory::<PluginFactory>()
        .expect("plugin factory exists")
        .plugin_descriptor(0)
        .expect("probe descriptor exists");
    let host_info = HostInfo::new("chassis-test", "", "", "").expect("host info is valid");
    let latency_changes = Arc::new(AtomicU32::new(0));
    let shared_changes = Arc::clone(&latency_changes);
    let mut plugin = PluginInstance::<TestHostHandlers>::new(
        move |_| TestHostShared {
            latency_changes: shared_changes,
        },
        |shared| TestHostMainThread {
            latency_changes: Arc::clone(&shared.latency_changes),
        },
        &entry,
        descriptor.id().expect("probe id is valid"),
        &host_info,
    )
    .expect("probe plugin instantiates");

    let plugin_handle = plugin.plugin_handle();
    let latency_extension = plugin_handle
        .get_extension::<PluginLatency>()
        .expect("probe exports CLAP latency");
    assert_eq!(latency_extension.get(&plugin_handle), 0);

    let processor = plugin
        .activate(
            |_, _| TestHostAudioProcessor,
            PluginAudioConfiguration {
                sample_rate: 44_100.0,
                min_frames_count: 1,
                max_frames_count: 32,
            },
        )
        .expect("first activation succeeds");
    let plugin_handle = plugin.plugin_handle();
    assert_eq!(latency_extension.get(&plugin_handle), 64);
    assert_eq!(latency_changes.load(Ordering::Relaxed), 1);

    let processor = std::thread::scope(|scope| {
        scope
            .spawn(move || {
                let processor = processor.start_processing().expect("processing starts");
                processor.stop_processing()
            })
            .join()
            .expect("audio thread returns processor")
    });
    plugin.deactivate(processor);

    let processor = plugin
        .activate(
            |_, _| TestHostAudioProcessor,
            PluginAudioConfiguration {
                sample_rate: 96_000.0,
                min_frames_count: 1,
                max_frames_count: 512,
            },
        )
        .expect("reactivation with new resource bounds succeeds");
    let plugin_handle = plugin.plugin_handle();
    assert_eq!(latency_extension.get(&plugin_handle), 128);
    assert_eq!(latency_changes.load(Ordering::Relaxed), 2);
    plugin.deactivate(processor);
}

#[test]
fn repeated_instances_can_be_created_and_destroyed_without_activation() {
    let entry = PluginEntry::load_from_clack::<SingleComponentEntryWithF64<ProbeComponent>>(c"")
        .expect("static probe entry loads");
    let descriptor = entry
        .get_factory::<PluginFactory>()
        .expect("plugin factory exists")
        .plugin_descriptor(0)
        .expect("probe descriptor exists");
    let host_info = HostInfo::new("chassis-test", "", "", "").expect("host info is valid");

    for _ in 0..32 {
        let latency_changes = Arc::new(AtomicU32::new(0));
        let shared_changes = Arc::clone(&latency_changes);
        let plugin = PluginInstance::<TestHostHandlers>::new(
            move |_| TestHostShared {
                latency_changes: shared_changes,
            },
            |shared| TestHostMainThread {
                latency_changes: Arc::clone(&shared.latency_changes),
            },
            &entry,
            descriptor.id().expect("probe id is valid"),
            &host_info,
        )
        .expect("probe instance constructs");
        assert!(!plugin.is_active());
        drop(plugin);
    }
}
