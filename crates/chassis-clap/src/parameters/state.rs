//! Non-realtime complete-state projection. No callback enters this module.

use std::{fmt, mem};

use chassis_runtime::{
    parameters::ParameterKind,
    schema::StateIdentity,
    state::{
        StateDocument, StateDocumentError, StateEncodeError, StateEntry, StateLimits, StateValue,
    },
};

use super::{ClapStateProjection, publication::PublicationError};

impl ClapStateProjection {
    /// Hold custom ownership stable while reading one completed scalar/epoch
    /// transaction. The lock is released before encoding, stream I/O or host calls.
    pub(crate) fn state_document(
        &self,
        identity: &StateIdentity,
    ) -> Result<(StateDocument, u64), StateProjectionError> {
        let custom = self
            .custom
            .lock()
            .map_err(|_| StateProjectionError::Poisoned)?;
        let mut values = vec![0.0; self.publication.len()];
        let snapshot = self
            .publication
            .snapshot_control_into(&mut values)
            .map_err(StateProjectionError::Publication)?;
        let mut document = StateDocument::new(
            identity.component().as_str(),
            identity.schema_version().get(),
        )
        .map_err(StateProjectionError::Document)?;
        for (binding, value) in self.bindings.iter().zip(values) {
            let state_value = binding
                .state_value(value)
                .ok_or(StateProjectionError::InvalidValue)?;
            document
                .insert(StateEntry::parameter(
                    binding.descriptor.key().as_str(),
                    state_value,
                ))
                .map_err(StateProjectionError::Document)?;
        }
        for entry in custom.iter() {
            document
                .insert(entry.clone())
                .map_err(StateProjectionError::Document)?;
        }
        Ok((document, snapshot.custom_generation))
    }

    pub(crate) fn encode_state(
        &self,
        identity: &StateIdentity,
        limits: StateLimits,
    ) -> Result<Vec<u8>, StateProjectionError> {
        self.state_document(identity)?
            .0
            .encode_with_limits(limits)
            .map_err(StateProjectionError::Encode)
    }

    // Call only after the component runtime has admitted the complete candidate.
    // Keep structural checks here for the projection's own schema invariant.
    pub(crate) fn apply_state(
        &self,
        document: &StateDocument,
        identity: &StateIdentity,
    ) -> Result<(), StateProjectionError> {
        if document.component_id() != identity.component().as_str() {
            return Err(StateProjectionError::ComponentIdentityMismatch);
        }
        if document.schema_version() != identity.schema_version().get() {
            return Err(StateProjectionError::SchemaVersionMismatch);
        }
        let limits = StateLimits::default();
        let mut maximum_encoded_len = document
            .encoded_len_with_limits(limits)
            .map_err(StateProjectionError::Encode)?;
        let mut candidate: Vec<_> = self
            .bindings
            .iter()
            .map(super::ClapParameterBinding::default_plain)
            .collect();
        let mut custom_candidate = Vec::new();
        let mut parameter_entries = 0;
        for entry in document.entries() {
            let Some(key) = entry.parameter_key() else {
                custom_candidate.push(entry.clone());
                continue;
            };
            let Some(index) = self.index_for_key(key) else {
                return Err(StateProjectionError::UnknownParameter(key.to_owned()));
            };
            let Some(value) = self.bindings[index].state_plain_value(entry.value()) else {
                return Err(StateProjectionError::InvalidValue);
            };
            // CHSS choice payloads are UTF-8 IDs. Reserve the longest declared
            // ID, not just the selected one; every independent control value must
            // remain saveable with this custom payload. Other scalar widths are fixed.
            if let (ParameterKind::Choice { options, .. }, StateValue::Choice(selected)) =
                (self.bindings[index].descriptor.kind(), entry.value())
            {
                let maximum_payload = options
                    .iter()
                    .map(|option| option.id().as_str().len())
                    .max()
                    .expect("validated choice domain is nonempty");
                if maximum_payload > limits.max_payload_bytes {
                    return Err(StateProjectionError::Encode(
                        StateEncodeError::PayloadTooLarge {
                            actual: maximum_payload,
                            maximum: limits.max_payload_bytes,
                        },
                    ));
                }
                // state_plain_value admitted this exact ID from the domain above.
                maximum_encoded_len = maximum_encoded_len
                    .checked_add(maximum_payload - selected.len())
                    .ok_or(StateProjectionError::Encode(StateEncodeError::SizeOverflow))?;
            }
            candidate[index] = value;
            parameter_entries += 1;
        }
        if parameter_entries != self.bindings.len() {
            return Err(StateProjectionError::IncompleteParameterState {
                expected: self.bindings.len(),
                actual: parameter_entries,
            });
        }
        if maximum_encoded_len > limits.max_total_bytes {
            return Err(StateProjectionError::Encode(
                StateEncodeError::TotalSizeTooLarge {
                    actual: maximum_encoded_len,
                    maximum: limits.max_total_bytes,
                },
            ));
        }
        let mut custom = self
            .custom
            .lock()
            .map_err(|_| StateProjectionError::Poisoned)?;
        let changed = *custom != custom_candidate;
        // Exhaustion leaves all fields unchanged. While the mutex is held, no
        // save/activation can observe the interval between the atomic transaction
        // and this infallible ownership swap; RT sees only the fixed-width epoch.
        self.publication
            .publish_values_control(&candidate, changed)
            .map_err(StateProjectionError::Publication)?;
        let retired = mem::replace(&mut *custom, custom_candidate);
        drop(custom);
        drop(retired); // Final payload destruction is always on this control caller.
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) enum StateProjectionError {
    Document(StateDocumentError),
    Encode(StateEncodeError),
    Publication(PublicationError),
    Poisoned,
    ComponentIdentityMismatch,
    SchemaVersionMismatch,
    UnknownParameter(String),
    IncompleteParameterState { expected: usize, actual: usize },
    InvalidValue,
}

impl fmt::Display for StateProjectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Document(error) => write!(formatter, "state document is invalid: {error}"),
            Self::Encode(error) => write!(formatter, "state could not be encoded: {error}"),
            Self::Publication(error) => error.fmt(formatter),
            Self::Poisoned => formatter.write_str("custom-state ownership is poisoned"),
            Self::ComponentIdentityMismatch => {
                formatter.write_str("state has the wrong component identity")
            }
            Self::SchemaVersionMismatch => {
                formatter.write_str("state has the wrong schema version")
            }
            Self::UnknownParameter(key) => write!(formatter, "state names unknown parameter {key}"),
            Self::IncompleteParameterState { expected, actual } => write!(
                formatter,
                "state contains {actual} parameters but {expected} are required"
            ),
            Self::InvalidValue => formatter.write_str("state contains an invalid value"),
        }
    }
}
impl std::error::Error for StateProjectionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Document(error) => Some(error),
            Self::Encode(error) => Some(error),
            Self::Publication(error) => Some(error),
            _ => None,
        }
    }
}
