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

use chassis_clap::{ClapComponent, F32AndF64, SingleComponentEntry};
use chassis_runtime::{
    audio::{AudioPortIndex, MAIN_INPUT, MAIN_OUTPUT},
    parameters::{ChoiceOption, ParameterDescriptor, ParameterValue},
    process::{ActivationConfig, ProcessBlock, ProcessBufferSource, ProcessChannel},
    runtime::{Component, LatencySamples, Process, Processor},
};
use clack_extensions::{
    latency::{HostLatency, HostLatencyImpl, PluginLatency},
    log::{HostLog, HostLogImpl, LogSeverity},
    params::PluginParams,
};
use clack_host::{
    events::event_types::ParamValueEvent, factory::plugin::PluginFactory, prelude::*,
};

const TRIM_CLAP_ID: u32 = 7;
const MAX_PARAMETER_EVENTS: u32 = 64;

struct CountingAllocator;

thread_local! {
    static COUNT_THIS_THREAD: Cell<bool> = const { Cell::new(false) };
}

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static DEALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static REALTIME_LOG_CALLS: AtomicU64 = AtomicU64::new(0);

fn counting_this_thread() -> bool {
    COUNT_THIS_THREAD.try_with(Cell::get).unwrap_or(false)
}

// Test allocator instrumentation, separate from the adapter's raw audio boundary.
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
                ParameterDescriptor::integer("steps", "Steps", -4, 4, 0).unwrap(),
                ParameterDescriptor::boolean("bypass", "Bypass", true).unwrap(),
                ParameterDescriptor::choice(
                    "mode",
                    "Mode",
                    vec![
                        ChoiceOption::new("clean-id", "Clean").unwrap(),
                        ChoiceOption::new("warm-id", "Warm").unwrap(),
                        ChoiceOption::new("driven-id", "Driven").unwrap(),
                    ],
                    "clean-id",
                )
                .unwrap(),
            ],
        }
    }
}

impl Component for ProbeComponent {
    type Processor = ProbeProcessor;
    type ActivationError = Infallible;

    fn schema(
        &self,
    ) -> Result<
        chassis_runtime::schema::ComponentSchema,
        chassis_runtime::schema::ComponentSchemaError,
    > {
        chassis_runtime::schema::ComponentSchema::stereo_effect_with_state(
            chassis_runtime::schema::ComponentId::new("org.nijaru.chassis.semantic.crates.chassis-clap.tests.host-conformance.probecomponent")
                .expect("semantic component identity is valid"),
            chassis_runtime::schema::StateSchemaVersion::new(1),
            self.parameters.clone(),
        )
    }

    fn activate(
        &self,
        config: &ActivationConfig<'_>,
        _parameters: &chassis_runtime::parameters::ParameterStore,
        _custom_state: &[chassis_runtime::state::StateEntry],
    ) -> Result<Self::Processor, Self::ActivationError> {
        let latency = if config.process().sample_rate() >= 88_200.0 {
            128
        } else {
            64
        };
        Ok(ProbeProcessor {
            latency: LatencySamples::new(latency),
            main_input: config
                .schema()
                .audio_port_index(&MAIN_INPUT)
                .expect("default effect schema has main input"),
            main_output: config
                .schema()
                .audio_port_index(&MAIN_OUTPUT)
                .expect("default effect schema has main output"),
        })
    }
}

struct ProbeProcessor {
    latency: LatencySamples,
    main_input: AudioPortIndex,
    main_output: AudioPortIndex,
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
                .is_some_and(|endpoint| endpoint.port_index() == self.main_input)
                && channel
                    .output_endpoint()
                    .is_some_and(|endpoint| endpoint.port_index() == self.main_output)
            {
                let mut trim = events
                    .float_cursor(trim_index, base)
                    .expect("trim cursor is valid");
                for (offset, sample) in channel
                    .copy_input_to_output()
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
                .is_some_and(|endpoint| endpoint.port_index() == self.main_input)
                && channel
                    .output_endpoint()
                    .is_some_and(|endpoint| endpoint.port_index() == self.main_output)
            {
                let mut trim = events
                    .float_cursor(trim_index, base)
                    .expect("trim cursor is valid");
                for (offset, sample) in channel
                    .copy_input_to_output()
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

impl ClapComponent for ProbeComponent {
    const CLAP_ID: &'static str = "org.nijaru.chassis.host-probe";
    const CLAP_NAME: &'static str = "Chassis Host Probe";
    const CLAP_FEATURES: &'static [&'static std::ffi::CStr] = &[c"audio-effect", c"stereo"];
    const CLAP_PARAMETER_IDS: &'static [(&'static str, u32)] = &[
        ("trim", TRIM_CLAP_ID),
        ("steps", 8),
        ("bypass", 9),
        ("mode", 10),
    ];
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

impl HostLogImpl for TestHostShared {
    fn log(&self, _severity: LogSeverity, _message: &str) {
        if counting_this_thread() {
            REALTIME_LOG_CALLS.fetch_add(1, Ordering::Relaxed);
        }
    }
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
        builder.register::<HostLog>();
    }
}

fn input_events() -> EventBuffer {
    let mut events = EventBuffer::with_capacity(
        usize::try_from(MAX_PARAMETER_EVENTS).expect("event bound fits usize"),
    );
    for offset in 0..MAX_PARAMETER_EVENTS {
        let (id, plain) = match offset {
            61 => (8, -2.7),
            62 => (9, 0.99),
            63 => (10, 1.99),
            _ => (TRIM_CLAP_ID, 0.5),
        };
        events.push(&ParamValueEvent::new(
            offset,
            ClapId::new(id),
            Pckn::match_all(),
            plain,
        ));
    }
    events
}

fn measure_f32_adapter_path(
    processor: &mut StartedPluginAudioProcessor<TestHostHandlers>,
    events: &EventBuffer,
    output_events: &mut EventBuffer,
) {
    let mut main_inputs = [vec![1.0_f32; 64], vec![1.0_f32; 64]];
    for channel in &mut main_inputs {
        for sample in channel.iter_mut().step_by(2) {
            *sample = 2.0;
        }
    }
    let mut sidechain_inputs = [vec![0.0_f32; 64], vec![0.0_f32; 64]];
    let mut outputs = [vec![0.0_f32; 64], vec![0.0_f32; 64]];
    let mut input_ports = AudioPorts::with_capacity(4, 2);
    let mut output_ports = AudioPorts::with_capacity(2, 1);
    {
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
        output_audio.as_raw_buffers()[0].constant_mask = u64::MAX;
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
        assert_eq!(output_audio.as_raw_buffers()[0].constant_mask, 0);

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
    }

    assert_eq!(ALLOCATIONS.load(Ordering::Relaxed), 0);
    assert_eq!(DEALLOCATIONS.load(Ordering::Relaxed), 0);
    for channel in &outputs {
        assert!(channel.iter().enumerate().all(|(index, sample)| {
            (*sample - if index % 2 == 0 { 1.0 } else { 0.5 }).abs() <= f32::EPSILON
        }));
    }
}

fn measure_f64_adapter_path(
    processor: &mut StartedPluginAudioProcessor<TestHostHandlers>,
    events: &EventBuffer,
    output_events: &mut EventBuffer,
) {
    let mut main_inputs = [vec![1.0_f64; 64], vec![1.0_f64; 64]];
    for channel in &mut main_inputs {
        for sample in channel.iter_mut().step_by(2) {
            *sample = 2.0;
        }
    }
    let mut sidechain_inputs = [vec![0.0_f64; 64], vec![0.0_f64; 64]];
    let mut outputs = [vec![0.0_f64; 64], vec![0.0_f64; 64]];
    let mut input_ports = AudioPorts::with_capacity(4, 2);
    let mut output_ports = AudioPorts::with_capacity(2, 1);
    {
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
        output_audio.as_raw_buffers()[0].constant_mask = u64::MAX;
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
        assert_eq!(output_audio.as_raw_buffers()[0].constant_mask, 0);

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
    }

    assert_eq!(ALLOCATIONS.load(Ordering::Relaxed), 0);
    assert_eq!(DEALLOCATIONS.load(Ordering::Relaxed), 0);
    for channel in &outputs {
        assert!(channel.iter().enumerate().all(|(index, sample)| {
            (*sample - if index % 2 == 0 { 1.0 } else { 0.5 }).abs() <= f64::EPSILON
        }));
    }
}

fn exercise_raw_aliases(processor: &mut StartedPluginAudioProcessor<TestHostHandlers>) {
    let mut main = [[1.0f32; 65]; 2];
    let mut sidechain = [[0.0f32; 64]; 2];
    let mut main_table = main.each_mut().map(|samples| samples.as_mut_ptr());
    let mut sidechain_table = sidechain.each_mut().map(|samples| samples.as_mut_ptr());
    let mut output_table = main_table;
    let mut dummy = [0.0f32; 64];
    let mut ports = AudioPorts::with_capacity(1, 1);
    let prototype = ports
        .with_input_buffers([AudioPortBuffer {
            channels: AudioPortBufferType::f32_input_only([InputChannel::variable(&mut dummy)]),
            latency: 0,
        }])
        .as_raw_buffers()[0];
    let mut inputs = [prototype; 2];
    let mut outputs = [prototype];
    inputs[0].channel_count = 2;
    inputs[0].data32 = main_table.as_mut_ptr();
    inputs[1].channel_count = 2;
    inputs[1].data32 = sidechain_table.as_mut_ptr();
    outputs[0].channel_count = 2;
    outputs[0].data32 = output_table.as_mut_ptr();
    let events = EventBuffer::with_capacity(0);
    let mut output_events = EventBuffer::with_capacity(0);
    for partial in [false, true] {
        if partial {
            output_table[0] = main_table[0].wrapping_add(1);
        }
        // SAFETY: descriptors, tables, and initialized sample allocations remain
        // live and cover 64 frames. The first callback is legal declared in-place;
        // the second deliberately probes rejection of partial sample overlap.
        // Neither host helper creates sample references from these pointers.
        let input_audio = unsafe { InputAudioBuffers::from_raw_buffers(&inputs, 64) };
        let mut output_audio = unsafe { OutputAudioBuffers::from_raw_buffers(&mut outputs, 64) };
        ALLOCATIONS.store(0, Ordering::Relaxed);
        DEALLOCATIONS.store(0, Ordering::Relaxed);
        // Measure both acceptance and rejection through the actual CLAP entry.
        COUNT_THIS_THREAD.with(|counting| counting.set(true));
        let result = processor.process(
            &input_audio,
            &mut output_audio,
            &events.as_input(),
            &mut output_events.as_output(),
            None,
            None,
        );
        COUNT_THIS_THREAD.with(|counting| counting.set(false));
        assert_eq!(result.is_ok(), !partial);
        assert_eq!(ALLOCATIONS.load(Ordering::Relaxed), 0);
        assert_eq!(DEALLOCATIONS.load(Ordering::Relaxed), 0);
    }
    for channel in main {
        assert!(
            channel[..64]
                .iter()
                .all(|sample| (*sample - 0.5).abs() <= f32::EPSILON)
        );
        assert!((channel[64] - 1.0).abs() <= f32::EPSILON);
    }
}

#[test]
fn full_adapter_process_path_is_allocation_free_at_event_bound_for_f32_and_f64() {
    REALTIME_LOG_CALLS.store(0, Ordering::Relaxed);
    let entry =
        PluginEntry::load_from_clack::<SingleComponentEntry<ProbeComponent, F32AndF64>>(c"")
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
        move |()| TestHostShared {
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

    let handle = plugin.plugin_handle();
    let params = handle.get_extension::<PluginParams>().unwrap();
    for (id, plain, text, normalized) in [
        (8, -2.7, c"-2", -2.0),
        (9, 0.99, c"0", 0.0),
        (10, 0.99, c"Clean", 0.0),
        (10, 1.99, c"Warm", 1.0),
        (10, 2.0, c"Driven", 2.0),
    ] {
        let mut buffer = [0; 32];
        assert_eq!(
            params
                .value_to_text(&handle, ClapId::new(id), plain, &mut buffer)
                .unwrap(),
            text.to_bytes()
        );
        assert_eq!(
            params.text_to_value(&handle, ClapId::new(id), text),
            Some(normalized)
        );
    }
    assert_eq!(
        params.text_to_value(&handle, ClapId::new(8), c"-2.7"),
        Some(-2.0)
    );
    assert_eq!(
        params.text_to_value(&handle, ClapId::new(9), c"0.99"),
        Some(0.0)
    );
    let flush_events = [
        ParamValueEvent::new(0, ClapId::new(8), Pckn::match_all(), -1.7),
        ParamValueEvent::new(0, ClapId::new(9), Pckn::match_all(), 1.0),
        ParamValueEvent::new(0, ClapId::new(10), Pckn::match_all(), 2.0),
    ];
    params.flush(
        &mut plugin.inactive_plugin_handle().unwrap(),
        &InputEvents::from_buffer(&flush_events),
        &mut OutputEvents::void(),
    );
    for (id, expected) in [(8, -1.0), (9, 1.0), (10, 2.0)] {
        assert_eq!(
            params.get_value(&plugin.plugin_handle(), ClapId::new(id)),
            Some(expected)
        );
    }

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
    measure_f32_adapter_path(&mut processor, &events, &mut output_events);
    measure_f64_adapter_path(&mut processor, &events, &mut output_events);
    for (id, expected) in [(8, -2.0), (9, 0.0), (10, 1.0)] {
        assert_eq!(
            params.get_value(&plugin.plugin_handle(), ClapId::new(id)),
            Some(expected)
        );
    }
    // Rejection terminates the run, so perform this probe last.
    exercise_raw_aliases(&mut processor);
    assert_eq!(REALTIME_LOG_CALLS.load(Ordering::Relaxed), 0);
    plugin.deactivate(processor.stop_processing());
}

#[test]
fn latency_reactivation_and_audio_thread_transfer_follow_clap_lifecycle() {
    let entry =
        PluginEntry::load_from_clack::<SingleComponentEntry<ProbeComponent, F32AndF64>>(c"")
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
        move |()| TestHostShared {
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
    let entry =
        PluginEntry::load_from_clack::<SingleComponentEntry<ProbeComponent, F32AndF64>>(c"")
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
            move |()| TestHostShared {
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
