//! Complete persistence and phase-oriented public error contracts.

use chassis_runtime::{
    automation::ParameterEventValue,
    parameters::{
        ChoiceId, ChoiceIndex, ChoiceOption, ParameterDescriptor, ParameterStore, ParameterValue,
        ParameterValueError,
    },
    process::ActivationConfig,
    runtime::{
        Component, InstanceRuntime, Processor, StateExportError, StateLoadError,
        StateValidationError,
    },
    schema::{ComponentId, ComponentSchema, ComponentSchemaError, StateSchemaVersion},
    state::{
        StateDecodeError, StateDocument, StateEntry, StateLimits, StateMigrationError, StateValue,
    },
};
use std::{convert::Infallible, error::Error};

struct Definition {
    reversed: bool,
}
struct Dsp;
impl Processor for Dsp {}
impl Component for Definition {
    type Processor = Dsp;
    type ActivationError = Infallible;
    fn schema(&self) -> Result<ComponentSchema, ComponentSchemaError> {
        let mut choices = vec![
            ChoiceOption::new("clean", "Clean").unwrap(),
            ChoiceOption::new("warm", "Warm").unwrap(),
        ];
        if self.reversed {
            choices.reverse();
        }
        ComponentSchema::stereo_effect_with_state(
            ComponentId::new("test.state-api").unwrap(),
            StateSchemaVersion::new(2),
            vec![
                ParameterDescriptor::float("gain", "Gain", -1.0, 1.0, 0.0).unwrap(),
                ParameterDescriptor::integer("quality", "Quality", 1, 4, 2).unwrap(),
                ParameterDescriptor::boolean("bypass", "Bypass", false).unwrap(),
                ParameterDescriptor::choice("mode", "Mode", choices, "clean").unwrap(),
            ],
        )
    }
    fn activate(
        &self,
        _: &ActivationConfig<'_>,
        _: &ParameterStore,
        _: &[StateEntry],
    ) -> Result<Dsp, Infallible> {
        Ok(Dsp)
    }
}

#[test]
fn complete_round_trip_preserves_plain_values_and_stable_choice_identity() {
    let definition = Definition { reversed: false };
    let mut runtime = InstanceRuntime::for_component(&definition).unwrap();
    for (key, value) in [
        ("gain", ParameterValue::Float(0.75)),
        ("quality", ParameterValue::Integer(4)),
        ("bypass", ParameterValue::Boolean(true)),
        (
            "mode",
            ParameterValue::Choice(ChoiceId::new("warm").unwrap()),
        ),
    ] {
        runtime.parameters_mut().set(key, value).unwrap();
    }
    let bytes = runtime.encode_state(StateLimits::default()).unwrap();
    let document = StateDocument::decode(&bytes).unwrap();
    assert_eq!(document.component_id(), "test.state-api");
    assert_eq!(document.schema_version(), 2);
    assert!(
        document
            .entries()
            .iter()
            .all(|entry| entry.parameter_key().is_some())
    );
    let reversed = Definition { reversed: true };
    let mut restored = InstanceRuntime::for_component(&reversed).unwrap();
    restored
        .apply_state_bytes(&reversed, &bytes, StateLimits::default())
        .unwrap();
    assert_eq!(
        restored.encode_state(StateLimits::default()).unwrap(),
        bytes
    );
    let mode = restored.parameters().index("mode").unwrap();
    assert_eq!(
        restored.parameters().get_event_value(mode),
        Some(ParameterEventValue::Choice(ChoiceIndex::new(0)))
    );
    assert_eq!(
        restored.parameters().get("mode"),
        runtime.parameters().get("mode")
    );
}

fn candidate(
    id: &str,
    schema: u32,
    replacement: Option<(&str, StateValue)>,
    omitted: Option<&str>,
) -> StateDocument {
    let mut document = StateDocument::new(id, schema).unwrap();
    for (key, value) in [
        ("gain", StateValue::Float(0.25)),
        ("quality", StateValue::Signed(2)),
        ("bypass", StateValue::Boolean(false)),
        ("mode", StateValue::Choice("clean".into())),
    ] {
        if omitted == Some(key)
            || replacement
                .as_ref()
                .is_some_and(|(replace, _)| *replace == key)
        {
            continue;
        }
        document.insert(StateEntry::parameter(key, value)).unwrap();
    }
    if let Some((key, value)) = replacement {
        document.insert(StateEntry::parameter(key, value)).unwrap();
    }
    document
}

#[test]
fn validation_errors_are_specific_and_failed_candidates_never_publish() {
    let definition = Definition { reversed: false };
    let mut runtime = InstanceRuntime::for_component(&definition).unwrap();
    let before = runtime.state_document().unwrap();
    let id = "test.state-api";
    let cases = [
        candidate("foreign", 2, None, None),
        candidate(id, 1, None, None),
        candidate(id, 2, None, Some("mode")),
        candidate(id, 2, Some(("quality", StateValue::Signed(99))), None),
        candidate(id, 2, Some(("gain", StateValue::Unsigned(1))), None),
        candidate(
            id,
            2,
            Some(("mode", StateValue::Choice("missing".into()))),
            None,
        ),
    ];
    for (index, document) in cases.iter().enumerate() {
        let error = runtime.apply_state(&definition, document).unwrap_err();
        match (index, &error) {
            (0, StateValidationError::ComponentIdentityMismatch { expected, actual }) => {
                assert_eq!(expected, id);
                assert_eq!(actual, "foreign");
            }
            (
                1,
                StateValidationError::SchemaVersionMismatch {
                    expected: 2,
                    actual: 1,
                },
            )
            | (
                2,
                StateValidationError::IncompleteParameterState {
                    expected: 4,
                    actual: 3,
                },
            ) => {}
            (
                3,
                StateValidationError::InvalidParameterValue {
                    parameter,
                    error: ParameterValueError::IntegerOutOfRange,
                },
            ) => assert_eq!(parameter, "quality"),
            (
                4,
                StateValidationError::InvalidParameterValue {
                    parameter,
                    error: ParameterValueError::UnsupportedStateValue("unsigned"),
                },
            ) => assert_eq!(parameter, "gain"),
            (
                5,
                StateValidationError::InvalidParameterValue {
                    parameter,
                    error: ParameterValueError::UnknownChoice,
                },
            ) => assert_eq!(parameter, "mode"),
            _ => panic!("unexpected validation evidence: {error:?}"),
        }
        if index >= 3 {
            assert!(error.source().unwrap().is::<ParameterValueError>());
        }
        assert_eq!(runtime.state_document().unwrap(), before);
        assert!(!runtime.restart_requested());
    }
    // Even the empty suffix belongs to the reserved namespace, not custom state.
    for key in ["parameter/unknown", "parameter/"] {
        let mut document = before.clone();
        document
            .insert(StateEntry::new(key, StateValue::Boolean(true)))
            .unwrap();
        assert!(matches!(
            runtime.apply_state(&definition, &document),
            Err(StateValidationError::UnknownParameter(_))
        ));
        assert_eq!(runtime.state_document().unwrap(), before);
    }
}

#[test]
fn byte_load_and_export_expose_the_failed_phase_and_source() {
    let definition = Definition { reversed: false };
    let mut runtime = InstanceRuntime::for_component(&definition).unwrap();
    let decode = runtime
        .apply_state_bytes(&definition, b"bad!", StateLimits::default())
        .unwrap_err();
    assert!(matches!(
        decode,
        StateLoadError::Decode(StateDecodeError::InvalidMagic)
    ));
    assert!(decode.source().unwrap().is::<StateDecodeError>());
    let old = candidate("test.state-api", 1, None, None).encode().unwrap();
    assert!(matches!(
        runtime.apply_state_bytes(&definition, &old, StateLimits::default()),
        Err(StateLoadError::Migration(
            StateMigrationError::MissingMigration { from: 1, target: 2 }
        ))
    ));
    let foreign = candidate("foreign", 1, None, None).encode().unwrap();
    assert!(matches!(
        runtime.apply_state_bytes(&definition, &foreign, StateLimits::default()),
        Err(StateLoadError::Validation(
            StateValidationError::ComponentIdentityMismatch { .. }
        ))
    ));
    let error = runtime
        .encode_state(StateLimits {
            max_total_bytes: 1,
            ..StateLimits::default()
        })
        .unwrap_err();
    assert!(matches!(error, StateExportError::Encode(_)));
    assert!(
        error
            .source()
            .unwrap()
            .is::<chassis_runtime::state::StateEncodeError>()
    );
}
