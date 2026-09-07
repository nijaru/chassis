//! In-process CLAP host proof for coherent state save while audio processing is active.

use std::{
    convert::Infallible,
    sync::{Arc, Barrier, OnceLock},
};

use chassis_clap::{ClapStereoEffect, SingleComponentEntry};
use chassis_core::{
    parameters::ParameterDescriptor,
    process::{ActivationConfig, ProcessBlock, ProcessBufferSource},
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

    fn parameter_descriptors(&self) -> &[ParameterDescriptor] {
        &self.parameters
    }

    fn activate(
        &self,
        _config: &ActivationConfig<'_>,
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
    fn process<B>(&mut self, _block: &mut ProcessBlock<'_, '_, '_, f32, B>)
    where
        B: ProcessBufferSource<f32> + ?Sized,
    {
        self.entered.wait();
        self.release.wait();
    }
}

impl ClapStereoEffect for StateProbe {
    const CLAP_ID: &'static str = "org.nijaru.chassis.active-state-probe";
    const CLAP_NAME: &'static str = "Chassis Active State Probe";
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

fn run_automation_callback(
    processor: StoppedPluginAudioProcessor<TestHostHandlers>,
) -> StoppedPluginAudioProcessor<TestHostHandlers> {
    let mut processor = processor.start_processing().expect("processing starts");
    let mut main_inputs = [vec![0.0_f32; 1], vec![0.0_f32; 1]];
    let mut sidechain_inputs = [vec![0.0_f32; 1], vec![0.0_f32; 1]];
    let mut outputs = [vec![0.0_f32; 1], vec![0.0_f32; 1]];
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
        channels: AudioPortBufferType::f32_output_only(outputs.iter_mut().map(Vec::as_mut_slice)),
        latency: 0,
    }]);
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
