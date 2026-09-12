//! Multi-layout + sidechain fixture through the public Chassis runtime.
//!
//! This proves whole-I/O policy selection is activation-scoped: one immutable
//! schema can accept multiple configurations, construct resources from the
//! accepted layout, and reject structurally valid combinations outside policy.

use std::{convert::Infallible, num::NonZeroU32};

use chassis_core::{
    audio::{
        AudioIoConfiguration, AudioIoConfigurationSpec, AudioIoPolicy, AudioPortDescriptor,
        AudioPortIndex, ChannelLayout, ConfiguredAudioPort, PortDirection, PortKey, PortRole,
    },
    automation::ParameterEvents,
    buffer::{ChannelBuffer, InputEndpoint, OutputEndpoint},
    process::{
        ActivationConfig, ProcessBlock, ProcessBufferSource, ProcessChannel, ProcessConfig,
        ProcessContext, ProcessMode, TransportSnapshot,
    },
    runtime::{ActivateError, Component, InstanceRuntime, Process, Processor},
    schema::ComponentSchema,
};

const MAIN_INPUT: PortKey = PortKey::new("audio.main.in");
const MAIN_OUTPUT: PortKey = PortKey::new("audio.main.out");
const SIDECHAIN: PortKey = PortKey::new("audio.sidechain");

static AUDIO_PORTS: &[AudioPortDescriptor] = &[
    AudioPortDescriptor::new(
        MAIN_INPUT,
        "Main Input",
        PortDirection::Input,
        PortRole::Main,
        false,
    ),
    AudioPortDescriptor::new(
        MAIN_OUTPUT,
        "Main Output",
        PortDirection::Output,
        PortRole::Main,
        false,
    ),
    AudioPortDescriptor::new(
        SIDECHAIN,
        "Sidechain",
        PortDirection::Input,
        PortRole::Sidechain,
        true,
    ),
];

fn audio_policy() -> AudioIoPolicy {
    AudioIoPolicy::enumerated(vec![
        AudioIoConfigurationSpec::new(vec![
            ConfiguredAudioPort::new(MAIN_INPUT, ChannelLayout::Mono),
            ConfiguredAudioPort::new(MAIN_OUTPUT, ChannelLayout::Mono),
        ]),
        AudioIoConfigurationSpec::new(vec![
            ConfiguredAudioPort::new(MAIN_INPUT, ChannelLayout::Stereo),
            ConfiguredAudioPort::new(MAIN_OUTPUT, ChannelLayout::Stereo),
        ]),
        AudioIoConfigurationSpec::new(vec![
            ConfiguredAudioPort::new(MAIN_INPUT, ChannelLayout::Stereo),
            ConfiguredAudioPort::new(MAIN_OUTPUT, ChannelLayout::Stereo),
            ConfiguredAudioPort::new(SIDECHAIN, ChannelLayout::Mono),
        ]),
    ])
}

struct LayoutEffect;

impl Component for LayoutEffect {
    type Processor = LayoutProcessor;
    type ActivationError = Infallible;

    fn schema(&self) -> Result<ComponentSchema, chassis_core::schema::ComponentSchemaError> {
        ComponentSchema::unidentified(AUDIO_PORTS.to_vec(), audio_policy(), vec![], vec![])
    }

    fn activate(
        &self,
        config: &ActivationConfig<'_>,
    ) -> Result<Self::Processor, Self::ActivationError> {
        let main_layout = config
            .audio_io()
            .ports()
            .iter()
            .find(|port| port.key.as_str() == MAIN_INPUT.as_str())
            .expect("accepted configuration contains main input")
            .layout;
        let sidechain_active = config
            .audio_io()
            .ports()
            .iter()
            .any(|port| port.key.as_str() == SIDECHAIN.as_str());
        let layout_factor = match main_layout {
            ChannelLayout::Mono => 1.0,
            ChannelLayout::Stereo => 2.0,
            _ => unreachable!("fixture policy accepts only mono/stereo main layouts"),
        };
        let factor = layout_factor + if sidechain_active { 1.0 } else { 0.0 };

        Ok(LayoutProcessor {
            main_input: config
                .schema()
                .audio_port_index(&MAIN_INPUT)
                .expect("main input resolves before activation"),
            main_output: config
                .schema()
                .audio_port_index(&MAIN_OUTPUT)
                .expect("main output resolves before activation"),
            factor,
        })
    }
}

struct LayoutProcessor {
    main_input: AudioPortIndex,
    main_output: AudioPortIndex,
    factor: f32,
}

impl Processor for LayoutProcessor {}

impl Process<f32> for LayoutProcessor {
    fn process<B>(&mut self, block: &mut ProcessBlock<'_, '_, '_, f32, B>)
    where
        B: ProcessBufferSource<f32> + ?Sized,
    {
        for mut channel in block.channels() {
            let is_main = channel
                .input_endpoint()
                .is_some_and(|endpoint| endpoint.port_index() == self.main_input)
                && channel
                    .output_endpoint()
                    .is_some_and(|endpoint| endpoint.port_index() == self.main_output);
            if !is_main {
                continue;
            }
            for sample in channel
                .make_in_place()
                .expect("main channels are paired input/output")
            {
                *sample *= self.factor;
            }
        }
    }
}

fn process_config() -> ProcessConfig {
    ProcessConfig::new(
        48_000.0,
        NonZeroU32::new(1),
        NonZeroU32::new(64).expect("maximum is non-zero"),
        0,
    )
    .expect("layout effect process configuration is valid")
}

fn process_context(frame_count: u32) -> ProcessContext<'static> {
    ProcessContext::new(
        ProcessMode::Realtime,
        TransportSnapshot::unknown(),
        ParameterEvents::empty_for_block(frame_count),
    )
}

fn assert_samples(actual: &[f32], expected: &[f32]) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        assert!((*actual - *expected).abs() <= f32::EPSILON);
    }
}

#[test]
fn reactivation_changes_layout_resources_and_rejects_unsupported_combinations() {
    let component = LayoutEffect;
    let mut runtime: InstanceRuntime<LayoutProcessor> =
        InstanceRuntime::for_component(&component).expect("layout effect schema is valid");
    let main_input = runtime
        .audio_port_index(&MAIN_INPUT)
        .expect("main input has a dense runtime index");
    let main_output = runtime
        .audio_port_index(&MAIN_OUTPUT)
        .expect("main output has a dense runtime index");
    let sidechain = runtime
        .audio_port_index(&SIDECHAIN)
        .expect("sidechain has a dense runtime index");

    let mono = [
        ConfiguredAudioPort::new(MAIN_INPUT, ChannelLayout::Mono),
        ConfiguredAudioPort::new(MAIN_OUTPUT, ChannelLayout::Mono),
    ];
    runtime
        .activate(
            &component,
            process_config(),
            AudioIoConfiguration::new(&mono),
        )
        .expect("mono configuration activates");

    let mut mono_samples = [1.0_f32, -0.5, 0.25, 2.0];
    let mut mono_buffers = [ChannelBuffer::in_place(
        InputEndpoint::new(main_input, 0),
        OutputEndpoint::new(main_output, 0),
        &mut mono_samples,
        4,
    )
    .expect("mono main buffer is valid")];
    runtime
        .process(4, process_context(4), &mut mono_buffers)
        .expect("mono process block succeeds");
    assert_samples(&mono_samples, &[1.0, -0.5, 0.25, 2.0]);
    runtime
        .deactivate()
        .expect("mono activation deactivates cleanly");

    let stereo_sidechain = [
        ConfiguredAudioPort::new(MAIN_INPUT, ChannelLayout::Stereo),
        ConfiguredAudioPort::new(MAIN_OUTPUT, ChannelLayout::Stereo),
        ConfiguredAudioPort::new(SIDECHAIN, ChannelLayout::Mono),
    ];
    runtime
        .activate(
            &component,
            process_config(),
            AudioIoConfiguration::new(&stereo_sidechain),
        )
        .expect("stereo + sidechain configuration activates");

    let mut left = [1.0_f32, 0.5, -0.5, 2.0];
    let mut right = [-1.0_f32, 0.25, 1.0, -0.25];
    let detector = [0.1_f32, 0.2, 0.3, 0.4];
    let mut stereo_buffers = [
        ChannelBuffer::input_only(InputEndpoint::new(sidechain, 0), &detector, 4)
            .expect("sidechain input buffer is valid"),
        ChannelBuffer::in_place(
            InputEndpoint::new(main_input, 1),
            OutputEndpoint::new(main_output, 1),
            &mut right,
            4,
        )
        .expect("right main buffer is valid"),
        ChannelBuffer::in_place(
            InputEndpoint::new(main_input, 0),
            OutputEndpoint::new(main_output, 0),
            &mut left,
            4,
        )
        .expect("left main buffer is valid"),
    ];
    runtime
        .process(4, process_context(4), &mut stereo_buffers)
        .expect("stereo + sidechain process block succeeds");
    assert_samples(&left, &[3.0, 1.5, -1.5, 6.0]);
    assert_samples(&right, &[-3.0, 0.75, 3.0, -0.75]);
    runtime
        .deactivate()
        .expect("stereo + sidechain activation deactivates cleanly");

    let unsupported = [
        ConfiguredAudioPort::new(MAIN_INPUT, ChannelLayout::Mono),
        ConfiguredAudioPort::new(MAIN_OUTPUT, ChannelLayout::Mono),
        ConfiguredAudioPort::new(SIDECHAIN, ChannelLayout::Mono),
    ];
    assert!(matches!(
        runtime.activate(
            &component,
            process_config(),
            AudioIoConfiguration::new(&unsupported),
        ),
        Err(ActivateError::UnsupportedAudioIo)
    ));
    assert!(!runtime.is_active());
}
