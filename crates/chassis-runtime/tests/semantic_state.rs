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

use chassis_runtime::{
    audio::{DEFAULT_EFFECT_CONFIGURATION, DEFAULT_EFFECT_PORTS, MAIN_INPUT, MAIN_OUTPUT},
    automation::ParameterEvents,
    buffer::{ChannelBuffer, InputEndpoint, OutputEndpoint},
    parameters::{ParameterDescriptor, ParameterStore, ParameterValue},
    process::{
        ActivationConfig, ProcessBlock, ProcessBufferSource, ProcessChannel, ProcessConfig,
        ProcessContext, ProcessMode, TransportSnapshot,
    },
    runtime::{
        Component, InstanceRuntime, InstanceSemanticStateError, InstanceSemanticStateLoadError,
        Process, Processor,
    },
    schema::{ComponentId, ComponentSchema, StateSchemaVersion},
    state::{
        StateDocument, StateDocumentError, StateEntry, StateLimits, StateMigration,
        StateMigrationError, StateValue,
    },
};

struct StateAwareEffect {
    parameters: Vec<ParameterDescriptor>,
    activation_quality: Arc<AtomicBool>,
    processor_restart: Arc<AtomicBool>,
}

impl StateAwareEffect {
    fn new(activation_quality: Arc<AtomicBool>) -> Self {
        Self {
            parameters: vec![
                ParameterDescriptor::float("gain", "Gain", 0.0, 2.0, 1.0)
                    .expect("test parameter is valid"),
            ],
            activation_quality,
            processor_restart: Arc::new(AtomicBool::new(false)),
        }
    }
}

struct StateProcessor {
    amount: f64,
    restart: Arc<AtomicBool>,
}

impl Processor for StateProcessor {
    fn restart_requested(&self) -> bool {
        self.restart.load(Ordering::Relaxed)
    }
}

impl Process<f64> for StateProcessor {
    fn process<B>(&mut self, block: &mut ProcessBlock<'_, '_, '_, f64, B>)
    where
        B: ProcessBufferSource<f64> + ?Sized,
    {
        for mut channel in block.channels() {
            channel
                .output_mut()
                .expect("output channel")
                .fill(self.amount);
        }
    }
}

impl Component for StateAwareEffect {
    type Processor = StateProcessor;
    type ActivationError = Infallible;

    fn schema(&self) -> Result<ComponentSchema, chassis_runtime::schema::ComponentSchemaError> {
        ComponentSchema::new(
            ComponentId::new("com.example.semantic").expect("component identity is valid"),
            StateSchemaVersion::new(2),
            DEFAULT_EFFECT_PORTS.to_vec(),
            chassis_runtime::audio::AudioIoPolicy::stereo_effect(),
            vec![],
            self.parameters.clone(),
        )
    }

    fn activate(
        &self,
        _config: &ActivationConfig<'_>,
    ) -> Result<Self::Processor, Self::ActivationError> {
        Ok(StateProcessor {
            amount: 1.0,
            restart: Arc::clone(&self.processor_restart),
        })
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
        let mut processor = self.activate(config)?;
        processor.amount = custom_state
            .iter()
            .find(|entry| entry.key() == "state/amount")
            .and_then(|entry| match entry.value() {
                StateValue::Float(value) => Some(*value),
                _ => None,
            })
            .unwrap_or(1.0);
        Ok(processor)
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

    runtime
        .activate(&component, process_config(), DEFAULT_EFFECT_CONFIGURATION)
        .expect("activation succeeds");
    assert!(!runtime.restart_requested());
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
    assert!(
        !runtime.restart_requested(),
        "rejection does not request restart"
    );
}

fn amount_document(gain: f64, amount: f64) -> StateDocument {
    let mut document = complete_document(2, gain, false);
    document
        .insert(StateEntry::new("state/amount", StateValue::Float(amount)))
        .expect("finite custom amount");
    document
}

fn render_amount(runtime: &mut InstanceRuntime<StateProcessor>) -> f64 {
    let input = runtime.audio_port_index(&MAIN_INPUT).expect("input port");
    let output = runtime.audio_port_index(&MAIN_OUTPUT).expect("output port");
    let mut samples = [0.0];
    let mut right = [0.0];
    let mut buffers = [
        ChannelBuffer::in_place(
            InputEndpoint::new(input, 0),
            OutputEndpoint::new(output, 0),
            &mut samples,
            1,
        )
        .unwrap(),
        ChannelBuffer::in_place(
            InputEndpoint::new(input, 1),
            OutputEndpoint::new(output, 1),
            &mut right,
            1,
        )
        .unwrap(),
    ];
    runtime
        .process(
            1,
            ProcessContext::new(
                ProcessMode::Realtime,
                TransportSnapshot::unknown(),
                ParameterEvents::empty_for_block(1),
            ),
            &mut buffers,
        )
        .expect("old resources remain usable");
    samples[0]
}

#[test]
fn active_custom_replacement_requests_restart_until_reactivation() {
    let component = StateAwareEffect::new(Arc::new(AtomicBool::new(false)));
    let mut runtime = InstanceRuntime::for_component(&component).expect("valid runtime");
    let accept = |_: &ParameterStore, _: &[StateEntry]| Ok::<(), Infallible>(());
    runtime
        .apply_state(&amount_document(1.0, 1.0), accept)
        .unwrap();
    assert!(!runtime.restart_requested());
    runtime
        .activate(&component, process_config(), DEFAULT_EFFECT_CONFIGURATION)
        .unwrap();
    assert_eq!(render_amount(&mut runtime).to_bits(), 1.0_f64.to_bits());

    component.processor_restart.store(true, Ordering::Relaxed);
    assert!(runtime.restart_requested(), "processor signal is retained");
    component.processor_restart.store(false, Ordering::Relaxed);
    runtime
        .apply_state(&amount_document(0.75, 1.0), accept)
        .unwrap();
    assert!(
        !runtime.restart_requested(),
        "parameter-only changes do not restart"
    );

    runtime
        .apply_state(&amount_document(0.25, 2.0), accept)
        .unwrap();
    assert!(runtime.restart_requested());
    assert_eq!(render_amount(&mut runtime).to_bits(), 1.0_f64.to_bits());
    let saved = runtime.state_document().unwrap();
    assert_eq!(
        saved.encode().unwrap(),
        amount_document(0.25, 2.0).encode().unwrap()
    );

    for amount in [2.0, 1.0, 2.0] {
        runtime
            .apply_state(&amount_document(0.25, amount), accept)
            .unwrap();
        assert!(
            runtime.restart_requested(),
            "request is sticky even if state reverts"
        );
    }
    let before_rejection = runtime.state_document().unwrap();
    assert!(
        runtime
            .apply_state(&amount_document(1.5, 3.0), |_, _| Err(RejectState))
            .is_err()
    );
    assert_eq!(runtime.state_document().unwrap(), before_rejection);
    assert!(runtime.restart_requested());
    runtime.reset().unwrap();
    assert!(
        runtime.restart_requested(),
        "reset does not adopt custom resources"
    );
    assert_eq!(render_amount(&mut runtime).to_bits(), 1.0_f64.to_bits());

    runtime.deactivate().unwrap();
    assert!(!runtime.restart_requested());
    runtime
        .activate(&component, process_config(), DEFAULT_EFFECT_CONFIGURATION)
        .unwrap();
    assert!(!runtime.restart_requested());
    assert_eq!(render_amount(&mut runtime).to_bits(), 2.0_f64.to_bits());
}

#[test]
fn typed_documents_reject_invalid_custom_values_without_mutation() {
    for (value, expected_error) in invalid_custom_values() {
        let mut document = amount_document(0.5, 1.0);
        let before = document.clone();
        assert_eq!(
            document.insert(StateEntry::new("state/invalid", value)),
            Err(expected_error)
        );
        assert_eq!(document, before);
        assert!(document.encode().is_ok());
    }
}

fn invalid_custom_values() -> [(StateValue, StateDocumentError); 4] {
    [
        (
            StateValue::Float(f64::NAN),
            StateDocumentError::NonFiniteFloat,
        ),
        (
            StateValue::Float(f64::INFINITY),
            StateDocumentError::NonFiniteFloat,
        ),
        (
            StateValue::Float(f64::NEG_INFINITY),
            StateDocumentError::NonFiniteFloat,
        ),
        (
            StateValue::Choice("not valid".into()),
            StateDocumentError::InvalidChoice,
        ),
    ]
}

struct InvalidCustomMigration(StateValue);

impl StateMigration for InvalidCustomMigration {
    type Error = StateDocumentError;

    fn source_schema(&self) -> u32 {
        1
    }
    fn to_schema(&self) -> u32 {
        2
    }

    fn migrate(&self, document: StateDocument) -> Result<StateDocument, Self::Error> {
        let mut migrated = complete_document(2, 0.25, true);
        assert_eq!(document.product_schema(), 1);
        migrated.insert(StateEntry::new("state/invalid", self.0.clone()))?;
        Ok(migrated)
    }
}

#[test]
fn migration_generated_invalid_custom_values_fail_before_live_publication() {
    let component = StateAwareEffect::new(Arc::new(AtomicBool::new(false)));
    let mut runtime = InstanceRuntime::for_component(&component).unwrap();
    runtime
        .apply_state(&amount_document(1.5, 1.0), |_, _| Ok::<(), Infallible>(()))
        .unwrap();
    runtime
        .activate(&component, process_config(), DEFAULT_EFFECT_CONFIGURATION)
        .unwrap();
    let before = runtime.state_document().unwrap();
    let bytes = complete_document(1, 0.5, false).encode().unwrap();
    for (value, expected_error) in invalid_custom_values() {
        let mut product_validation_called = false;
        let result = runtime.apply_state_bytes(
            &bytes,
            StateLimits::default(),
            &[&InvalidCustomMigration(value)],
            |_, _| {
                product_validation_called = true;
                Ok::<(), Infallible>(())
            },
        );
        assert!(matches!(
            result,
            Err(InstanceSemanticStateLoadError::Migration(
                StateMigrationError::Failed(error)
            )) if error == expected_error
        ));
        assert!(!product_validation_called);
        assert_eq!(runtime.state_document().unwrap(), before);
        assert!(!runtime.restart_requested());
        assert!(runtime.encode_state(StateLimits::default()).is_ok());
    }
}

#[test]
fn finite_custom_floats_preserve_the_full_f64_domain() {
    for value in [
        f64::MIN,
        f64::MAX,
        f64::MIN_POSITIVE,
        f64::from(f32::MAX),
        -0.0,
    ] {
        let document = amount_document(0.5, value);
        let decoded = StateDocument::decode(&document.encode().unwrap()).unwrap();
        assert_eq!(
            decoded
                .entries()
                .iter()
                .find(|entry| entry.key() == "state/amount")
                .unwrap()
                .value(),
            &StateValue::Float(value)
        );
    }
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
        .activate(&component, process_config(), DEFAULT_EFFECT_CONFIGURATION)
        .expect("activation succeeds");
    assert!(!runtime.restart_requested());

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
    assert!(
        runtime.restart_requested(),
        "migrated custom changes require restart"
    );

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
    assert!(
        runtime.restart_requested(),
        "rejection retains the pending restart"
    );
}
