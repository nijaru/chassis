//! In-process CLAP host proof for coherent state save while audio processing is active.

use std::{
    convert::Infallible,
    sync::{Arc, Barrier, OnceLock},
};

use chassis_clap::{ClapComponent, SingleComponentEntry};
use chassis_runtime::{
    parameters::ParameterDescriptor,
    process::{ActivationConfig, ProcessBlock},
    runtime::{Component, Process, Processor},
    state::{StateDocument, StateValue},
};
use clack_extensions::state::PluginState;
use clack_host::{
    events::event_types::ParamValueEvent, factory::plugin::PluginFactory, prelude::*,
};

const FIRST_CLAP_ID: u32 = 17;
const SECOND_CLAP_ID: u32 = 18;

struct Barriers {
    entered: Arc<Barrier>,
    release: Arc<Barrier>,
}

static BARRIERS: OnceLock<Barriers> = OnceLock::new();

struct StateProbe {
    parameters: Vec<ParameterDescriptor>,
}

impl Default for StateProbe {
    fn default() -> Self {
        Self {
            parameters: vec![
                ParameterDescriptor::float("first", "First", 0.0, 1.0, 0.0)
                    .expect("first parameter is valid"),
                ParameterDescriptor::float("second", "Second", 0.0, 1.0, 0.0)
                    .expect("second parameter is valid"),
            ],
        }
    }
}

impl Component for StateProbe {
    type Processor = StateProcessor;
    type ActivationError = Infallible;

    fn schema(
        &self,
    ) -> Result<
        chassis_runtime::schema::ComponentSchema,
        chassis_runtime::schema::ComponentSchemaError,
    > {
        chassis_runtime::schema::ComponentSchema::stereo_effect_with_state(
            chassis_runtime::schema::ComponentId::new("org.nijaru.chassis.semantic.crates.chassis-clap.tests.host-active-state-save.stateprobe")
                .expect("semantic component identity is valid"),
            chassis_runtime::schema::StateSchemaVersion::new(1),
            self.parameters.clone(),
        )
    }

    fn activate(
        &self,
        _config: &ActivationConfig<'_>,
        _parameters: &chassis_runtime::parameters::ParameterStore,
        _custom_state: &[chassis_runtime::state::StateEntry],
    ) -> Result<Self::Processor, Self::ActivationError> {
        let barriers = BARRIERS.get().expect("test barriers are initialized");
        Ok(StateProcessor {
            entered: Arc::clone(&barriers.entered),
            release: Arc::clone(&barriers.release),
        })
    }
}

struct StateProcessor {
    entered: Arc<Barrier>,
    release: Arc<Barrier>,
}

impl Processor for StateProcessor {}

impl Process<f32> for StateProcessor {
    fn process(
        &mut self,
        _block: &mut ProcessBlock<'_, f32>,
    ) -> Result<(), chassis_runtime::runtime::ProcessError> {
        self.entered.wait();
        self.release.wait();
        Ok(())
    }
}

impl ClapComponent for StateProbe {
    const CLAP_DESCRIPTOR: chassis_clap::ClapDescriptor = chassis_clap::ClapDescriptor {
        features: &[c"audio-effect", c"stereo"],
        version: env!("CARGO_PKG_VERSION"),
        ..chassis_clap::ClapDescriptor::new(
            "org.nijaru.chassis.active-state-probe",
            "Chassis Active State Probe",
        )
    };
    const CLAP_PARAMETER_IDS: &'static [(&'static str, u32)] =
        &[("first", FIRST_CLAP_ID), ("second", SECOND_CLAP_ID)];
    const CLAP_MAX_PARAMETER_EVENTS: u32 = 2;
}

struct TestHostShared;
struct TestHostMainThread;
struct TestHostAudioProcessor;
struct TestHostHandlers;

impl SharedHandler<'_> for TestHostShared {
    fn request_restart(&self) {}
    fn request_process(&self) {}
    fn request_callback(&self) {}
}

impl MainThreadHandler<'_> for TestHostMainThread {}
impl AudioProcessorHandler<'_> for TestHostAudioProcessor {}

impl HostHandlers for TestHostHandlers {
    type Shared<'a> = TestHostShared;
    type MainThread<'a> = TestHostMainThread;
    type AudioProcessor<'a> = TestHostAudioProcessor;
}

fn save_state(plugin: &mut PluginInstance<TestHostHandlers>, state: PluginState) -> Vec<u8> {
    let mut bytes = Vec::new();
    state
        .save(&plugin.plugin_handle(), &mut bytes)
        .expect("active state save succeeds");
    bytes
}

fn parameter_value(bytes: &[u8], key: &str) -> f64 {
    let document = StateDocument::decode(bytes).expect("saved Chassis state decodes");
    let full_key = format!("parameter/{key}");
    let entry = document
        .entries()
        .iter()
        .find(|entry| entry.key() == full_key)
        .expect("saved state contains parameter");
    match entry.value() {
        StateValue::Float(value) => *value,
        _ => panic!("saved parameter has unexpected value type"),
    }
}

#[allow(unsafe_code)]
fn run_automation_callback(
    processor: StoppedPluginAudioProcessor<TestHostHandlers>,
) -> StoppedPluginAudioProcessor<TestHostHandlers> {
    let mut processor = processor.start_processing().expect("processing starts");
    let mut main_inputs = [[0.0_f32; 1]; 2];
    let mut sidechain_inputs = [[0.0_f32; 1]; 2];
    let mut outputs = [[0.0_f32; 1]; 2];
    let mut main_pointers = main_inputs.each_mut().map(|channel| channel.as_mut_ptr());
    let mut sidechain_pointers = sidechain_inputs
        .each_mut()
        .map(|channel| channel.as_mut_ptr());
    let mut output_pointers = outputs.each_mut().map(|channel| channel.as_mut_ptr());
    let buffer = |data32| clap_sys::audio_buffer::clap_audio_buffer {
        data32,
        data64: std::ptr::null_mut(),
        channel_count: 2,
        latency: 0,
        constant_mask: 0,
    };
    let input_tables = [
        buffer(main_pointers.as_mut_ptr()),
        buffer(sidechain_pointers.as_mut_ptr()),
    ];
    let mut output_tables = [buffer(output_pointers.as_mut_ptr())];
    // Avoid the pinned development host's multiport pointer-table reborrow.
    // SAFETY: caller-owned tables and disjoint initialized one-frame channels
    // remain stable through process; inputs are read-only, outputs exclusive.
    // No backing channel or pointer table is reborrowed while these views live.
    let input_audio = unsafe { InputAudioBuffers::from_raw_buffers(&input_tables, 1) };
    // SAFETY: the same storage/lifetime guarantees apply to the exclusive output.
    let mut output_audio = unsafe { OutputAudioBuffers::from_raw_buffers(&mut output_tables, 1) };
    let mut input_events = EventBuffer::with_capacity(2);
    input_events.push(&ParamValueEvent::new(
        0,
        ClapId::new(FIRST_CLAP_ID),
        Pckn::match_all(),
        0.25,
    ));
    input_events.push(&ParamValueEvent::new(
        0,
        ClapId::new(SECOND_CLAP_ID),
        Pckn::match_all(),
        0.75,
    ));
    let mut output_events = EventBuffer::with_capacity(0);

    processor
        .process(
            &input_audio,
            &mut output_audio,
            &input_events.as_input(),
            &mut output_events.as_output(),
            None,
            None,
        )
        .expect("automation callback succeeds");
    processor.stop_processing()
}

#[test]
fn active_save_linearizes_before_or_after_audio_endpoint_publication() {
    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    BARRIERS
        .set(Barriers {
            entered: Arc::clone(&entered),
            release: Arc::clone(&release),
        })
        .unwrap_or_else(|_| panic!("test barriers initialize once"));

    let entry = PluginEntry::load_from_clack::<SingleComponentEntry<StateProbe>>(c"")
        .expect("static state probe entry loads");
    let descriptor = entry
        .get_factory::<PluginFactory>()
        .expect("plugin factory exists")
        .plugin_descriptor(0)
        .expect("state probe descriptor exists");
    let host_info = HostInfo::new("chassis-test", "", "", "").expect("host info is valid");
    let mut plugin = PluginInstance::<TestHostHandlers>::new(
        |()| TestHostShared,
        |_| TestHostMainThread,
        &entry,
        descriptor.id().expect("state probe id is valid"),
        &host_info,
    )
    .expect("state probe instantiates");
    let state = plugin
        .plugin_handle()
        .get_extension::<PluginState>()
        .expect("state probe exports CLAP state");

    let processor = plugin
        .activate(
            |_, _| TestHostAudioProcessor,
            PluginAudioConfiguration {
                sample_rate: 48_000.0,
                min_frames_count: 1,
                max_frames_count: 1,
            },
        )
        .expect("state probe activates");
    let initial = save_state(&mut plugin, state);
    assert!((parameter_value(&initial, "first") - 0.0).abs() <= f64::EPSILON);
    assert!((parameter_value(&initial, "second") - 0.0).abs() <= f64::EPSILON);

    let processor = std::thread::scope(|scope| {
        let worker = scope.spawn(move || run_automation_callback(processor));
        entered.wait();
        let during = save_state(&mut plugin, state);
        assert_eq!(during, initial);
        release.wait();
        worker.join().expect("audio worker returns processor")
    });

    let after = save_state(&mut plugin, state);
    assert!((parameter_value(&after, "first") - 0.25).abs() <= f64::EPSILON);
    assert!((parameter_value(&after, "second") - 0.75).abs() <= f64::EPSILON);
    assert_ne!(after, initial);

    let processor = std::thread::scope(|scope| {
        let worker = scope.spawn(move || run_automation_callback(processor));
        entered.wait();
        state
            .load(&plugin.plugin_handle(), &mut initial.as_slice())
            .expect("state load succeeds during processing");
        release.wait();
        worker.join().expect("audio worker returns processor")
    });
    assert_eq!(
        save_state(&mut plugin, state),
        initial,
        "newer loaded state must win over in-flight automation"
    );
    plugin.deactivate(processor);
}
