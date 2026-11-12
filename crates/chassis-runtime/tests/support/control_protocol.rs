//! Headless control client using only the public runtime protocol.

use super::*;
use chassis_runtime::controls::{
    ControlError, ControlNotification as Notice, ControlOrigin, ControlValue as Edit, GestureStatus,
};

fn notices(outcome: &chassis_runtime::controls::ControlOutcome) -> Vec<Notice> {
    outcome.notifications().collect()
}

#[test]
fn stopped_headless_gesture_observation_and_external_echo_share_one_base() {
    let mut runtime = InstanceRuntime::for_component(&Definition::default()).unwrap();
    let initial = runtime.control_observation(GAIN).unwrap();
    assert_eq!(initial.value(), ParameterEventValue::Float(0.0));
    assert_eq!(initial.gesture(), GestureStatus::Idle);
    let (token, begin) = runtime.controls().begin(GAIN).unwrap();
    assert_eq!(notices(&begin), [Notice::Begin { parameter: GAIN }]);
    let changed = runtime
        .controls()
        .change(&token, Edit::Normalized(0.25))
        .unwrap();
    assert_eq!(changed.origin(), ControlOrigin::Local);
    assert_eq!(changed.observation().normalized_coordinate(), Some(0.25));
    assert_eq!(
        notices(&changed),
        [Notice::Value {
            parameter: GAIN,
            value: ParameterEventValue::Float(0.0)
        }]
    );
    assert_eq!(
        runtime.control_observation(GAIN).unwrap(),
        changed.observation()
    );

    // A host echo is external input, not a guessed acknowledgement. Even an
    // equal plain write clears provenance, but never generates another notice.
    let external = runtime
        .controls()
        .external_edit(GAIN, Edit::Plain(ParameterEventValue::Float(0.0)))
        .unwrap();
    assert_eq!(external.origin(), ControlOrigin::External);
    assert_eq!(notices(&external), Vec::<Notice>::new());
    assert_eq!(external.observation().normalized_coordinate(), None);
    assert_eq!(external.observation().gesture(), GestureStatus::Active);
    assert_eq!(
        runtime.control_observation(GAIN).unwrap(),
        external.observation()
    );
    let end = runtime.controls().end(&token).unwrap();
    assert_eq!(notices(&end), [Notice::End { parameter: GAIN }]);
    assert_eq!(end.observation().gesture(), GestureStatus::Idle);
    assert!(!runtime.is_active()); // No processing was needed for any operation.
}

#[test]
fn discrete_local_edits_and_external_normalized_input_are_typed() {
    let mut runtime = InstanceRuntime::for_component(&Definition::default()).unwrap();
    for (parameter, value) in [
        (GAIN, ParameterEventValue::Float(0.75)),
        (BOOL, ParameterEventValue::Boolean(false)),
        (
            ParameterIndex::new(3),
            ParameterEventValue::Choice(ChoiceIndex::new(1)),
        ),
        (ParameterIndex::new(4), ParameterEventValue::Integer(4)),
    ] {
        let edited = runtime
            .controls()
            .edit(parameter, Edit::Plain(value))
            .unwrap();
        assert_eq!(
            notices(&edited),
            [
                Notice::Begin { parameter },
                Notice::Value { parameter, value },
                Notice::End { parameter }
            ]
        );
        assert_eq!(edited.observation().value(), value);
        assert_eq!(edited.observation().gesture(), GestureStatus::Idle);
        assert_eq!(runtime.parameters().get_event_value(parameter), Some(value));
    }
    let external = runtime
        .controls()
        .external_edit(GAIN, Edit::Normalized(0.5))
        .unwrap();
    assert_eq!(notices(&external), Vec::<Notice>::new());
    assert_eq!(
        external.observation().value(),
        ParameterEventValue::Float(0.0)
    );
    assert_eq!(external.observation().normalized_coordinate(), Some(0.5));
    // Low-level backend synchronization is explicitly external/unnotified.
    runtime
        .parameters_mut()
        .set_event_value(GAIN, ParameterEventValue::Float(0.0))
        .unwrap();
    assert_eq!(
        runtime
            .control_observation(GAIN)
            .unwrap()
            .normalized_coordinate(),
        None
    );
}

#[test]
fn protocol_rejection_preserves_values_provenance_and_gesture_authority() {
    let mut runtime = InstanceRuntime::for_component(&Definition::default()).unwrap();
    let mut other = InstanceRuntime::for_component(&Definition::default()).unwrap();
    let (token, _) = runtime.controls().begin(GAIN).unwrap();
    let (foreign, _) = other.controls().begin(GAIN).unwrap();
    let _ = runtime
        .controls()
        .change(&token, Edit::Normalized(0.25))
        .unwrap();
    let before = runtime.control_observation(GAIN).unwrap();
    assert!(matches!(
        runtime.controls().begin(GAIN),
        Err(ControlError::GestureActive(GAIN))
    ));
    assert!(matches!(
        runtime.controls().edit(GAIN, Edit::Normalized(0.5)),
        Err(ControlError::GestureActive(GAIN))
    ));
    assert!(matches!(
        runtime.controls().change(&foreign, Edit::Normalized(0.5)),
        Err(ControlError::InvalidGesture)
    ));
    assert!(matches!(
        runtime.controls().end(&foreign),
        Err(ControlError::InvalidGesture)
    ));
    for invalid in [
        Edit::Normalized(f64::NAN),
        Edit::Normalized(1.1),
        Edit::Plain(ParameterEventValue::Boolean(true)),
    ] {
        assert!(runtime.controls().change(&token, invalid).is_err());
    }
    assert_eq!(runtime.control_observation(GAIN).unwrap(), before);
    let unknown = ParameterIndex::new(99);
    assert!(runtime.controls().begin(unknown).is_err());
    assert!(
        runtime
            .controls()
            .external_edit(unknown, Edit::Normalized(0.5))
            .is_err()
    );
    assert!(runtime.control_observation(unknown).is_err());
    let _ = runtime.controls().end(&token).unwrap();
    assert!(matches!(
        runtime.controls().end(&token),
        Err(ControlError::InvalidGesture)
    ));
    let (new_token, _) = runtime.controls().begin(GAIN).unwrap();
    assert!(matches!(
        runtime.controls().change(&token, Edit::Normalized(0.5)),
        Err(ControlError::InvalidGesture)
    ));
    let _ = runtime
        .controls()
        .change(&new_token, Edit::Normalized(0.5))
        .unwrap();
    let _ = runtime.controls().end(&new_token).unwrap();
}

#[test]
fn invalid_mapping_does_not_begin_a_discrete_edit_or_damage_an_open_gesture() {
    let component = Definition {
        mapping: Mapping::Broken,
        ..Definition::default()
    };
    let mut runtime = InstanceRuntime::for_component(&component).unwrap();
    for coordinate in [0.25, 0.75] {
        assert!(
            runtime
                .controls()
                .edit(GAIN, Edit::Normalized(coordinate))
                .is_err()
        );
        assert_eq!(
            runtime.control_observation(GAIN).unwrap().gesture(),
            GestureStatus::Idle
        );
    }
    let (token, _) = runtime.controls().begin(GAIN).unwrap();
    let _ = runtime
        .controls()
        .change(&token, Edit::Normalized(0.5))
        .unwrap();
    let before = runtime.control_observation(GAIN).unwrap();
    assert!(
        runtime
            .controls()
            .change(&token, Edit::Normalized(0.25))
            .is_err()
    );
    assert_eq!(runtime.control_observation(GAIN).unwrap(), before);
    let _ = runtime.controls().end(&token).unwrap();
}

#[test]
fn accepted_load_invalidates_changes_but_preserves_matching_end_even_if_identical() {
    let component = Definition::default();
    let mut runtime = InstanceRuntime::for_component(&component).unwrap();
    let (token, _) = runtime.controls().begin(GAIN).unwrap();
    let _ = runtime
        .controls()
        .change(&token, Edit::Normalized(0.25))
        .unwrap();
    let state = runtime.state_document().unwrap();
    let invalid = document(&runtime, 2.0, false);
    assert!(runtime.apply_state(&component, &invalid).is_err());
    let _ = runtime
        .controls()
        .change(&token, Edit::Normalized(0.5))
        .unwrap();
    runtime.apply_state(&component, &state).unwrap();
    let loaded = runtime.control_observation(GAIN).unwrap();
    assert_eq!(loaded.value(), ParameterEventValue::Float(0.0));
    assert_eq!(loaded.normalized_coordinate(), None);
    assert_eq!(loaded.gesture(), GestureStatus::Stale);
    assert!(matches!(
        runtime.controls().change(&token, Edit::Normalized(1.0)),
        Err(ControlError::StateReplaced)
    ));
    runtime.apply_state(&component, &state).unwrap(); // Cannot revive the token.
    let _ = runtime
        .controls()
        .external_edit(GAIN, Edit::Normalized(0.25))
        .unwrap();
    assert_eq!(
        runtime.control_observation(GAIN).unwrap().gesture(),
        GestureStatus::Stale
    );
    let end = runtime.controls().end(&token).unwrap();
    assert_eq!(notices(&end), [Notice::End { parameter: GAIN }]);
    let (fresh, _) = runtime.controls().begin(GAIN).unwrap();
    let _ = runtime
        .controls()
        .change(&fresh, Edit::Normalized(1.0))
        .unwrap();
    let _ = runtime.controls().end(&fresh).unwrap();
}

#[test]
fn gesture_lives_across_reset_and_reactivation_without_processor_access() {
    let component = Definition::default();
    let mut runtime = InstanceRuntime::for_component(&component).unwrap();
    let (token, _) = runtime.controls().begin(GAIN).unwrap();
    let _ = runtime
        .controls()
        .change(&token, Edit::Normalized(0.75))
        .unwrap();
    activate(&mut runtime, &component);
    assert_eq!(
        render(&mut runtime, &[]).unwrap().map(f64::to_bits),
        [0.5_f64.to_bits(), 0.75_f64.to_bits()]
    );
    runtime.reset().unwrap();
    assert_eq!(
        runtime.control_observation(GAIN).unwrap().gesture(),
        GestureStatus::Active
    );
    runtime.deactivate().unwrap();
    let _ = runtime
        .controls()
        .change(&token, Edit::Normalized(1.0))
        .unwrap();
    activate(&mut runtime, &component);
    assert_eq!(
        render(&mut runtime, &[]).unwrap().map(f64::to_bits),
        [1.0_f64.to_bits(), 1.0_f64.to_bits()]
    );
    let _ = runtime.controls().end(&token).unwrap();
}

#[test]
fn resource_load_freezes_consumed_controls_not_desired_control_protocol() {
    let component = Definition::default();
    let mut runtime = InstanceRuntime::for_component(&component).unwrap();
    let (old, _) = runtime.controls().begin(GAIN).unwrap();
    let _ = runtime
        .controls()
        .change(&old, Edit::Normalized(0.75))
        .unwrap();
    activate(&mut runtime, &component);
    let loaded = document(&runtime, 0.25, true);
    runtime.apply_state(&component, &loaded).unwrap();
    let _ = runtime.controls().end(&old).unwrap();
    let (new, _) = runtime.controls().begin(GAIN).unwrap();
    let _ = runtime
        .controls()
        .change(&new, Edit::Normalized(1.0))
        .unwrap();
    assert_eq!(
        runtime.control_observation(GAIN).unwrap().value(),
        ParameterEventValue::Float(1.0)
    );
    assert_eq!(
        render(&mut runtime, &[]).unwrap().map(f64::to_bits),
        [0.5_f64.to_bits(), 0.75_f64.to_bits()]
    );
    let saved = runtime.state_document().unwrap();
    assert_eq!(
        saved
            .entries()
            .iter()
            .find(|entry| entry.key() == "parameter/gain")
            .unwrap()
            .value(),
        &StateValue::Float(1.0)
    );
    runtime.reset().unwrap();
    assert_eq!(
        render(&mut runtime, &[]).unwrap().map(f64::to_bits),
        [0.5_f64.to_bits(), 0.75_f64.to_bits()]
    );
    runtime.deactivate().unwrap();
    activate(&mut runtime, &component);
    assert_eq!(
        render(&mut runtime, &[]).unwrap().map(f64::to_bits),
        [3.0_f64.to_bits(), 1.0_f64.to_bits()]
    );
    let _ = runtime.controls().end(&new).unwrap();
}

#[test]
fn explicit_detach_closes_live_and_stale_gestures_without_another_audio_callback() {
    let component = Definition::default();
    let mut runtime = InstanceRuntime::for_component(&component).unwrap();
    let (old, _) = runtime.controls().begin(GAIN).unwrap();
    let state = runtime.state_document().unwrap();
    runtime.apply_state(&component, &state).unwrap();
    let (other, _) = runtime.controls().begin(OTHER).unwrap();
    // Token destruction cannot invoke a host; the protocol retains closure debt.
    drop(other);
    assert_eq!(
        runtime.controls().close(),
        [
            Notice::End { parameter: GAIN },
            Notice::End { parameter: OTHER }
        ]
    );
    assert_eq!(runtime.controls().close(), Vec::<Notice>::new());
    assert!(matches!(
        runtime.controls().end(&old),
        Err(ControlError::InvalidGesture)
    ));
    let (fresh, _) = runtime.controls().begin(GAIN).unwrap();
    assert!(matches!(
        runtime.controls().change(&old, Edit::Normalized(0.5)),
        Err(ControlError::InvalidGesture)
    ));
    let _ = runtime.controls().end(&fresh).unwrap();
}
