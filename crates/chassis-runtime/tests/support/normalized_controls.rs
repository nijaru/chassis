//! Production normalized controls and runtime-owned coordinate provenance.

#[path = "normalized_trajectories.rs"]
mod trajectories;

thread_local! {
    static AFTER_DSP: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

use chassis_runtime::{
    audio::{AudioPortIndex, DEFAULT_EFFECT_CONFIGURATION},
    automation::{ParameterEvent, ParameterEventValue, ParameterEvents},
    buffer::{ChannelBuffer, InputEndpoint, OutputEndpoint},
    parameters::{
        ChoiceIndex, ChoiceOption, FloatMapping, FloatParameter, ParameterControlError,
        ParameterDescriptor, ParameterIndex, ParameterMappingError, ParameterStore, ParameterValue,
    },
    process::{ActivationConfig, ProcessBlock, ProcessConfig, ProcessContext, ProcessMode},
    runtime::{
        Component, ComponentStateError, InstanceProcessError, InstanceRuntime, Process,
        ProcessError, Processor,
    },
    schema::{ComponentId, ComponentSchema, ComponentSchemaError, StateSchemaVersion},
    state::{StateDocument, StateEntry, StateValue},
    transport::TransportSnapshot,
};
use std::{
    convert::Infallible,
    fmt,
    num::NonZeroU32,
    panic::{AssertUnwindSafe, catch_unwind},
};
const GAIN: ParameterIndex = ParameterIndex::new(0);
const OTHER: ParameterIndex = ParameterIndex::new(1);
const BOOL: ParameterIndex = ParameterIndex::new(2);

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mapping {
    Plateau(f64),
    Square,
    CompletionProbe,
    Broken,
}
impl FloatMapping for Mapping {
    fn validate(&self, minimum: f64, maximum: f64) -> Result<(), ParameterControlError> {
        let valid = match self {
            Self::Plateau(cutoff) => cutoff.is_finite() && (0.0..1.0).contains(cutoff),
            Self::Square | Self::CompletionProbe | Self::Broken => true,
        };
        if valid && matches!((minimum, maximum), (0.0, 1.0)) {
            Ok(())
        } else {
            Err(ParameterControlError::InvalidPolicy(
                "test mapping requires 0..=1 and a valid cutoff",
            ))
        }
    }
    fn normalized_to_plain(&self, coordinate: f64) -> f64 {
        match *self {
            Self::Square | Self::CompletionProbe => {
                if matches!(self, Self::CompletionProbe) {
                    assert!(!AFTER_DSP.with(std::cell::Cell::get), "mapping after DSP");
                }
                coordinate * coordinate
            }
            Self::Plateau(cutoff) => {
                if coordinate <= cutoff {
                    0.0
                } else {
                    (coordinate - cutoff) / (1.0 - cutoff)
                }
            }
            // Deliberately violates the author contract at unprobed interiors.
            // Framework consumption must still protect canonical state.
            Self::Broken => match coordinate {
                0.25 => f64::NAN,
                0.75 => 2.0,
                _ => coordinate,
            },
        }
    }
    fn plain_to_normalized(&self, plain: f64) -> Result<f64, ParameterControlError> {
        assert!(
            !super::counting_this_thread(),
            "non-realtime inverse entered tracked mutation"
        );
        Ok(match *self {
            Self::Plateau(cutoff) => {
                if plain == 0.0 {
                    0.0
                } else {
                    cutoff + plain * (1.0 - cutoff)
                }
            }
            Self::Square | Self::CompletionProbe => plain.sqrt(),
            Self::Broken => plain,
        })
    }
}
#[derive(Clone, Copy)]
enum Failure {
    None,
    Process,
    Tail,
}
struct Definition {
    mapping: Mapping,
    failure: Failure,
}
impl Default for Definition {
    fn default() -> Self {
        Self {
            mapping: Mapping::Plateau(0.5),
            failure: Failure::None,
        }
    }
}
struct Dsp {
    gain: FloatParameter,
    bank: bool,
    failure: Failure,
    processed: bool,
    completion_probe: bool,
}
#[derive(Debug)]
struct InvalidBank;
impl fmt::Display for InvalidBank {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("expected one Boolean bank")
    }
}
impl std::error::Error for InvalidBank {}
fn bank(custom: &[StateEntry]) -> Option<bool> {
    let [entry] = custom else { return None };
    if entry.key() != "state/bank" {
        return None;
    }
    match entry.value() {
        StateValue::Boolean(bank) => Some(*bank),
        _ => None,
    }
}
impl Component for Definition {
    type Processor = Dsp;
    type ActivationError = Infallible;
    fn schema(&self) -> Result<ComponentSchema, ComponentSchemaError> {
        ComponentSchema::stereo_effect_with_state(
            ComponentId::new("test.normalized-controls").unwrap(),
            StateSchemaVersion::new(1),
            vec![
                ParameterDescriptor::float("gain", "Gain", 0.0, 1.0, 0.0)
                    .unwrap()
                    .with_float_mapping(self.mapping)
                    .unwrap()
                    .with_global_modulation()
                    .unwrap(),
                ParameterDescriptor::float("other", "Other", 0.0, 1.0, 0.0).unwrap(),
                ParameterDescriptor::boolean("enabled", "Enabled", true).unwrap(),
                ParameterDescriptor::choice(
                    "mode",
                    "Mode",
                    vec![
                        ChoiceOption::new("a", "A").unwrap(),
                        ChoiceOption::new("b", "B").unwrap(),
                    ],
                    "a",
                )
                .unwrap(),
                ParameterDescriptor::integer("count", "Count", 0, 4, 0).unwrap(),
            ],
        )
    }
    fn default_custom_state(&self) -> Vec<StateEntry> {
        vec![StateEntry::new("state/bank", StateValue::Boolean(false))]
    }
    fn validate_custom_state(&self, custom: &[StateEntry]) -> Result<(), ComponentStateError> {
        bank(custom)
            .map(|_| ())
            .ok_or_else(|| ComponentStateError::new(InvalidBank))
    }
    fn activate(
        &self,
        _: &ActivationConfig<'_>,
        parameters: &ParameterStore,
        custom: &[StateEntry],
    ) -> Result<Dsp, Infallible> {
        Ok(Dsp {
            gain: parameters.float_parameter("gain").unwrap(),
            bank: bank(custom).unwrap(),
            failure: self.failure,
            processed: false,
            completion_probe: matches!(self.mapping, Mapping::CompletionProbe),
        })
    }
}
impl Processor for Dsp {
    fn tail(&self) -> chassis_runtime::runtime::TailLength {
        assert!(
            !(self.processed && matches!(self.failure, Failure::Tail)),
            "injected tail failure"
        );
        chassis_runtime::runtime::TailLength::ZERO
    }
}
impl Process<f64> for Dsp {
    fn process(&mut self, block: &mut ProcessBlock<'_, f64>) -> Result<(), ProcessError> {
        let coordinate = block
            .parameters()
            .normalized_coordinate(self.gain.index())
            .unwrap()
            .unwrap_or(-1.0);
        let mut cursor = self.gain.cursor(block)?;
        let mut values = [0.0; 4];
        for offset in 0..block.frame_count() {
            values[offset as usize] =
                cursor.value_at(offset)? + 2.0 * f64::from(u8::from(self.bank));
        }
        block.for_each_channel(|mut channel| {
            let left = channel.output_endpoint().unwrap().channel() == 0;
            let output = channel.output_mut().unwrap();
            if left {
                output.copy_from_slice(&values[..output.len()]);
            } else {
                output.fill(coordinate);
            }
        });
        self.processed = true;
        AFTER_DSP.with(|flag| flag.set(self.completion_probe));
        if matches!(self.failure, Failure::Process) {
            return Err(ProcessError::Message("injected DSP failure"));
        }
        Ok(())
    }
}
fn config() -> ProcessConfig {
    ProcessConfig::new(48_000.0, NonZeroU32::new(1), NonZeroU32::new(4).unwrap(), 8).unwrap()
}
fn activate(runtime: &mut InstanceRuntime<Dsp>, definition: &Definition) {
    runtime
        .activate(definition, config(), DEFAULT_EFFECT_CONFIGURATION)
        .unwrap();
}
fn render(
    runtime: &mut InstanceRuntime<Dsp>,
    events: &[ParameterEvent],
) -> Result<[f64; 2], InstanceProcessError> {
    let output = render_frames(runtime, events, 4)?;
    Ok([output[0][0], output[1][0]])
}
fn render_frames(
    runtime: &mut InstanceRuntime<Dsp>,
    events: &[ParameterEvent],
    frames: u32,
) -> Result<[[f64; 4]; 2], InstanceProcessError> {
    let mut left = [0.0; 4];
    let mut right = [0.0; 4];
    let mut buffers = [
        ChannelBuffer::in_place(
            InputEndpoint::new(AudioPortIndex::new(0), 0),
            OutputEndpoint::new(AudioPortIndex::new(1), 0),
            &mut left[..frames as usize],
            frames,
        )
        .unwrap(),
        ChannelBuffer::in_place(
            InputEndpoint::new(AudioPortIndex::new(0), 1),
            OutputEndpoint::new(AudioPortIndex::new(1), 1),
            &mut right[..frames as usize],
            frames,
        )
        .unwrap(),
    ];
    let context = ProcessContext::new(
        ProcessMode::Realtime,
        &TransportSnapshot::UNKNOWN,
        ParameterEvents::new(events, frames, 16).unwrap(),
    );
    runtime.process(frames, context, &mut buffers)?;
    AFTER_DSP.with(|flag| flag.set(false));
    Ok([left, right])
}
fn document(runtime: &InstanceRuntime<Dsp>, gain: f64, bank: bool) -> StateDocument {
    let current = runtime.state_document().unwrap();
    let mut result = StateDocument::new(current.component_id(), current.schema_version()).unwrap();
    for entry in current.entries() {
        let value = match entry.key() {
            "parameter/gain" => StateValue::Float(gain),
            "state/bank" => StateValue::Boolean(bank),
            _ => entry.value().clone(),
        };
        result.insert(StateEntry::new(entry.key(), value)).unwrap();
    }
    result
}

fn pair(store: &ParameterStore) -> (u64, Option<u64>) {
    let ParameterValue::Float(plain) = store.get_index(GAIN).unwrap() else {
        unreachable!()
    };
    (
        plain.to_bits(),
        store.normalized_coordinate(GAIN).unwrap().map(f64::to_bits),
    )
}

#[test]
fn normalized_scalar_provenance_is_exact_and_not_a_canonical_inverse() {
    let mut runtime = InstanceRuntime::for_component(&Definition::default()).unwrap();
    assert_eq!(
        runtime.parameters().normalized_coordinate(GAIN).unwrap(),
        None
    );
    for coordinate in [0.25, 0.5] {
        runtime
            .parameters_mut()
            .set_normalized("gain", coordinate)
            .unwrap();
        assert_eq!(
            pair(runtime.parameters()),
            (0.0_f64.to_bits(), Some(coordinate.to_bits()))
        );
        assert_eq!(
            runtime
                .parameters()
                .normalized_value(GAIN)
                .unwrap()
                .to_bits(),
            coordinate.to_bits()
        );
        let descriptor = runtime.parameters().descriptor(GAIN).unwrap();
        assert_eq!(
            descriptor
                .value_to_normalized(runtime.parameters().get_index(GAIN).unwrap())
                .unwrap()
                .to_bits(),
            0.0_f64.to_bits()
        );
    }
    // None means unknown provenance, not a lazily reconstructed coordinate.
    runtime
        .parameters_mut()
        .set("gain", ParameterValue::Float(0.0))
        .unwrap();
    assert_eq!(
        runtime
            .parameters()
            .normalized_value(GAIN)
            .unwrap()
            .to_bits(),
        0.0_f64.to_bits()
    );
    assert_eq!(
        runtime.parameters().normalized_coordinate(GAIN).unwrap(),
        None
    );
}

#[test]
fn every_plain_scalar_replacement_invalidates_even_equal_values() {
    let mut store =
        ParameterStore::new(Definition::default().schema().unwrap().parameters()).unwrap();
    for replace in [
        (|s: &mut ParameterStore| s.set("gain", ParameterValue::Float(0.0)).unwrap())
            as fn(&mut ParameterStore),
        |s| s.set_index(GAIN, ParameterValue::Float(0.0)).unwrap(),
        |s| {
            s.set_event_value(GAIN, ParameterEventValue::Float(0.0))
                .unwrap();
        },
        |s| {
            s.values_mut()
                .set("gain", ParameterValue::Float(0.0))
                .unwrap();
        },
        |s| {
            s.values_mut()
                .set_index(GAIN, ParameterValue::Float(0.0))
                .unwrap();
        },
        |s| {
            s.values_mut()
                .set_event_value(GAIN, ParameterEventValue::Float(0.0))
                .unwrap();
        },
        |s| s.reset(),
        |s| s.values_mut().reset(),
    ] {
        store.set_normalized_index(GAIN, 0.5).unwrap();
        replace(&mut store);
        assert_eq!(pair(&store), (0.0_f64.to_bits(), None));
    }
}

#[test]
fn normalized_mutation_and_state_rejection_are_failure_atomic() {
    let component = Definition {
        mapping: Mapping::Broken,
        ..Definition::default()
    };
    let mut runtime = InstanceRuntime::for_component(&component).unwrap();
    runtime
        .parameters_mut()
        .set_normalized_index(GAIN, 0.5)
        .unwrap();
    let before = pair(runtime.parameters());
    for coordinate in [f64::NAN, f64::INFINITY, -0.1, 1.1, 0.25, 0.75] {
        assert!(
            runtime
                .parameters_mut()
                .set_normalized_index(GAIN, coordinate)
                .is_err()
        );
        assert!(
            runtime
                .parameters_mut()
                .set_normalized("gain", coordinate)
                .is_err()
        );
        assert_eq!(pair(runtime.parameters()), before);
    }
    assert_eq!(
        runtime
            .parameters_mut()
            .set_normalized_index(ParameterIndex::new(99), 0.5),
        Err(ParameterMappingError::UnknownParameter(
            ParameterIndex::new(99)
        ))
    );
    assert!(
        runtime
            .parameters_mut()
            .set_normalized_index(BOOL, 0.5)
            .is_err()
    );
    assert!(runtime.parameters().normalized_coordinate(BOOL).is_err());
    assert!(
        runtime
            .parameters()
            .normalized_coordinate(ParameterIndex::new(99))
            .is_err()
    );
    assert!(
        runtime
            .parameters_mut()
            .set_normalized("missing", 0.5)
            .is_err()
    );
    assert!(
        runtime
            .parameters_mut()
            .set_event_value(GAIN, ParameterEventValue::Float(2.0))
            .is_err()
    );
    let invalid = document(&runtime, 2.0, true);
    assert!(runtime.apply_state(&component, &invalid).is_err());
    assert_eq!(pair(runtime.parameters()), before);
}

#[test]
fn completion_invalidates_only_successful_plain_endpoints() {
    let component = Definition::default();
    let mut runtime = InstanceRuntime::for_component(&component).unwrap();
    runtime
        .parameters_mut()
        .set_normalized_index(GAIN, 0.5)
        .unwrap();
    runtime
        .parameters_mut()
        .set_normalized_index(OTHER, 0.75)
        .unwrap();
    activate(&mut runtime, &component);
    assert_eq!(
        render(&mut runtime, &[]).unwrap().map(f64::to_bits),
        [0.0, 0.5].map(f64::to_bits)
    );
    assert_eq!(
        render(&mut runtime, &[ParameterEvent::modulation(0, GAIN, 0.25)])
            .unwrap()
            .map(f64::to_bits),
        [0.25, 0.5].map(f64::to_bits)
    );
    assert_eq!(
        pair(runtime.parameters()),
        (0.0_f64.to_bits(), Some(0.5_f64.to_bits()))
    );
    assert!(
        render(
            &mut runtime,
            &[ParameterEvent::set(
                0,
                GAIN,
                ParameterEventValue::Float(2.0)
            )]
        )
        .is_err()
    );
    assert_eq!(
        pair(runtime.parameters()),
        (0.0_f64.to_bits(), Some(0.5_f64.to_bits()))
    );
    render(&mut runtime, &[ParameterEvent::linear(3, GAIN, 0.0)]).unwrap();
    assert_eq!(pair(runtime.parameters()), (0.0_f64.to_bits(), None));
    assert_eq!(
        runtime
            .parameters()
            .normalized_coordinate(OTHER)
            .unwrap()
            .map(f64::to_bits),
        Some(0.75_f64.to_bits())
    );
    for failure in [Failure::Process, Failure::Tail] {
        let component = Definition {
            failure,
            ..Definition::default()
        };
        let mut failed = InstanceRuntime::for_component(&component).unwrap();
        failed
            .parameters_mut()
            .set_normalized_index(GAIN, 0.5)
            .unwrap();
        activate(&mut failed, &component);
        let result = catch_unwind(AssertUnwindSafe(|| {
            render(
                &mut failed,
                &[ParameterEvent::set(
                    0,
                    GAIN,
                    ParameterEventValue::Float(0.8),
                )],
            )
        }));
        match failure {
            Failure::Process => assert!(result.unwrap().is_err()),
            Failure::Tail => assert!(result.is_err()),
            Failure::None => unreachable!(),
        }
        assert_eq!(
            pair(failed.parameters()),
            (0.0_f64.to_bits(), Some(0.5_f64.to_bits()))
        );
        // Neither damaged processor is resumed.
    }
}

#[test]
fn loads_clear_desired_provenance_but_frozen_resources_keep_the_first_pair() {
    let component = Definition::default();
    let mut runtime = InstanceRuntime::for_component(&component).unwrap();
    runtime
        .parameters_mut()
        .set_normalized_index(GAIN, 0.5)
        .unwrap();
    let same = runtime.state_document().unwrap();
    activate(&mut runtime, &component);
    runtime.reset().unwrap();
    assert_eq!(
        pair(runtime.parameters()),
        (0.0_f64.to_bits(), Some(0.5_f64.to_bits()))
    );
    runtime.deactivate().unwrap();
    activate(&mut runtime, &component);
    assert_eq!(
        render(&mut runtime, &[]).unwrap()[1].to_bits(),
        0.5_f64.to_bits()
    );
    runtime.apply_state(&component, &same).unwrap();
    assert!(!runtime.restart_requested());
    assert_eq!(pair(runtime.parameters()), (0.0_f64.to_bits(), None));
    runtime
        .parameters_mut()
        .set_normalized_index(GAIN, 0.5)
        .unwrap();
    let replacement = document(&runtime, 0.0, true);
    runtime.apply_state(&component, &replacement).unwrap();
    assert!(runtime.restart_requested());
    assert_eq!(pair(runtime.parameters()), (0.0_f64.to_bits(), None));
    for coordinate in [0.25, 1.0] {
        runtime
            .parameters_mut()
            .set_normalized_index(GAIN, coordinate)
            .unwrap();
        assert_eq!(
            render(
                &mut runtime,
                &[ParameterEvent::set(
                    0,
                    GAIN,
                    ParameterEventValue::Float(0.9)
                )]
            )
            .unwrap()
            .map(f64::to_bits),
            [0.0, 0.5].map(f64::to_bits)
        );
        assert_eq!(
            runtime
                .parameters()
                .normalized_coordinate(GAIN)
                .unwrap()
                .map(f64::to_bits),
            Some(coordinate.to_bits())
        );
        runtime.reset().unwrap();
        runtime.apply_state(&component, &replacement).unwrap();
        assert_eq!(pair(runtime.parameters()), (0.0_f64.to_bits(), None));
    }
    runtime
        .parameters_mut()
        .set_normalized_index(GAIN, 0.25)
        .unwrap();
    runtime.deactivate().unwrap();
    activate(&mut runtime, &component);
    assert_eq!(
        render(&mut runtime, &[]).unwrap().map(f64::to_bits),
        [2.0, 0.25].map(f64::to_bits)
    );
}

#[test]
fn mapping_configuration_equality_is_semantic_not_pointer_identity() {
    let component = Definition::default();
    let mut runtime = InstanceRuntime::for_component(&component).unwrap();
    runtime
        .parameters_mut()
        .set_normalized_index(GAIN, 0.5)
        .unwrap();
    let equivalent = Definition::default();
    activate(&mut runtime, &equivalent);
    runtime.deactivate().unwrap();
    let changed = Definition {
        mapping: Mapping::Plateau(0.25),
        ..Definition::default()
    };
    assert!(
        runtime
            .activate(&changed, config(), DEFAULT_EFFECT_CONFIGURATION)
            .is_err()
    );
    let state = runtime.state_document().unwrap();
    assert!(runtime.apply_state(&changed, &state).is_err());
    assert_eq!(
        pair(runtime.parameters()),
        (0.0_f64.to_bits(), Some(0.5_f64.to_bits()))
    );
    assert!(
        ParameterDescriptor::boolean("bool", "Bool", false)
            .unwrap()
            .with_float_mapping(Mapping::Plateau(0.5))
            .is_err()
    );
    assert!(
        ParameterDescriptor::float("gain", "Gain", 0.0, 1.0, 0.0)
            .unwrap()
            .with_float_mapping(Mapping::Plateau(1.0))
            .is_err()
    );
}

pub(super) fn qualify() {
    let component = Definition::default();
    let mut runtime = InstanceRuntime::for_component(&component).unwrap();
    activate(&mut runtime, &component);
    let mut broken = ParameterStore::new(
        Definition {
            mapping: Mapping::Broken,
            ..Definition::default()
        }
        .schema()
        .unwrap()
        .parameters(),
    )
    .unwrap();
    broken.set_normalized_index(GAIN, 0.5).unwrap();
    super::ALLOCATIONS.store(0, std::sync::atomic::Ordering::Relaxed);
    super::DEALLOCATIONS.store(0, std::sync::atomic::Ordering::Relaxed);
    super::COUNT_THIS_THREAD.with(|flag| flag.set(true));
    for i in 0..1_000 {
        let coordinate = if i % 2 == 0 { 0.25 } else { 1.0 };
        runtime
            .parameters_mut()
            .set_normalized_index(GAIN, coordinate)
            .unwrap();
        runtime
            .parameters_mut()
            .set_normalized_index(OTHER, coordinate)
            .unwrap();
        let rendered = render(&mut runtime, &[]).unwrap();
        assert_eq!(rendered[1].to_bits(), coordinate.to_bits());
        assert_eq!(
            rendered[0].to_bits(),
            if i % 2 == 0 {
                0.0_f64.to_bits()
            } else {
                1.0_f64.to_bits()
            }
        );
        let timed = [
            ParameterEvent::normalized_set(0, GAIN, 0.25),
            ParameterEvent::set(0, OTHER, ParameterEventValue::Float(0.5)),
            ParameterEvent::normalized_set(1, OTHER, 0.25),
            ParameterEvent::set(2, BOOL, ParameterEventValue::Boolean(false)),
            ParameterEvent::set(
                2,
                ParameterIndex::new(3),
                ParameterEventValue::Choice(ChoiceIndex::new(1)),
            ),
            ParameterEvent::set(2, ParameterIndex::new(4), ParameterEventValue::Integer(2)),
            ParameterEvent::normalized_linear(3, GAIN, 0.75),
            ParameterEvent::normalized_linear(3, OTHER, 0.75),
        ];
        render(&mut runtime, &timed).unwrap();
        assert_eq!(
            pair(runtime.parameters()),
            (0.5_f64.to_bits(), Some(0.75_f64.to_bits()))
        );
        assert_eq!(
            runtime.parameters().get_event_value(ParameterIndex::new(3)),
            Some(ParameterEventValue::Choice(ChoiceIndex::new(1)))
        );
        let invalid_endpoint = [ParameterEvent::normalized_set(0, GAIN, 0.25)];
        assert!(
            broken
                .validate_events(ParameterEvents::new(&invalid_endpoint, 4, 1).unwrap())
                .is_err()
        );
        for invalid in [f64::NAN, -0.1, 1.1] {
            assert!(
                runtime
                    .parameters_mut()
                    .set_normalized_index(GAIN, invalid)
                    .is_err()
            );
        }
        assert!(
            runtime
                .parameters_mut()
                .set_normalized_index(BOOL, 0.5)
                .is_err()
        );
        assert!(
            runtime
                .parameters_mut()
                .set_normalized_index(ParameterIndex::new(99), 0.5)
                .is_err()
        );
        assert!(broken.set_normalized_index(GAIN, 0.25).is_err());
        assert!(broken.set_normalized_index(GAIN, 0.75).is_err());
    }
    super::COUNT_THIS_THREAD.with(|flag| flag.set(false));
    assert_eq!(
        super::ALLOCATIONS.load(std::sync::atomic::Ordering::Relaxed),
        0
    );
    assert_eq!(
        super::DEALLOCATIONS.load(std::sync::atomic::Ordering::Relaxed),
        0
    );
}
