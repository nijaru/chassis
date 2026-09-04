//! Negative-space lifecycle tests for durable instance activation.

use std::{
    fmt,
    num::NonZeroU32,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
};

use chassis_core::{
    audio::DEFAULT_EFFECT_CONFIGURATION,
    parameters::{ParameterDescriptor, ParameterValue},
    process::{ActivationConfig, ProcessConfig},
    runtime::{ActivateError, Component, InstanceRuntime, Processor},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ActivationRejected;

impl fmt::Display for ActivationRejected {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("activation rejected by test product")
    }
}

impl std::error::Error for ActivationRejected {}

struct FailableEffect {
    fail_next: AtomicBool,
    activations: Arc<AtomicU32>,
    parameters: Vec<ParameterDescriptor>,
}

impl FailableEffect {
    fn new(activations: Arc<AtomicU32>) -> Self {
        Self {
            fail_next: AtomicBool::new(true),
            activations,
            parameters: vec![
                ParameterDescriptor::float("gain", "Gain", 0.0, 1.0, 0.5)
                    .expect("test parameter is valid"),
            ],
        }
    }
}

impl Component for FailableEffect {
    type Processor = TestProcessor;
    type ActivationError = ActivationRejected;

    fn parameter_descriptors(&self) -> &[ParameterDescriptor] {
        &self.parameters
    }

    fn activate(
        &self,
        _config: &ActivationConfig<'_>,
    ) -> Result<Self::Processor, Self::ActivationError> {
        self.activations.fetch_add(1, Ordering::Relaxed);
        if self.fail_next.swap(false, Ordering::Relaxed) {
            return Err(ActivationRejected);
        }
        Ok(TestProcessor)
    }
}

struct TestProcessor;
impl Processor for TestProcessor {}

fn process_config() -> ProcessConfig {
    ProcessConfig::new(
        48_000.0,
        Some(NonZeroU32::new(1).expect("one is non-zero")),
        NonZeroU32::new(512).expect("512 is non-zero"),
        16,
    )
    .expect("test process configuration is valid")
}

#[test]
fn failed_activation_leaves_runtime_inactive_and_reusable() {
    let activations = Arc::new(AtomicU32::new(0));
    let component = FailableEffect::new(Arc::clone(&activations));
    let mut runtime = InstanceRuntime::for_component(&component).expect("runtime schema is valid");
    runtime
        .parameters_mut()
        .set("gain", ParameterValue::Float(0.75))
        .expect("test value is valid");

    let first = runtime.activate(&component, process_config(), DEFAULT_EFFECT_CONFIGURATION);
    assert!(matches!(
        first,
        Err(ActivateError::Product(ActivationRejected))
    ));
    assert!(!runtime.is_active());
    assert_eq!(
        runtime.parameters().get("gain"),
        Some(&ParameterValue::Float(0.75))
    );

    runtime
        .activate(&component, process_config(), DEFAULT_EFFECT_CONFIGURATION)
        .expect("runtime can activate after a product failure");
    assert!(runtime.is_active());
    assert_eq!(activations.load(Ordering::Relaxed), 2);
    runtime.deactivate().expect("active runtime deactivates");
}
