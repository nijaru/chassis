//! Metadata-only control protocol composed without a `ParameterStore`.

use chassis_runtime::{
    controls::{ControlError, ControlNotification as Notice, GestureProtocol, GestureStatus},
    parameters::ParameterIndex,
};

const A: ParameterIndex = ParameterIndex::new(0);
const B: ParameterIndex = ParameterIndex::new(1);

#[test]
fn independent_authorities_reject_foreign_duplicate_and_ended_tokens() {
    let mut protocol = GestureProtocol::new(2);
    let mut other = GestureProtocol::new(2);
    let (token, begin) = protocol.begin(A).unwrap();
    let (foreign, _) = other.begin(A).unwrap();
    assert_eq!(begin, Notice::Begin { parameter: A });
    assert_eq!(protocol.status(A), Ok(GestureStatus::Active));
    assert_eq!(protocol.ensure_idle(A), Err(ControlError::GestureActive(A)));
    assert!(matches!(
        protocol.begin(A),
        Err(ControlError::GestureActive(A))
    ));
    assert_eq!(protocol.ensure_idle(B), Ok(()));
    assert_eq!(
        protocol.change_parameter(&foreign),
        Err(ControlError::InvalidGesture)
    );
    assert_eq!(protocol.end(&foreign), Err(ControlError::InvalidGesture));
    assert_eq!(
        other.change_parameter(&token),
        Err(ControlError::InvalidGesture)
    );
    // Repeated validation is read-only and does not consume change authority.
    assert_eq!(protocol.change_parameter(&token), Ok(A));
    assert_eq!(protocol.change_parameter(&token), Ok(A));
    assert_eq!(protocol.end(&token), Ok(Notice::End { parameter: A }));
    assert_eq!(protocol.end(&token), Err(ControlError::InvalidGesture));
    let (fresh, _) = protocol.begin(A).unwrap();
    assert_eq!(
        protocol.change_parameter(&token),
        Err(ControlError::InvalidGesture)
    );
    assert_eq!(protocol.change_parameter(&fresh), Ok(A));
    assert_eq!(protocol.end(&fresh), Ok(Notice::End { parameter: A }));
    assert_eq!(other.end(&foreign), Ok(Notice::End { parameter: A }));
}

#[test]
fn replacement_revokes_changes_but_keeps_closure_and_fresh_authority() {
    let mut protocol = GestureProtocol::new(2);
    let (old, _) = protocol.begin(A).unwrap();
    protocol.invalidate_changes();
    protocol.invalidate_changes();
    assert_eq!(protocol.status(A), Ok(GestureStatus::Stale));
    assert_eq!(
        protocol.change_parameter(&old),
        Err(ControlError::StateReplaced)
    );
    assert_eq!(protocol.ensure_idle(A), Err(ControlError::GestureActive(A)));
    assert!(matches!(
        protocol.begin(A),
        Err(ControlError::GestureActive(A))
    ));
    // Invalidation affects only gestures already open, not future authority.
    let (fresh, _) = protocol.begin(B).unwrap();
    assert_eq!(protocol.change_parameter(&fresh), Ok(B));
    assert_eq!(protocol.end(&old), Ok(Notice::End { parameter: A }));
    assert_eq!(protocol.status(A), Ok(GestureStatus::Idle));
    assert_eq!(protocol.end(&fresh), Ok(Notice::End { parameter: B }));
}

#[test]
fn close_collects_stale_and_abandoned_debt_without_reusing_authority() {
    let mut protocol = GestureProtocol::new(2);
    let (old, _) = protocol.begin(B).unwrap();
    protocol.invalidate_changes();
    let (abandoned, _) = protocol.begin(A).unwrap();
    drop(abandoned);
    assert_eq!(protocol.status(A), Ok(GestureStatus::Active));
    assert_eq!(
        protocol.close(),
        [Notice::End { parameter: A }, Notice::End { parameter: B }]
    );
    assert_eq!(protocol.close(), Vec::<Notice>::new());
    assert_eq!(protocol.end(&old), Err(ControlError::InvalidGesture));
    let (fresh, _) = protocol.begin(B).unwrap();
    assert_eq!(
        protocol.change_parameter(&old),
        Err(ControlError::InvalidGesture)
    );
    assert_eq!(protocol.end(&fresh), Ok(Notice::End { parameter: B }));
    drop(protocol);
    let mut replacement = GestureProtocol::new(2);
    let (new_owner, _) = replacement.begin(B).unwrap();
    assert_eq!(replacement.end(&old), Err(ControlError::InvalidGesture));
    assert_eq!(
        replacement.end(&new_owner),
        Ok(Notice::End { parameter: B })
    );
}

#[test]
fn schema_bounds_reject_unknown_indices_without_opening_metadata() {
    let mut protocol = GestureProtocol::new(1);
    assert_eq!(protocol.status(B), Err(ControlError::UnknownParameter(B)));
    assert_eq!(
        protocol.ensure_idle(B),
        Err(ControlError::UnknownParameter(B))
    );
    assert!(matches!(
        protocol.begin(B),
        Err(ControlError::UnknownParameter(B))
    ));
    assert_eq!(protocol.status(A), Ok(GestureStatus::Idle));
    assert_eq!(protocol.close(), Vec::<Notice>::new());
    let mut empty = GestureProtocol::new(0);
    assert!(matches!(
        empty.begin(A),
        Err(ControlError::UnknownParameter(A))
    ));
    assert_eq!(empty.close(), Vec::<Notice>::new());
}
