//! External multi-output fixture through the public Chassis runtime.
//!
//! This proves output buses remain semantically distinct through dense port
//! identity even when callback channel order does not match schema order.

use std::{convert::Infallible, num::NonZeroU32};

use chassis_runtime::transport::TransportSnapshot;
use chassis_runtime::{
    audio::{
        AudioIoConfiguration, AudioIoConfigurationSpec, AudioIoPolicy, AudioPortDescriptor,
        AudioPortDirection, AudioPortIndex, AudioPortKey, AudioPortRole, ChannelLayout,
        ConfiguredAudioPort,
    },
    automation::ParameterEvents,
    buffer::{ChannelBuffer, OutputEndpoint},
    process::{ActivationConfig, ProcessBlock, ProcessConfig, ProcessContext, ProcessMode},
    runtime::{Component, InstanceRuntime, Process, Processor},
    schema::ComponentSchema,
};

const MAIN_OUTPUT: AudioPortKey = AudioPortKey::new("audio.main.out");
const AUX_OUTPUT: AudioPortKey = AudioPortKey::new("audio.aux.out");

static AUDIO_PORTS: &[AudioPortDescriptor] = &[
    AudioPortDescriptor::new(
        MAIN_OUTPUT,
        "Main Output",
        AudioPortDirection::Output,
        AudioPortRole::Main,
        false,
    ),
    AudioPortDescriptor::new(
        AUX_OUTPUT,
        "Aux Output",
        AudioPortDirection::Output,
        AudioPortRole::Auxiliary,
        false,
    ),
];

fn audio_policy() -> AudioIoPolicy {
    AudioIoPolicy::enumerated(vec![AudioIoConfigurationSpec::new(vec![
        ConfiguredAudioPort::new(MAIN_OUTPUT, ChannelLayout::Stereo),
        ConfiguredAudioPort::new(AUX_OUTPUT, ChannelLayout::Mono),
    ])])
}

struct MultiOutput;

impl Component for MultiOutput {
    type Processor = MultiOutputProcessor;
    type ActivationError = Infallible;

    fn schema(&self) -> Result<ComponentSchema, chassis_runtime::schema::ComponentSchemaError> {
        ComponentSchema::unidentified(AUDIO_PORTS.to_vec(), audio_policy(), vec![], vec![])
    }

    fn activate(
        &self,
        config: &ActivationConfig<'_>,
        _parameters: &chassis_runtime::parameters::ParameterStore,
        _custom_state: &[chassis_runtime::state::StateEntry],
    ) -> Result<Self::Processor, Self::ActivationError> {
        Ok(MultiOutputProcessor {
            main: config
                .schema()
                .audio_port_index(&MAIN_OUTPUT)
                .expect("main output resolves before activation"),
            aux: config
                .schema()
                .audio_port_index(&AUX_OUTPUT)
                .expect("aux output resolves before activation"),
        })
    }
}

struct MultiOutputProcessor {
    main: AudioPortIndex,
    aux: AudioPortIndex,
}

impl Processor for MultiOutputProcessor {}

impl Process<f32> for MultiOutputProcessor {
    fn process(
        &mut self,
        block: &mut ProcessBlock<'_, f32>,
    ) -> Result<(), chassis_runtime::runtime::ProcessError> {
        block.for_each_channel(|mut channel| {
            let Some(endpoint) = channel.output_endpoint() else {
                return;
            };
            let value = if endpoint.port_index() == self.main {
                0.125
            } else if endpoint.port_index() == self.aux {
                0.75
            } else {
                return;
            };
            channel
                .output_mut()
                .expect("multi-output fixture receives output-only channels")
                .fill(value);
        });
        Ok(())
    }
}

fn process_config() -> ProcessConfig {
    ProcessConfig::new(
        48_000.0,
        NonZeroU32::new(1),
        NonZeroU32::new(64).expect("maximum is non-zero"),
        0,
    )
    .expect("multi-output process configuration is valid")
}

fn process_context(frame_count: u32) -> ProcessContext<'static> {
    ProcessContext::new(
        ProcessMode::Realtime,
        &TransportSnapshot::UNKNOWN,
        ParameterEvents::empty_for_block(frame_count),
    )
}

fn assert_samples(actual: &[f32], expected: f32) {
    for sample in actual {
        assert!((*sample - expected).abs() <= f32::EPSILON);
    }
}

#[test]
fn distinct_output_buses_are_routed_by_dense_port_identity() {
    let component = MultiOutput;
    let mut runtime: InstanceRuntime<MultiOutputProcessor> =
        InstanceRuntime::for_component(&component).expect("multi-output schema is valid");
    let main = runtime
        .audio_port_index(&MAIN_OUTPUT)
        .expect("main output has a dense runtime index");
    let aux = runtime
        .audio_port_index(&AUX_OUTPUT)
        .expect("aux output has a dense runtime index");
    assert_ne!(main, aux);

    let audio = [
        ConfiguredAudioPort::new(MAIN_OUTPUT, ChannelLayout::Stereo),
        ConfiguredAudioPort::new(AUX_OUTPUT, ChannelLayout::Mono),
    ];
    runtime
        .activate(
            &component,
            process_config(),
            AudioIoConfiguration::new(&audio),
        )
        .expect("multi-output component activates");

    let mut main_left = [-1.0_f32; 8];
    let mut main_right = [-1.0_f32; 8];
    let mut aux_mono = [-1.0_f32; 8];

    // Deliberately present the auxiliary bus first and reverse the main channels.
    // Product DSP must route by dense endpoint identity, not callback position.
    let mut buffers = [
        ChannelBuffer::output_only(OutputEndpoint::new(aux, 0), &mut aux_mono, 8)
            .expect("aux output buffer is valid"),
        ChannelBuffer::output_only(OutputEndpoint::new(main, 1), &mut main_right, 8)
            .expect("main right output buffer is valid"),
        ChannelBuffer::output_only(OutputEndpoint::new(main, 0), &mut main_left, 8)
            .expect("main left output buffer is valid"),
    ];

    runtime
        .process(8, process_context(8), &mut buffers)
        .expect("multi-output process block succeeds");

    assert_samples(&main_left, 0.125);
    assert_samples(&main_right, 0.125);
    assert_samples(&aux_mono, 0.75);
}
