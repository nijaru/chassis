//! Activation-scoped latency contract tests for the durable core runtime.

use std::{
    convert::Infallible,
    num::NonZeroU32,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};

use chassis_core::{
    audio::DEFAULT_EFFECT_CONFIGURATION,
    process::{ActivationConfig, ProcessConfig},
    runtime::{Component, InstanceRuntime, LatencySamples, Processor},
};

struct LatencyComponent {
    next_latency: Arc<AtomicU32>,
}

impl Component for LatencyComponent {
    type Processor = LatencyProcessor;
    type ActivationError = Infallible;

    fn schema(
        &self,
    ) -> Result<chassis_core::schema::ComponentSchema, chassis_core::schema::ComponentSchemaError>
    {
        chassis_core::schema::ComponentSchema::stereo_effect(vec![])
    }

    fn activate(
        &self,
        _config: &ActivationConfig<'_>,
    ) -> Result<Self::Processor, Self::ActivationError> {
        Ok(LatencyProcessor {
            latency: LatencySamples::new(self.next_latency.load(Ordering::Relaxed)),
        })
    }
}

struct LatencyProcessor {
    latency: LatencySamples,
}

impl Processor for LatencyProcessor {
    fn latency(&self) -> LatencySamples {
        self.latency
    }
}

fn process_config() -> ProcessConfig {
    ProcessConfig::new(
        48_000.0,
        Some(NonZeroU32::new(1).expect("test minimum is non-zero")),
        NonZeroU32::new(512).expect("test maximum is non-zero"),
        8,
    )
    .expect("test process configuration is valid")
}

#[test]
fn latency_is_snapshotted_for_each_successful_activation() {
    let next_latency = Arc::new(AtomicU32::new(256));
    let component = LatencyComponent {
        next_latency: Arc::clone(&next_latency),
    };
    let mut runtime =
        InstanceRuntime::for_component(&component).expect("component schema is valid");

    assert_eq!(runtime.active_latency(), None);

    runtime
        .activate(&component, process_config(), DEFAULT_EFFECT_CONFIGURATION)
        .expect("first activation succeeds");
    assert_eq!(runtime.active_latency(), Some(LatencySamples::new(256)));

    next_latency.store(512, Ordering::Relaxed);
    assert_eq!(
        runtime.active_latency(),
        Some(LatencySamples::new(256)),
        "latency must not drift while one activation remains live"
    );

    runtime.deactivate().expect("active runtime deactivates");
    assert_eq!(runtime.active_latency(), None);

    runtime
        .activate(&component, process_config(), DEFAULT_EFFECT_CONFIGURATION)
        .expect("reactivation succeeds");
    assert_eq!(runtime.active_latency(), Some(LatencySamples::new(512)));
}
