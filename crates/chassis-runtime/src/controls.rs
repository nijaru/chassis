//! Headless, synchronous parameter intent and gesture protocol.
//!
//! [`InstanceRuntime::controls`](crate::runtime::InstanceRuntime::controls) borrows
//! the canonical desired parameter store, never live DSP. Operations are
//! NON-REALTIME; gesture storage is provisioned on first access. There is no
//! queue, subscription, thread, or implicit host callback.
//!
//! Accepted local operations return ordered notification effects **not yet
//! delivered**. The embedding owns their delivery, in order, outside the runtime
//! borrow. Serialize each accepted operation and its complete effect delivery
//! against further canonical/gesture mutation, including external writes, loads
//! and automation completion. Defer or reject reentrant mutations; read-only
//! queries may reenter. Returned effects are immutable and are not refreshed or
//! filtered by the runtime. Holding them across writes requires a deployment-owned
//! stale-effect protocol.
//! External input produces no outbound notification; observing values is not an
//! edit. Asynchronous delivery/backpressure needs an explicit deployment bridge.

use std::{fmt, sync::Arc};

use crate::{
    automation::ParameterEventValue,
    parameters::{
        ParameterControlError, ParameterEventValueError, ParameterIndex, ParameterMappingError,
        ParameterStore, ParameterValueError,
    },
};

/// Source representation of one scalar edit; not a timed automation point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ControlValue {
    /// Typed plain value. Successful writes clear normalized provenance.
    Plain(ParameterEventValue),
    /// Exact normalized float coordinate, mapped by the prepared descriptor.
    Normalized(f64),
}

/// Whether accepted intent should generate outbound parameter notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlOrigin {
    /// Locally initiated product/application intent; notify the outer embedding.
    Local,
    /// Input from the outer embedding, such as a host; do not echo it back.
    External,
}

/// Gesture authority at the time of a latest-value observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureStatus {
    /// No gesture owns this parameter.
    Idle,
    /// A gesture may change and end this parameter.
    Active,
    /// Complete state replacement invalidated changes, but End is still owed.
    Stale,
}

/// Fixed-width desired-state snapshot, not a change log or an edit command.
///
/// Modulation/effective DSP values are separate. A pending custom-resource
/// replacement can leave DSP consuming older controls than this observation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControlObservation {
    parameter: ParameterIndex,
    value: ParameterEventValue,
    normalized_coordinate: Option<f64>,
    gesture: GestureStatus,
}

impl ControlObservation {
    /// Construct read-only transport from a caller's canonical projection.
    ///
    /// This does not validate or admit parameter state. No canonical mutator
    /// accepts this DTO; the caller must supply its accepted base/provenance pair
    /// and the matching gesture status, not an independently editable mirror.
    #[must_use]
    pub const fn new(
        parameter: ParameterIndex,
        value: ParameterEventValue,
        normalized_coordinate: Option<f64>,
        gesture: GestureStatus,
    ) -> Self {
        Self {
            parameter,
            value,
            normalized_coordinate,
            gesture,
        }
    }

    /// Schema-local parameter identity.
    #[must_use]
    pub const fn parameter(self) -> ParameterIndex {
        self.parameter
    }

    /// Current canonical plain base value.
    #[must_use]
    pub const fn value(self) -> ParameterEventValue {
        self.value
    }

    /// Exact retained float provenance; never an inverse reconstruction.
    ///
    /// `None` also applies to non-floats. This read invokes no author hook.
    #[must_use]
    pub const fn normalized_coordinate(self) -> Option<f64> {
        self.normalized_coordinate
    }

    /// Whether an accepted gesture still has change or closure authority.
    #[must_use]
    pub const fn gesture(self) -> GestureStatus {
        self.gesture
    }
}

/// One ordered effect for delivery by the embedding after semantic application.
///
/// A returned effect is not proof of backend acceptance. Never guess an echo
/// from equal values: external ingress is explicitly separate from local intent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ControlNotification {
    /// Begin a local gesture for this schema-local parameter.
    Begin {
        /// Parameter being controlled.
        parameter: ParameterIndex,
    },
    /// Notify the outer embedding of an accepted local plain base value.
    Value {
        /// Parameter being controlled.
        parameter: ParameterIndex,
        /// Mapped, validated plain base value, without modulation.
        value: ParameterEventValue,
    },
    /// Close the matching accepted local gesture.
    End {
        /// Parameter being controlled.
        parameter: ParameterIndex,
    },
}

/// Accepted synchronous operation and its still-undelivered effects.
#[must_use = "semantic application does not deliver the returned notifications"]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControlOutcome {
    origin: ControlOrigin,
    observation: ControlObservation,
    notifications: [Option<ControlNotification>; 3],
}

impl ControlOutcome {
    /// Explicit ingress origin; observations never create another outcome.
    #[must_use]
    pub const fn origin(self) -> ControlOrigin {
        self.origin
    }

    /// Desired state immediately after this accepted operation.
    #[must_use]
    pub const fn observation(self) -> ControlObservation {
        self.observation
    }

    /// Undelivered effects in required order; at most three, with no allocation.
    pub fn notifications(&self) -> impl Iterator<Item = ControlNotification> + use<> {
        self.notifications.into_iter().flatten()
    }
}

/// Opaque gesture authority belonging to the protocol that issued it.
///
/// This is a control-side handle, not a realtime payload. Dropping it does not
/// notify or end a gesture. Explicitly end it or close the control session before
/// detachment. An accepted state load revokes changes but permits matching End.
#[must_use = "end the gesture or explicitly close the control session"]
#[derive(Debug)]
pub struct GestureToken {
    owner: Arc<()>,
    parameter: ParameterIndex,
    serial: u64,
}

/// Failure before a control operation publishes any change or effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlError {
    /// Dense index does not belong to this immutable schema.
    UnknownParameter(ParameterIndex),
    /// A plain edit failed descriptor validation.
    InvalidValue(ParameterValueError),
    /// A normalized edit failed float/mapping validation.
    Mapping(ParameterControlError),
    /// Another live or stale gesture must be ended first.
    GestureActive(ParameterIndex),
    /// Token belongs elsewhere, was ended, or was superseded by session closure.
    InvalidGesture,
    /// State replacement revoked this token's changes; its End remains valid.
    StateReplaced,
    /// Gesture serials cannot be reused or wrapped.
    GenerationExhausted,
}

impl fmt::Display for ControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownParameter(index) => write!(formatter, "unknown parameter {}", index.get()),
            Self::InvalidValue(error) => error.fmt(formatter),
            Self::Mapping(error) => error.fmt(formatter),
            Self::GestureActive(index) => {
                write!(formatter, "parameter {} has an open gesture", index.get())
            }
            Self::InvalidGesture => formatter.write_str("invalid or ended gesture token"),
            Self::StateReplaced => formatter.write_str("state replacement revoked gesture changes"),
            Self::GenerationExhausted => formatter.write_str("gesture serials exhausted"),
        }
    }
}
impl std::error::Error for ControlError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidValue(error) => Some(error),
            Self::Mapping(error) => Some(error),
            _ => None,
        }
    }
}
impl From<ParameterEventValueError> for ControlError {
    fn from(error: ParameterEventValueError) -> Self {
        match error {
            ParameterEventValueError::UnknownParameter(index) => Self::UnknownParameter(index),
            ParameterEventValueError::InvalidValue(error) => Self::InvalidValue(error),
        }
    }
}
impl From<ParameterMappingError> for ControlError {
    fn from(error: ParameterMappingError) -> Self {
        match error {
            ParameterMappingError::UnknownParameter(index) => Self::UnknownParameter(index),
            ParameterMappingError::Mapping(error) => Self::Mapping(error),
        }
    }
}

struct GestureEntry {
    parameter: ParameterIndex,
    serial: u64,
    stale: bool,
}

/// NON-REALTIME, instance-lifetime gesture metadata, owning no parameter values.
///
/// Embeddings retain one protocol across audio-runtime recreation and compose it
/// with their sole canonical state authority. An immutable parameter count binds
/// dense indices to that authority's schema; no mapping or value admission occurs
/// here. Tokens retain a unique brand even after their protocol is destroyed.
///
/// The caller must exclusively serialize canonical acceptance, metadata changes,
/// and ordered effect delivery. Reserve any backend queue credits and finish
/// fallible admission before changing metadata. Returned effects are not delivered
/// notifications or proof of backend acceptance. This type provides no queue,
/// cross-thread endpoint, host callback, or implicit cleanup on Drop.
pub struct GestureProtocol {
    owner: Arc<()>,
    serial: u64,
    active: Vec<Option<GestureEntry>>,
}
impl GestureProtocol {
    /// Allocate metadata for one immutable schema, off the audio callback.
    #[must_use]
    pub fn new(parameter_count: usize) -> Self {
        Self {
            owner: Arc::new(()),
            serial: 0,
            active: (0..parameter_count).map(|_| None).collect(),
        }
    }

    /// Read gesture authority without observing or admitting any value.
    ///
    /// # Errors
    /// Rejects an index outside the parameter count supplied at construction.
    pub fn status(&self, parameter: ParameterIndex) -> Result<GestureStatus, ControlError> {
        Ok(match self.active[self.position(parameter)?].as_ref() {
            None => GestureStatus::Idle,
            Some(entry) if entry.stale => GestureStatus::Stale,
            Some(_) => GestureStatus::Active,
        })
    }

    /// Check that a discrete edit would not overlap a live or stale gesture.
    ///
    /// This does not reserve a gesture or accept a value.
    ///
    /// # Errors
    /// Rejects unknown parameters and open gestures.
    pub fn ensure_idle(&self, parameter: ParameterIndex) -> Result<(), ControlError> {
        if self.status(parameter)? != GestureStatus::Idle {
            return Err(ControlError::GestureActive(parameter));
        }
        Ok(())
    }

    /// Begin metadata-only local intent and return its undelivered Begin effect.
    ///
    /// # Errors
    /// Rejects unknown parameters, open gestures, and exhausted serials without
    /// changing metadata. The caller must reserve delivery capacity beforehand.
    pub fn begin(
        &mut self,
        parameter: ParameterIndex,
    ) -> Result<(GestureToken, ControlNotification), ControlError> {
        self.ensure_idle(parameter)?;
        let position = self.position(parameter)?;
        let serial = self
            .serial
            .checked_add(1)
            .ok_or(ControlError::GenerationExhausted)?;
        let token = GestureToken {
            owner: Arc::clone(&self.owner),
            parameter,
            serial,
        };
        self.serial = serial;
        self.active[position] = Some(GestureEntry {
            parameter,
            serial,
            stale: false,
        });
        Ok((token, ControlNotification::Begin { parameter }))
    }

    /// Validate change authority and return its parameter without mutation.
    ///
    /// No value is accepted and no Value effect is produced. The caller owns
    /// canonical admission and notification construction after this check.
    ///
    /// # Errors
    /// Rejects foreign, ended, closed, or state-replaced tokens.
    pub fn change_parameter(&self, token: &GestureToken) -> Result<ParameterIndex, ControlError> {
        let entry = self.gesture(token)?;
        if entry.stale {
            return Err(ControlError::StateReplaced);
        }
        Ok(entry.parameter)
    }

    /// Close matching metadata and return its undelivered End effect.
    ///
    /// Stale tokens and serial exhaustion do not prevent closure.
    ///
    /// # Errors
    /// Rejects foreign, ended, or session-closed tokens without changing metadata.
    pub fn end(&mut self, token: &GestureToken) -> Result<ControlNotification, ControlError> {
        self.gesture(token)?;
        let position = self.position(token.parameter)?;
        self.active[position] = None;
        Ok(ControlNotification::End {
            parameter: token.parameter,
        })
    }

    /// Revoke changes after accepted complete state replacement, retaining End.
    ///
    /// NON-REALTIME: visits all parameter slots, without resetting serials or
    /// producing effects. Rejected state replacements must not call this method.
    pub fn invalidate_changes(&mut self) {
        for entry in self.active.iter_mut().flatten() {
            entry.stale = true;
        }
    }

    /// Close all live, stale, or abandoned gestures in parameter order.
    ///
    /// NON-REALTIME: allocates up to one End per parameter. Deliver these effects
    /// before detachment; no destructor or future audio callback does it. Serial
    /// identities are not reset, so old tokens cannot regain authority.
    #[must_use = "deliver the returned End effects before controller detachment"]
    pub fn close(&mut self) -> Vec<ControlNotification> {
        let mut notifications = Vec::with_capacity(self.active.iter().flatten().count());
        for entry in &mut self.active {
            if let Some(entry) = entry.take() {
                notifications.push(ControlNotification::End {
                    parameter: entry.parameter,
                });
            }
        }
        notifications
    }

    fn position(&self, parameter: ParameterIndex) -> Result<usize, ControlError> {
        let position = usize::try_from(parameter.get())
            .map_err(|_| ControlError::UnknownParameter(parameter))?;
        self.active
            .get(position)
            .ok_or(ControlError::UnknownParameter(parameter))?;
        Ok(position)
    }

    fn gesture(&self, token: &GestureToken) -> Result<&GestureEntry, ControlError> {
        if !Arc::ptr_eq(&self.owner, &token.owner) {
            return Err(ControlError::InvalidGesture);
        }
        let position = self.position(token.parameter)?;
        self.active[position]
            .as_ref()
            .filter(|entry| entry.serial == token.serial)
            .ok_or(ControlError::InvalidGesture)
    }
}

pub(crate) fn observe(
    parameters: &ParameterStore,
    protocol: Option<&GestureProtocol>,
    parameter: ParameterIndex,
) -> Result<ControlObservation, ControlError> {
    let value = parameters
        .get_event_value(parameter)
        .ok_or(ControlError::UnknownParameter(parameter))?;
    let normalized_coordinate = if matches!(value, ParameterEventValue::Float(_)) {
        parameters.normalized_coordinate(parameter)?
    } else {
        None
    };
    let gesture = if let Some(protocol) = protocol {
        protocol.status(parameter)?
    } else {
        GestureStatus::Idle
    };
    Ok(ControlObservation::new(
        parameter,
        value,
        normalized_coordinate,
        gesture,
    ))
}

/// Exclusive, NON-REALTIME access to desired parameters and gesture metadata.
///
/// No unrestricted processor access or host callback is exposed. External edits
/// may interleave an open gesture; call order decides the canonical value. State
/// load revokes old changes, not closure. Reset/reactivation preserve gestures.
pub struct ParameterControls<'a> {
    parameters: &'a mut ParameterStore,
    protocol: &'a mut GestureProtocol,
}
impl<'a> ParameterControls<'a> {
    pub(crate) const fn new(
        parameters: &'a mut ParameterStore,
        protocol: &'a mut GestureProtocol,
    ) -> Self {
        Self {
            parameters,
            protocol,
        }
    }

    /// Observe latest desired state without emitting intent or invoking hooks.
    ///
    /// # Errors
    /// Rejects an unknown parameter index.
    pub fn observe(&self, parameter: ParameterIndex) -> Result<ControlObservation, ControlError> {
        observe(self.parameters, Some(self.protocol), parameter)
    }

    /// Begin one local gesture; deliver the returned Begin before later effects.
    ///
    /// # Errors
    /// Rejects unknown parameters, open gestures and exhausted serials.
    pub fn begin(
        &mut self,
        parameter: ParameterIndex,
    ) -> Result<(GestureToken, ControlOutcome), ControlError> {
        let (token, notification) = self.protocol.begin(parameter)?;
        let observation = self.accepted_observation(parameter);
        Ok((
            token,
            ControlOutcome {
                origin: ControlOrigin::Local,
                observation,
                notifications: [Some(notification), None, None],
            },
        ))
    }

    /// Apply a local gesture change and return its mapped plain Value effect.
    ///
    /// # Errors
    /// Rejects invalid/revoked tokens or invalid edits, without changing state.
    pub fn change(
        &mut self,
        token: &GestureToken,
        value: ControlValue,
    ) -> Result<ControlOutcome, ControlError> {
        let parameter = self.protocol.change_parameter(token)?;
        let observation = self.apply(parameter, value)?;
        Ok(ControlOutcome {
            origin: ControlOrigin::Local,
            observation,
            notifications: [
                Some(ControlNotification::Value {
                    parameter,
                    value: observation.value,
                }),
                None,
                None,
            ],
        })
    }

    /// End a matching gesture, including one invalidated by state replacement.
    ///
    /// # Errors
    /// Rejects foreign, ended or session-closed tokens without emitting effects.
    pub fn end(&mut self, token: &GestureToken) -> Result<ControlOutcome, ControlError> {
        let notification = self.protocol.end(token)?;
        let observation = self.accepted_observation(token.parameter);
        Ok(ControlOutcome {
            origin: ControlOrigin::Local,
            observation,
            notifications: [Some(notification), None, None],
        })
    }

    /// Apply a discrete local edit with ordered Begin/Value/End effects.
    ///
    /// All admission precedes application. No transient gesture is published on
    /// rejection. Deliver this batch in order after releasing the runtime borrow.
    ///
    /// # Errors
    /// Rejects unknown parameters, open gestures or invalid values.
    pub fn edit(
        &mut self,
        parameter: ParameterIndex,
        value: ControlValue,
    ) -> Result<ControlOutcome, ControlError> {
        self.protocol.ensure_idle(parameter)?;
        let observation = self.apply(parameter, value)?;
        Ok(ControlOutcome {
            origin: ControlOrigin::Local,
            observation,
            notifications: [
                Some(ControlNotification::Begin { parameter }),
                Some(ControlNotification::Value {
                    parameter,
                    value: observation.value,
                }),
                Some(ControlNotification::End { parameter }),
            ],
        })
    }

    /// Apply outer/host input without retransmitting it, even if values differ.
    ///
    /// Equal plain writes still clear normalized provenance. This does not end
    /// another client's gesture or restore change authority after a state load.
    ///
    /// # Errors
    /// Rejects unknown parameters or invalid values without changing state.
    pub fn external_edit(
        &mut self,
        parameter: ParameterIndex,
        value: ControlValue,
    ) -> Result<ControlOutcome, ControlError> {
        let observation = self.apply(parameter, value)?;
        Ok(ControlOutcome {
            origin: ControlOrigin::External,
            observation,
            notifications: [None; 3],
        })
    }

    /// Close every accepted gesture, including stale or abandoned handles.
    ///
    /// NON-REALTIME: allocates up to one End per parameter. Deliver these effects
    /// before detachment; no token destructor or future audio callback does it.
    /// Serial identities are not reset, so old tokens cannot acquire authority.
    #[must_use = "deliver the returned End effects before controller detachment"]
    pub fn close(&mut self) -> Vec<ControlNotification> {
        self.protocol.close()
    }

    fn apply(
        &mut self,
        parameter: ParameterIndex,
        value: ControlValue,
    ) -> Result<ControlObservation, ControlError> {
        match value {
            ControlValue::Plain(value) => self.parameters.set_event_value(parameter, value)?,
            ControlValue::Normalized(value) => {
                self.parameters.set_normalized_index(parameter, value)?;
            }
        }
        Ok(self.accepted_observation(parameter))
    }

    fn accepted_observation(&self, parameter: ParameterIndex) -> ControlObservation {
        // Construction pairs the protocol with the store's immutable schema;
        // successful metadata or scalar admission guarantees a valid projection.
        self.observe(parameter)
            .expect("accepted control retains a validated parameter projection")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn serial_exhaustion_rejects_begin_without_wrapping_or_losing_end() {
        let a = ParameterIndex::new(0);
        let b = ParameterIndex::new(1);
        let c = ParameterIndex::new(2);
        let mut protocol = GestureProtocol::new(3);
        protocol.serial = u64::MAX - 2;
        let (first, _) = protocol.begin(a).unwrap();
        let (last, _) = protocol.begin(b).unwrap();
        assert!(matches!(
            protocol.begin(c),
            Err(ControlError::GenerationExhausted)
        ));
        assert_eq!(protocol.status(c), Ok(GestureStatus::Idle));
        assert_eq!(protocol.change_parameter(&last), Ok(b));
        protocol.invalidate_changes();
        assert_eq!(
            protocol.end(&last),
            Ok(ControlNotification::End { parameter: b })
        );
        assert_eq!(
            protocol.close(),
            [ControlNotification::End { parameter: a }]
        );
        assert_eq!(
            protocol.change_parameter(&first),
            Err(ControlError::InvalidGesture)
        );
        for parameter in [a, b, c] {
            assert_eq!(protocol.status(parameter), Ok(GestureStatus::Idle));
            assert!(matches!(
                protocol.begin(parameter),
                Err(ControlError::GenerationExhausted)
            ));
        }
    }
}
