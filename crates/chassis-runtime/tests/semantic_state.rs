//! Component-owned state policy and resource-backed deferred-reactivation proof.
//! Codec invalid-value/migration-output cases remain in the state module tests;
//! these cases protect complete-state admission and the DSP/resource transition.

#[path = "support/resource_state.rs"]
mod resource_state;

use chassis_runtime::transport::TransportSnapshot;
use chassis_runtime::{
    audio::{AudioPortIndex, DEFAULT_EFFECT_CONFIGURATION},
    automation::{ParameterEvent, ParameterEventValue, ParameterEvents},
    buffer::{ChannelBuffer, InputEndpoint, OutputEndpoint},
    parameters::{ChoiceIndex, ParameterIndex, ParameterStore, ParameterValue},
    process::{ActivationConfig, ProcessConfig, ProcessContext, ProcessMode},
    runtime::{
        ActivateError, Component, ComponentStateError, InstanceRuntime, InstanceRuntimeError,
        StateValidationError,
    },
    state::{StateDocument, StateEntry, StateLimits, StateValue},
};
use resource_state::{TableComponent, TableProcessor, document};
use std::{num::NonZeroU32, sync::atomic::Ordering};

fn config() -> ProcessConfig {
    ProcessConfig::new(
        48_000.0,
        NonZeroU32::new(1),
        NonZeroU32::new(64).unwrap(),
        8,
    )
    .unwrap()
}
fn render(
    runtime: &mut InstanceRuntime<TableProcessor>,
    events: &[ParameterEvent],
) -> Result<[f64; 4], chassis_runtime::runtime::InstanceProcessError> {
    let mut left = [0.0; 4];
    let mut right = [0.0; 4];
    let mut buffers = [
        ChannelBuffer::in_place(
            InputEndpoint::new(AudioPortIndex::new(0), 0),
            OutputEndpoint::new(AudioPortIndex::new(1), 0),
            &mut left,
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
    runtime.process(
        4,
        ProcessContext::new(
            ProcessMode::Realtime,
            &TransportSnapshot::UNKNOWN,
            ParameterEvents::new(events, 4, 16).unwrap(),
        ),
        &mut buffers,
    )?;
    assert_eq!(left.map(f64::to_bits), right.map(f64::to_bits));
    Ok(left)
}
fn activate(runtime: &mut InstanceRuntime<TableProcessor>, component: &TableComponent) {
    runtime
        .activate(component, config(), DEFAULT_EFFECT_CONFIGURATION)
        .unwrap();
}

#[test]
fn pending_resources_freeze_consumed_modulation_but_keep_desired_signal() {
    let component = TableComponent::default();
    let index = ParameterIndex::new(0);
    let mut runtime = InstanceRuntime::for_component(&component).unwrap();
    activate(&mut runtime, &component);
    runtime
        .apply_state(&component, &document(0.0, &[3, 5]))
        .unwrap();
    let events = [
        ParameterEvent::set(0, index, ParameterEventValue::Float(1.0)),
        ParameterEvent::modulation(0, index, 1.0),
    ];
    assert_eq!(
        render(&mut runtime, &events).unwrap().map(f64::to_bits),
        [2.0; 4].map(f64::to_bits)
    );
    assert_eq!(runtime.modulation(index), Some(1.0));
    assert_eq!(
        runtime.parameters().get("index"),
        Some(&ParameterValue::Float(0.0))
    );
    runtime.reset().unwrap();
    assert_eq!(
        render(&mut runtime, &[]).unwrap().map(f64::to_bits),
        [2.0; 4].map(f64::to_bits)
    );
    runtime
        .apply_state(&component, &document(0.0, &[2]))
        .unwrap();
    assert_eq!(
        render(&mut runtime, &[]).unwrap().map(f64::to_bits),
        [2.0; 4].map(f64::to_bits)
    );
    runtime
        .apply_state(&component, &document(0.0, &[3, 5]))
        .unwrap();
    let saved = runtime.encode_state(StateLimits::default()).unwrap();
    assert_eq!(component.incompatible_controls.load(Ordering::Relaxed), 0);
    runtime.deactivate().unwrap();
    activate(&mut runtime, &component);
    assert_eq!(
        render(&mut runtime, &[]).unwrap().map(f64::to_bits),
        [5.0; 4].map(f64::to_bits)
    );
    let mut reopened = InstanceRuntime::for_component(&component).unwrap();
    reopened
        .apply_state_bytes(&component, &saved, StateLimits::default())
        .unwrap();
    activate(&mut reopened, &component);
    assert_eq!(
        render(&mut reopened, &[]).unwrap().map(f64::to_bits),
        [3.0; 4].map(f64::to_bits)
    );
}

#[test]
fn defaults_are_complete_validated_and_do_not_require_persistence_identity() {
    let component = TableComponent {
        identified: false,
        ..TableComponent::default()
    };
    let mut runtime = InstanceRuntime::for_component(&component).unwrap();
    assert_eq!(runtime.custom_state(), component.defaults);
    activate(&mut runtime, &component);
    assert_eq!(
        render(&mut runtime, &[]).unwrap().map(f64::to_bits),
        [2.0_f64.to_bits(); 4]
    );
    assert!(runtime.state_document().is_err());

    for defaults in [
        vec![StateEntry::new("state/table", StateValue::Bytes(vec![]))],
        vec![
            StateEntry::new("state/table", StateValue::Bytes(vec![2])),
            StateEntry::new("state/table", StateValue::Bytes(vec![3])),
        ],
        vec![StateEntry::new("parameter/index", StateValue::Float(0.0))],
        vec![StateEntry::new("", StateValue::Boolean(false))],
        vec![StateEntry::new("state/table", StateValue::Float(f64::NAN))],
    ] {
        let component = TableComponent {
            defaults,
            ..TableComponent::default()
        };
        assert!(matches!(
            InstanceRuntime::for_component(&component),
            Err(InstanceRuntimeError::InvalidState(_))
        ));
    }
}

#[test]
fn complete_replacement_and_component_rejection_are_failure_atomic() {
    let component = TableComponent::default();
    let mut runtime = InstanceRuntime::for_component(&component).unwrap();
    activate(&mut runtime, &component);
    let before = runtime.state_document().unwrap();
    let mut partial = StateDocument::new("test.resource-table", 2).unwrap();
    partial
        .insert(StateEntry::new("parameter/index", StateValue::Float(0.0)))
        .unwrap();
    for invalid in [partial, document(1.0, &[3]), document(0.0, &[])] {
        assert!(runtime.apply_state(&component, &invalid).is_err());
        assert_eq!(runtime.state_document().unwrap(), before);
        assert!(!runtime.restart_requested());
    }
    assert_eq!(
        render(&mut runtime, &[]).unwrap().map(f64::to_bits),
        [2.0_f64.to_bits(); 4]
    );
}

#[test]
fn pending_complete_generation_never_retargets_old_resources_or_accepts_old_endpoints() {
    let component = TableComponent::default();
    let mut runtime = InstanceRuntime::for_component(&component).unwrap();
    activate(&mut runtime, &component);
    runtime
        .apply_state(&component, &document(0.25, &[2]))
        .unwrap();
    assert!(
        !runtime.restart_requested(),
        "parameter-only loads remain live"
    );
    let replacement = document(1.0, &[3, 7]);
    runtime.apply_state(&component, &replacement).unwrap();
    assert!(runtime.restart_requested());
    assert_eq!(runtime.state_document().unwrap(), replacement);
    // Index 1 is valid for the replacement but not for the still-active table.
    assert_eq!(
        render(&mut runtime, &[]).unwrap().map(f64::to_bits),
        [2.0_f64.to_bits(); 4]
    );
    let automation = [
        ParameterEvent::set(0, ParameterIndex::new(0), ParameterEventValue::Float(0.0)),
        ParameterEvent::set(
            0,
            ParameterIndex::new(1),
            ParameterEventValue::Choice(ChoiceIndex::new(0)),
        ),
        ParameterEvent::linear(3, ParameterIndex::new(0), 1.0),
    ];
    assert_eq!(
        render(&mut runtime, &automation).unwrap().map(f64::to_bits),
        [2.0_f64.to_bits(); 4]
    );
    assert_eq!(
        runtime.state_document().unwrap(),
        replacement,
        "ignored trajectories cannot complete into pending state"
    );
    runtime
        .parameters_mut()
        .set("index", ParameterValue::Float(0.75))
        .unwrap();
    let edited = runtime.state_document().unwrap();
    assert_eq!(
        render(&mut runtime, &automation).unwrap().map(f64::to_bits),
        [2.0_f64.to_bits(); 4]
    );
    assert_eq!(runtime.state_document().unwrap(), edited);
    for next in [
        document(0.0, &[2]),
        replacement.clone(),
        replacement.clone(),
    ] {
        runtime.apply_state(&component, &next).unwrap();
        runtime.reset().unwrap();
        assert!(
            runtime.restart_requested(),
            "reversion/reset cannot clear the first frozen generation"
        );
        assert_eq!(
            render(&mut runtime, &automation).unwrap().map(f64::to_bits),
            [2.0_f64.to_bits(); 4]
        );
    }
    assert!(
        runtime
            .apply_state(&component, &document(1.0, &[9]))
            .is_err()
    );
    assert_eq!(runtime.state_document().unwrap(), replacement);
    assert_eq!(
        component.incompatible_controls.load(Ordering::Relaxed),
        0,
        "old DSP never saw replacement controls"
    );

    let saved = runtime.encode_state(StateLimits::default()).unwrap();
    let mut reopened = InstanceRuntime::for_component(&component).unwrap();
    reopened
        .apply_state_bytes(&component, &saved, StateLimits::default())
        .unwrap();
    activate(&mut reopened, &component);
    assert_eq!(
        render(&mut reopened, &[]).unwrap().map(f64::to_bits),
        [7.0_f64.to_bits(); 4]
    );
    runtime.deactivate().unwrap();
    activate(&mut runtime, &component);
    assert!(!runtime.restart_requested());
    assert_eq!(
        render(&mut runtime, &[]).unwrap().map(f64::to_bits),
        [7.0_f64.to_bits(); 4]
    );
}

#[test]
fn pending_callbacks_still_validate_original_parameter_events() {
    let component = TableComponent::default();
    let mut runtime = InstanceRuntime::for_component(&component).unwrap();
    activate(&mut runtime, &component);
    runtime
        .apply_state(&component, &document(1.0, &[3, 7]))
        .unwrap();
    let before = runtime.state_document().unwrap();
    for events in [
        vec![ParameterEvent::set(
            0,
            ParameterIndex::new(99),
            ParameterEventValue::Float(0.0),
        )],
        vec![ParameterEvent::set(
            0,
            ParameterIndex::new(1),
            ParameterEventValue::Choice(ChoiceIndex::new(9)),
        )],
        vec![ParameterEvent::set(0, ParameterIndex::new(0), ParameterEventValue::Float(0.0)); 9],
    ] {
        assert!(render(&mut runtime, &events).is_err());
        assert_eq!(runtime.state_document().unwrap(), before);
    }
}

#[test]
fn component_migrations_and_schema_admission_are_not_loader_policies() {
    let component = TableComponent::default();
    let mut runtime = InstanceRuntime::for_component(&component).unwrap();
    let current = document(1.0, &[3, 7]);
    let mut legacy = StateDocument::new("test.resource-table", 1).unwrap();
    for entry in current.entries() {
        let key = if entry.key() == "parameter/index" {
            "parameter/old_index"
        } else {
            entry.key()
        };
        legacy
            .insert(StateEntry::new(key, entry.value().clone()))
            .unwrap();
    }
    runtime
        .apply_state_bytes(
            &component,
            &legacy.encode().unwrap(),
            StateLimits::default(),
        )
        .unwrap();
    assert_eq!(runtime.state_document().unwrap(), current);
    let drifted = TableComponent {
        identified: false,
        ..TableComponent::default()
    };
    assert!(matches!(
        runtime.apply_state(&drifted, &current),
        Err(StateValidationError::ComponentSchemaMismatch)
    ));
    let foreign = StateDocument::new("foreign.component", 1)
        .unwrap()
        .encode()
        .unwrap();
    assert!(
        runtime
            .apply_state_bytes(&component, &foreign, StateLimits::default())
            .is_err()
    );
    assert_eq!(runtime.state_document().unwrap(), current);
}

#[test]
fn reactivation_validates_coupled_control_edits_without_publishing_resources() {
    let component = TableComponent::default();
    let mut runtime = InstanceRuntime::for_component(&component).unwrap();
    runtime
        .parameters_mut()
        .set("index", ParameterValue::Float(1.0))
        .unwrap();
    assert!(matches!(
        runtime.activate(&component, config(), DEFAULT_EFFECT_CONFIGURATION),
        Err(ActivateError::InvalidState(_))
    ));
    assert!(!runtime.is_active());
    runtime
        .parameters_mut()
        .set("index", ParameterValue::Float(0.0))
        .unwrap();
    activate(&mut runtime, &component);
}

struct ScalarOnly;
impl Component for ScalarOnly {
    type Processor = TableProcessor;
    type ActivationError = std::convert::Infallible;
    fn schema(
        &self,
    ) -> Result<
        chassis_runtime::schema::ComponentSchema,
        chassis_runtime::schema::ComponentSchemaError,
    > {
        TableComponent::default().schema()
    }
    fn activate(
        &self,
        config: &ActivationConfig<'_>,
        parameters: &ParameterStore,
        custom: &[StateEntry],
    ) -> Result<TableProcessor, Self::ActivationError> {
        TableComponent::default().activate(config, parameters, custom)
    }
}
#[test]
fn parameter_only_policy_rejects_custom_state_instead_of_silently_ignoring_it() {
    let mut runtime = InstanceRuntime::for_component(&ScalarOnly).unwrap();
    let before = runtime.state_document().unwrap();
    assert!(matches!(
        runtime.apply_state(&ScalarOnly, &document(0.0, &[2])),
        Err(StateValidationError::Product(
            ComponentStateError::UnsupportedCustomState
        ))
    ));
    assert_eq!(runtime.state_document().unwrap(), before);
}
