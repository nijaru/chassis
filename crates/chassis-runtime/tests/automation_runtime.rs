//! Automation completion through the public instance runtime and state APIs.

use std::{convert::Infallible, num::NonZeroU32};

use chassis_runtime::{
    audio::{AudioPortIndex, DEFAULT_EFFECT_CONFIGURATION},
    automation::{ParameterEvent, ParameterEventValue, ParameterEvents},
    buffer::{ChannelBuffer, InputEndpoint, OutputEndpoint},
    parameters::{
        ChoiceIndex, ChoiceOption, ParameterAutomationError, ParameterDescriptor, ParameterIndex,
        ParameterKind, ParameterValue, ParameterValueError,
    },
    process::{
        ActivationConfig, ProcessBlock, ProcessBlockError, ProcessBufferSource, ProcessChannel,
        ProcessConfig, ProcessContext, ProcessMode, TransportSnapshot,
    },
    runtime::{Component, InstanceProcessError, InstanceRuntime, Process, Processor},
    schema::{ComponentId, ComponentSchema, ComponentSchemaError, StateSchemaVersion},
    state::{StateDocument, StateLimits, StateValue},
};

struct Effect;
struct Gain;

impl Component for Effect {
    type Processor = Gain;
    type ActivationError = Infallible;

    fn schema(&self) -> Result<ComponentSchema, ComponentSchemaError> {
        ComponentSchema::stereo_effect_with_state(
            ComponentId::new("org.nijaru.chassis.automation-test").expect("valid identity"),
            StateSchemaVersion::new(1),
            vec![
                ParameterDescriptor::float("gain", "Gain", 0.0, 2.0, 1.0).unwrap(),
                ParameterDescriptor::integer("count", "Count", 0, 8, 0).unwrap(),
                ParameterDescriptor::boolean("mute", "Mute", false).unwrap(),
                ParameterDescriptor::choice(
                    "mode",
                    "Mode",
                    vec![
                        ChoiceOption::new("clean", "Clean").unwrap(),
                        ChoiceOption::new("warm", "Warm").unwrap(),
                    ],
                    "clean",
                )
                .unwrap(),
            ],
        )
    }

    fn activate(
        &self,
        _: &ActivationConfig<'_>,
        _: &chassis_runtime::parameters::ParameterStore,
        _: &[chassis_runtime::state::StateEntry],
    ) -> Result<Gain, Infallible> {
        Ok(Gain)
    }
}

impl Processor for Gain {}

impl Process<f64> for Gain {
    fn process<B>(&mut self, block: &mut ProcessBlock<'_, '_, '_, f64, B>)
    where
        B: ProcessBufferSource<f64> + ?Sized,
    {
        let ParameterValue::Float(initial) = block
            .parameters()
            .get_index(ParameterIndex::new(0))
            .unwrap()
        else {
            unreachable!("gain schema is float");
        };
        let initial = *initial;
        let events = block.parameter_events();
        for mut channel in block.channels() {
            let mut cursor = events
                .float_cursor(ParameterIndex::new(0), initial)
                .unwrap();
            for (offset, sample) in channel
                .copy_input_to_output()
                .unwrap()
                .iter_mut()
                .enumerate()
            {
                *sample *= cursor.value_at(u32::try_from(offset).unwrap()).unwrap();
            }
        }
    }
}

fn render(runtime: &mut InstanceRuntime<Gain>, events: &[ParameterEvent]) -> [f64; 4] {
    let mut samples = [1.0; 4];
    let mut right = [1.0; 4];
    let mut channels = [
        ChannelBuffer::in_place(
            InputEndpoint::new(AudioPortIndex::new(0), 0),
            OutputEndpoint::new(AudioPortIndex::new(1), 0),
            &mut samples,
            4,
        )
        .unwrap(),
        ChannelBuffer::in_place(
            InputEndpoint::new(AudioPortIndex::new(0), 1),
            OutputEndpoint::new(AudioPortIndex::new(1), 1),
            &mut right,
            4,
        )
        .unwrap(),
    ];
    let context = ProcessContext::new(
        ProcessMode::Realtime,
        TransportSnapshot::unknown(),
        ParameterEvents::new(events, 4, 8).unwrap(),
    );
    runtime.process(4, context, &mut channels).unwrap();
    samples
}

#[test]
fn invalid_batch_cannot_partially_retain_valid_endpoints() {
    let component = Effect;
    let config =
        ProcessConfig::new(48_000.0, NonZeroU32::new(1), NonZeroU32::new(4).unwrap(), 8).unwrap();
    let mut runtime = InstanceRuntime::for_component(&component).unwrap();
    runtime
        .activate(&component, config, DEFAULT_EFFECT_CONFIGURATION)
        .unwrap();
    let before = runtime.encode_state(StateLimits::default()).unwrap();
    // One valid endpoint must not publish when a later integer or dense
    // choice index is invalid. Reject the first out-of-range index and u32::MAX.
    for (parameter, value, error) in [
        (
            ParameterIndex::new(1),
            ParameterEventValue::Integer(9),
            ParameterValueError::IntegerOutOfRange,
        ),
        (
            ParameterIndex::new(3),
            ParameterEventValue::Choice(ChoiceIndex::new(2)),
            ParameterValueError::UnknownChoice,
        ),
        (
            ParameterIndex::new(3),
            ParameterEventValue::Choice(ChoiceIndex::new(u32::MAX)),
            ParameterValueError::UnknownChoice,
        ),
    ] {
        let events = [
            ParameterEvent::set(0, ParameterIndex::new(0), ParameterEventValue::Float(0.25)),
            ParameterEvent::set(1, parameter, value),
        ];
        let mut samples = [1.0_f64; 4];
        let mut right = [1.0_f64; 4];
        let mut channels = [
            ChannelBuffer::in_place(
                InputEndpoint::new(AudioPortIndex::new(0), 0),
                OutputEndpoint::new(AudioPortIndex::new(1), 0),
                &mut samples,
                4,
            )
            .unwrap(),
            ChannelBuffer::in_place(
                InputEndpoint::new(AudioPortIndex::new(0), 1),
                OutputEndpoint::new(AudioPortIndex::new(1), 1),
                &mut right,
                4,
            )
            .unwrap(),
        ];
        let context = ProcessContext::new(
            ProcessMode::Realtime,
            TransportSnapshot::unknown(),
            ParameterEvents::new(&events, 4, 8).unwrap(),
        );
        assert_eq!(
            runtime.process(4, context, &mut channels),
            Err(InstanceProcessError::InvalidBlock(
                ProcessBlockError::InvalidParameterEvents(ParameterAutomationError::InvalidValue {
                    index: 1,
                    error
                },),
            )),
        );
        assert_eq!(
            samples.map(f64::to_bits),
            [1.0_f64.to_bits(); 4],
            "invalid events must prevent DSP"
        );
        assert_eq!(
            runtime.encode_state(StateLimits::default()).unwrap(),
            before
        );
    }
}

#[test]
fn completed_automation_carries_into_next_block_state_and_reactivation() {
    let component = Effect;
    let config =
        ProcessConfig::new(48_000.0, NonZeroU32::new(1), NonZeroU32::new(4).unwrap(), 8).unwrap();
    let mut runtime = InstanceRuntime::for_component(&component).unwrap();
    runtime
        .activate(&component, config, DEFAULT_EFFECT_CONFIGURATION)
        .unwrap();

    let mode = ParameterIndex::new(3);
    let warm = runtime
        .parameters()
        .descriptor(mode)
        .unwrap()
        .choice_index("warm")
        .unwrap();
    let events = [
        ParameterEvent::set(1, ParameterIndex::new(1), ParameterEventValue::Integer(2)),
        ParameterEvent::set(1, ParameterIndex::new(1), ParameterEventValue::Integer(7)),
        ParameterEvent::set(
            1,
            ParameterIndex::new(2),
            ParameterEventValue::Boolean(true),
        ),
        ParameterEvent::set(1, ParameterIndex::new(3), ParameterEventValue::Choice(warm)),
        ParameterEvent::linear(3, ParameterIndex::new(0), 0.25),
    ];
    assert_eq!(
        render(&mut runtime, &events).map(f64::to_bits),
        [1.0, 0.75, 0.5, 0.25].map(f64::to_bits)
    );
    assert_eq!(
        render(&mut runtime, &[]).map(f64::to_bits),
        [0.25_f64; 4].map(f64::to_bits)
    );
    let ParameterKind::Choice { options, .. } =
        runtime.parameters().descriptor(mode).unwrap().kind()
    else {
        unreachable!("mode is a choice");
    };
    let ParameterValue::Choice(retained) = runtime.parameters().get_index(mode).unwrap() else {
        unreachable!("mode retains a choice");
    };
    assert!(
        std::ptr::eq(retained.as_str(), options[1].id().as_str()),
        "completion must retain the schema-owned ID, not allocate a copy",
    );
    let state =
        StateDocument::decode(&runtime.encode_state(StateLimits::default()).unwrap()).unwrap();
    assert_eq!(
        state
            .entries()
            .iter()
            .map(|entry| (entry.key(), entry.value()))
            .collect::<Vec<_>>(),
        [
            ("parameter/count", &StateValue::Signed(7)),
            ("parameter/gain", &StateValue::Float(0.25)),
            ("parameter/mode", &StateValue::Choice("warm".into())),
            ("parameter/mute", &StateValue::Boolean(true)),
        ],
    );

    let continuation = [ParameterEvent::linear(3, ParameterIndex::new(0), 1.0)];
    assert_eq!(
        render(&mut runtime, &continuation).map(f64::to_bits),
        [0.25, 0.5, 0.75, 1.0].map(f64::to_bits)
    );
    assert_eq!(
        runtime.parameters().get("count"),
        Some(&ParameterValue::Integer(7))
    );

    runtime
        .parameters_mut()
        .set("gain", ParameterValue::Float(0.75))
        .unwrap();
    assert_eq!(
        render(&mut runtime, &[]).map(f64::to_bits),
        [0.75_f64; 4].map(f64::to_bits)
    );
    runtime.deactivate().unwrap();
    runtime
        .activate(&component, config, DEFAULT_EFFECT_CONFIGURATION)
        .unwrap();
    assert_eq!(
        render(&mut runtime, &[]).map(f64::to_bits),
        [0.75_f64; 4].map(f64::to_bits)
    );
}
