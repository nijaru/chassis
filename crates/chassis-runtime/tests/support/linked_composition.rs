//! Public-runtime linked DSP and caller-owned composition, under heap counters.

#![forbid(unsafe_code)]

use std::{convert::Infallible, num::NonZeroU32, sync::atomic::Ordering};

use chassis_runtime::{
    audio::{
        AudioIoConfiguration, AudioIoConfigurationSpec, AudioIoPolicy, AudioPortIndex,
        ChannelLayout, ConfiguredAudioPort, DEFAULT_EFFECT_PORTS, MAIN_INPUT, MAIN_OUTPUT,
        SIDECHAIN_INPUT,
    },
    automation::{ParameterEvent, ParameterEventValue, ParameterEvents},
    buffer::{ChannelBuffer, InputEndpoint, OutputEndpoint},
    parameters::{FloatParameter, ParameterDescriptor, ParameterStore},
    process::{ActivationConfig, ProcessBlock, ProcessConfig, ProcessContext, ProcessMode},
    runtime::{Component, InstanceRuntime, Process, ProcessError, Processor},
    schema::{ComponentId, ComponentSchema, ComponentSchemaError, StateSchemaVersion},
    state::StateEntry,
    transport::TransportSnapshot,
};

use super::{ALLOCATIONS, COUNT_THIS_THREAD, DEALLOCATIONS};

const FRAMES: u32 = 8;
const CONFIGURATION: [ConfiguredAudioPort; 3] = [
    ConfiguredAudioPort::new(MAIN_INPUT, ChannelLayout::Stereo),
    ConfiguredAudioPort::new(MAIN_OUTPUT, ChannelLayout::Stereo),
    ConfiguredAudioPort::new(SIDECHAIN_INPUT, ChannelLayout::Stereo),
];

#[derive(Clone, Copy)]
struct Ports {
    input: AudioPortIndex,
    output: AudioPortIndex,
    sidechain: AudioPortIndex,
}
impl Ports {
    fn resolve(schema: &ComponentSchema) -> Self {
        Self {
            input: schema.audio_port_index(&MAIN_INPUT).unwrap(),
            output: schema.audio_port_index(&MAIN_OUTPUT).unwrap(),
            sidechain: schema.audio_port_index(&SIDECHAIN_INPUT).unwrap(),
        }
    }
}

struct LinkedEffect;
impl Component for LinkedEffect {
    type Processor = LinkedProcessor;
    type ActivationError = Infallible;

    fn schema(&self) -> Result<ComponentSchema, ComponentSchemaError> {
        ComponentSchema::new(
            ComponentId::new("test.linked-composition").unwrap(),
            StateSchemaVersion::new(1),
            DEFAULT_EFFECT_PORTS.to_vec(),
            AudioIoPolicy::enumerated(vec![AudioIoConfigurationSpec::new(CONFIGURATION.to_vec())]),
            vec![],
            vec![
                ParameterDescriptor::float("trim", "Trim", 0.0, 1.0, 0.5)
                    .unwrap()
                    .with_global_modulation()
                    .unwrap(),
                ParameterDescriptor::float("width", "Width", 0.0, 2.0, 1.0).unwrap(),
            ],
        )
    }

    fn activate(
        &self,
        config: &ActivationConfig<'_>,
        parameters: &ParameterStore,
        _: &[StateEntry],
    ) -> Result<Self::Processor, Infallible> {
        Ok(LinkedProcessor {
            ports: Ports::resolve(config.schema()),
            trim: parameters.float_parameter("trim").unwrap(),
            width: parameters.float_parameter("width").unwrap(),
        })
    }
}

struct LinkedProcessor {
    ports: Ports,
    trim: FloatParameter,
    width: FloatParameter,
}
impl Processor for LinkedProcessor {}
impl Process<f64> for LinkedProcessor {
    fn process(&mut self, block: &mut ProcessBlock<'_, f64>) -> Result<(), ProcessError> {
        let mut trim = self.trim.cursor(block)?;
        let mut width = self.width.cursor(block)?;
        block.with_channels(|channels| {
            let (mut left, mut right, mut side_left, mut side_right) = (None, None, None, None);
            for channel in channels {
                match channel {
                    ChannelBuffer::Separate {
                        input,
                        input_samples,
                        output,
                        output_samples,
                    } if input.port_index() == self.ports.input
                        && output.port_index() == self.ports.output =>
                    {
                        assert_eq!(input.channel(), output.channel());
                        match input.channel() {
                            0 => left = Some((input_samples, output_samples)),
                            1 => right = Some((input_samples, output_samples)),
                            _ => unreachable!("fixture policy admits only stereo"),
                        }
                    }
                    ChannelBuffer::InputOnly { input, samples }
                        if input.port_index() == self.ports.sidechain =>
                    {
                        match input.channel() {
                            0 => side_left = Some(samples),
                            1 => side_right = Some(samples),
                            _ => unreachable!("fixture sidechain is stereo"),
                        }
                    }
                    _ => panic!("fixture requires separate main I/O and input-only sidechain"),
                }
            }
            let (left_in, left_out) = left.unwrap();
            let (right_in, right_out) = right.unwrap();
            let (side_left, side_right) = (side_left.unwrap(), side_right.unwrap());
            for i in 0..left_in.len() {
                let frame = u32::try_from(i).unwrap();
                let mid = f64::midpoint(left_in[i], right_in[i]);
                let side = (left_in[i] - right_in[i]) * 0.5 * width.value_at(frame)?;
                let detector = side_left[i].abs().max(side_right[i].abs());
                let gain = trim.value_at(frame)? * if detector > 0.5 { 0.5 } else { 1.0 };
                left_out[i] = (mid + side) * gain;
                right_out[i] = (mid - side) * gain;
            }
            Ok(())
        })
    }
}

fn active() -> (InstanceRuntime<LinkedProcessor>, Ports) {
    let mut runtime = InstanceRuntime::for_component(&LinkedEffect).unwrap();
    runtime
        .activate(
            &LinkedEffect,
            ProcessConfig::new(
                48_000.0,
                NonZeroU32::new(1),
                NonZeroU32::new(FRAMES).unwrap(),
                8,
            )
            .unwrap(),
            AudioIoConfiguration::new(&CONFIGURATION),
        )
        .unwrap();
    let ports = Ports::resolve(runtime.schema());
    (runtime, ports)
}

fn render(
    runtime: &mut InstanceRuntime<LinkedProcessor>,
    ports: Ports,
    inputs: [&[f64]; 4],
    outputs: [&mut [f64]; 2],
    events: &[ParameterEvent],
) {
    let [left_in, right_in, side_left, side_right] = inputs;
    let [left_out, right_out] = outputs;
    // Deliberately not schema or channel order: the processor routes endpoints.
    let mut channels = [
        ChannelBuffer::input_only(InputEndpoint::new(ports.sidechain, 1), side_right, FRAMES)
            .unwrap(),
        ChannelBuffer::separate(
            InputEndpoint::new(ports.input, 1),
            right_in,
            OutputEndpoint::new(ports.output, 1),
            right_out,
            FRAMES,
        )
        .unwrap(),
        ChannelBuffer::input_only(InputEndpoint::new(ports.sidechain, 0), side_left, FRAMES)
            .unwrap(),
        ChannelBuffer::separate(
            InputEndpoint::new(ports.input, 0),
            left_in,
            OutputEndpoint::new(ports.output, 0),
            left_out,
            FRAMES,
        )
        .unwrap(),
    ];
    let context = ProcessContext::new(
        ProcessMode::Realtime,
        &TransportSnapshot::UNKNOWN,
        ParameterEvents::new(events, FRAMES, 8).unwrap(),
    );
    runtime.process(FRAMES, context, &mut channels).unwrap();
}

fn assert_samples(actual: [f64; 8], expected: [f64; 8]) {
    assert_eq!(actual.map(f64::to_bits), expected.map(f64::to_bits));
}

pub(super) fn qualify() {
    let (mut first, first_ports) = active();
    let (mut second, second_ports) = active();
    let trim = first.parameters().float_parameter("trim").unwrap();
    let width = first.parameters().float_parameter("width").unwrap();
    let second_trim = second.parameters().float_parameter("trim").unwrap();
    let events = [
        ParameterEvent::set(0, trim.index(), ParameterEventValue::Float(0.5)),
        ParameterEvent::modulation(0, trim.index(), 0.0),
        ParameterEvent::set(0, width.index(), ParameterEventValue::Float(1.0)),
        ParameterEvent::set(2, width.index(), ParameterEventValue::Float(0.0)),
        ParameterEvent::set(4, trim.index(), ParameterEventValue::Float(0.75)),
        ParameterEvent::set(5, width.index(), ParameterEventValue::Float(1.0)),
        ParameterEvent::modulation(6, trim.index(), 0.125),
    ];
    let left_in = [2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0];
    let right_in = [0.0, 2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0];
    let side_left = [0.0, 0.25, 0.0, 0.75, 0.0, 0.0, 0.125, 0.0];
    let side_right = [0.0, 0.0, 0.75, 0.0, 0.0, 0.5, 0.0, 1.0];
    let originals = [left_in, right_in, side_left, side_right];
    let silent_sidechain = [0.0; 8];
    let (mut intermediate_left, mut intermediate_right) = ([f64::NAN; 8], [f64::NAN; 8]);
    let (mut final_left, mut final_right) = ([f64::NAN; 8], [f64::NAN; 8]);

    ALLOCATIONS.store(0, Ordering::Relaxed);
    DEALLOCATIONS.store(0, Ordering::Relaxed);
    COUNT_THIS_THREAD.with(|counting| counting.set(true));
    for _ in 0..1_000 {
        render(
            &mut first,
            first_ports,
            [&left_in, &right_in, &side_left, &side_right],
            [&mut intermediate_left, &mut intermediate_right],
            &events,
        );
        render(
            &mut second,
            second_ports,
            [
                &intermediate_left,
                &intermediate_right,
                &silent_sidechain,
                &silent_sidechain,
            ],
            [&mut final_left, &mut final_right],
            &[],
        );
        // Hand-calculated binary fractions: width collapses at frame 2,
        // trim steps at 4, width returns at 5, modulation steps at 6.
        assert_samples(
            intermediate_left,
            [1.0, 2.0, 1.25, 1.75, 6.75, 9.0, 12.25, 7.0],
        );
        assert_samples(
            intermediate_right,
            [0.0, 1.0, 1.25, 1.75, 6.75, 7.5, 10.5, 6.125],
        );
        assert_samples(final_left, [0.5, 1.0, 0.625, 0.875, 3.375, 4.5, 6.125, 3.5]);
        assert_samples(
            final_right,
            [0.0, 0.5, 0.625, 0.875, 3.375, 3.75, 5.25, 3.0625],
        );
    }
    COUNT_THIS_THREAD.with(|counting| counting.set(false));
    assert_eq!(ALLOCATIONS.load(Ordering::Relaxed), 0);
    assert_eq!(DEALLOCATIONS.load(Ordering::Relaxed), 0);
    for (actual, original) in [left_in, right_in, side_left, side_right]
        .into_iter()
        .zip(originals)
    {
        assert_samples(actual, original);
    }
    assert_samples(silent_sidechain, [0.0; 8]);
    assert_eq!(
        trim.value(first.parameters()).unwrap().to_bits(),
        0.75_f64.to_bits()
    );
    assert_eq!(first.modulation(trim.index()), Some(0.125));
    assert_eq!(
        second_trim.value(second.parameters()).unwrap().to_bits(),
        0.5_f64.to_bits()
    );
    assert_eq!(second.modulation(second_trim.index()), Some(0.0));
    first.deactivate().unwrap();
    second.deactivate().unwrap();
}
