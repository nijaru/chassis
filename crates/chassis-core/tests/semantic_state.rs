//! External conformance tests for complete semantic instance state ownership.

use std::{
    convert::Infallible,
    fmt,
    num::NonZeroU32,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use chassis_core::{
    audio::{DEFAULT_EFFECT_CONFIGURATION, DEFAULT_EFFECT_PORTS},
    parameters::{ParameterDescriptor, ParameterStore, ParameterValue},
    process::{ActivationConfig, ProcessConfig},
    runtime::{
        Component, InstanceRuntime, InstanceSemanticStateError, InstanceSemanticStateLoadError,
        Processor,
    },
    schema::{ComponentId, ComponentSchema, StateSchemaVersion},
    state::{StateDocument, StateEntry, StateLimits, StateMigration, StateValue},
};

struct StateAwareEffect {
    parameters: Vec<ParameterDescriptor>,
    activation_quality: Arc<AtomicBool>,
}

impl StateAwareEffect {
    fn new(activation_quality: Arc<AtomicBool>) -> Self {
        Self {
            parameters: vec![
                ParameterDescriptor::float("gain", "Gain", 0.0, 2.0, 1.0)
                    .expect("test parameter is valid"),
            ],
            activation_quality,
        }
    }
}

struct StateProcessor;

impl Processor for StateProcessor {}

impl Component for StateAwareEffect {
    type Processor = StateProcessor;
    type ActivationError = Infallible;

    fn parameter_descriptors(&self) -> &[ParameterDescriptor] {
        &self.parameters
    }

    fn schema(&self) -> Result<ComponentSchema, chassis_core::schema::ComponentSchemaError> {
        ComponentSchema::new(
            ComponentId::new("com.example.semantic").expect("component identity is valid"),
            StateSchemaVersion::new(2),
            DEFAULT_EFFECT_PORTS.to_vec(),
            vec![],
            self.parameters.clone(),
        )
    }

    fn activate(
        &self,
        _config: &ActivationConfig<'_>,
    ) -> Result<Self::Processor, Self::ActivationError> {
        Ok(StateProcessor)
    }

    fn activate_with_state(
        &self,
        config: &ActivationConfig<'_>,
        _parameters: &ParameterStore,
        custom_state: &[StateEntry],
    ) -> Result<Self::Processor, Self::ActivationError> {
        let quality = custom_state
            .iter()
            .find(|entry| entry.key() == "state/quality")
            .and_then(|entry| match entry.value() {
                StateValue::Boolean(value) => Some(*value),
                _ => None,
            })
            .unwrap_or(false);
        self.activation_quality.store(quality, Ordering::Relaxed);
        self.activate(config)
    }
}

fn process_config() -> ProcessConfig {
    ProcessConfig::new(
        48_000.0,
        NonZeroU32::new(1),
        NonZeroU32::new(64).expect("maximum is non-zero"),
        8,
    )
    .expect("process config is valid")
}

fn complete_document(schema: u32, gain: f64, quality: bool) -> StateDocument {
    let mut document =
        StateDocument::new("com.example.semantic", schema).expect("product identity is valid");
    document
        .insert(StateEntry::new(
            "state/quality",
            StateValue::Boolean(quality),
        ))
        .expect("custom key is unique");
    document
        .insert(StateEntry::new("parameter/gain", StateValue::Float(gain)))
        .expect("parameter key is unique");
    document
}

fn quality(custom_state: &[StateEntry]) -> Option<bool> {
    custom_state
        .iter()
        .find(|entry| entry.key() == "state/quality")
        .and_then(|entry| match entry.value() {
            StateValue::Boolean(value) => Some(*value),
            _ => None,
        })
}

#[test]
fn complete_state_is_committed_saved_and_visible_to_activation() {
    let activation_quality = Arc::new(AtomicBool::new(false));
    let component = StateAwareEffect::new(Arc::clone(&activation_quality));
    let mut runtime: InstanceRuntime<StateProcessor> =
        InstanceRuntime::for_component(&component).expect("runtime schema is valid");
    let document = complete_document(2, 0.25, true);

    runtime
        .apply_state(
            &document,
            |parameters, custom_state| -> Result<(), Infallible> {
                assert_eq!(parameters.get("gain"), Some(&ParameterValue::Float(0.25)));
                assert_eq!(quality(custom_state), Some(true));
                Ok(())
            },
        )
        .expect("complete state applies");

    assert_eq!(
        runtime.parameters().get("gain"),
        Some(&ParameterValue::Float(0.25))
    );
    assert_eq!(quality(runtime.custom_state()), Some(true));

    let saved = runtime.state_document().expect("complete state exports");
    assert_eq!(saved.product_id(), "com.example.semantic");
    assert_eq!(saved.product_schema(), 2);
    assert_eq!(saved.entries().len(), 2);
    assert_eq!(
        saved
            .entries()
            .iter()
            .find(|entry| entry.key() == "state/quality")
            .map(StateEntry::value),
        Some(&StateValue::Boolean(true))
    );

    runtime
        .activate(&component, process_config(), DEFAULT_EFFECT_CONFIGURATION)
        .expect("activation succeeds");
    assert!(activation_quality.load(Ordering::Relaxed));
    runtime.deactivate().expect("deactivation succeeds");
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RejectState;

impl fmt::Display for RejectState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("candidate state rejected")
    }
}

impl std::error::Error for RejectState {}

#[test]
fn product_validation_failure_is_atomic_across_parameters_and_custom_state() {
    let activation_quality = Arc::new(AtomicBool::new(false));
    let component = StateAwareEffect::new(activation_quality);
    let mut runtime: InstanceRuntime<StateProcessor> =
        InstanceRuntime::for_component(&component).expect("runtime schema is valid");

    runtime
        .apply_state(
            &complete_document(2, 1.5, false),
            |_parameters, _custom_state| Ok::<(), Infallible>(()),
        )
        .expect("initial complete state applies");

    let result = runtime.apply_state(
        &complete_document(2, 0.25, true),
        |parameters, custom_state| {
            assert_eq!(parameters.get("gain"), Some(&ParameterValue::Float(0.25)));
            assert_eq!(quality(custom_state), Some(true));
            Err(RejectState)
        },
    );
    assert!(matches!(
        result,
        Err(InstanceSemanticStateError::Product(RejectState))
    ));
    assert_eq!(
        runtime.parameters().get("gain"),
        Some(&ParameterValue::Float(1.5))
    );
    assert_eq!(quality(runtime.custom_state()), Some(false));
}

struct RenameGain;

impl StateMigration for RenameGain {
    type Error = Infallible;

    fn source_schema(&self) -> u32 {
        1
    }

    fn to_schema(&self) -> u32 {
        2
    }

    fn migrate(&self, document: StateDocument) -> Result<StateDocument, Self::Error> {
        let mut migrated = StateDocument::new(document.product_id().to_owned(), 2)
            .expect("product identity is valid");
        for entry in document.entries() {
            let key = if entry.key() == "parameter/old_gain" {
                "parameter/gain"
            } else {
                entry.key()
            };
            migrated
                .insert(StateEntry::new(key, entry.value().clone()))
                .expect("migration keys are unique");
        }
        Ok(migrated)
    }
}

#[test]
fn migrated_byte_load_publishes_parameters_and_custom_state_together() {
    let activation_quality = Arc::new(AtomicBool::new(false));
    let component = StateAwareEffect::new(activation_quality);
    let mut runtime: InstanceRuntime<StateProcessor> =
        InstanceRuntime::for_component(&component).expect("runtime schema is valid");

    let mut legacy =
        StateDocument::new("com.example.semantic", 1).expect("product identity is valid");
    legacy
        .insert(StateEntry::new(
            "parameter/old_gain",
            StateValue::Float(0.75),
        ))
        .expect("legacy parameter is unique");
    legacy
        .insert(StateEntry::new("state/quality", StateValue::Boolean(true)))
        .expect("legacy custom key is unique");
    let bytes = legacy.encode().expect("legacy state encodes");
    let migration = RenameGain;

    runtime
        .apply_state_bytes(
            &bytes,
            StateLimits::default(),
            &[&migration],
            |_parameters, custom_state| {
                if quality(custom_state) == Some(true) {
                    Ok(())
                } else {
                    Err(RejectState)
                }
            },
        )
        .expect("migrated complete state applies");

    assert_eq!(
        runtime.parameters().get("gain"),
        Some(&ParameterValue::Float(0.75))
    );
    assert_eq!(quality(runtime.custom_state()), Some(true));

    let invalid = runtime.apply_state_bytes(
        &bytes,
        StateLimits::default(),
        &[&migration],
        |_parameters, _custom_state| Err(RejectState),
    );
    assert!(matches!(
        invalid,
        Err(InstanceSemanticStateLoadError::Apply(
            InstanceSemanticStateError::Product(RejectState)
        ))
    ));
    assert_eq!(
        runtime.parameters().get("gain"),
        Some(&ParameterValue::Float(0.75))
    );
    assert_eq!(quality(runtime.custom_state()), Some(true));
}
