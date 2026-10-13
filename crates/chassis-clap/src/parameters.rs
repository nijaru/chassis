use std::{fmt, string::String, vec::Vec};

use chassis_runtime::{
    automation::{ParameterEvent, ParameterEventValue},
    parameters::{
        ChoiceId, ChoiceIndex, ChoiceOption, ParameterDescriptor, ParameterIndex, ParameterKind,
        ParameterStore, ParameterValue, ParameterValuesMut,
    },
    schema::StateIdentity,
    state::{
        StateDocument, StateDocumentError, StateEncodeError, StateEntry, StateLimits, StateValue,
    },
};
use clack_plugin::{
    events::{Event, event_types::ParamValueEvent, io::InputEvents},
    utils::ClapId,
};

mod publication;

use publication::{PublicationError, ScalarPublication};

const EXACT_INTEGER_LIMIT: f64 = 9_007_199_254_740_992.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ParameterMappingError {
    MissingId(String),
    ExtraId(String),
    InvalidId(String),
    DuplicateId(u32),
    ParameterIndexUnrepresentable(usize),
    IntegerRangeNotRepresentable(String),
    ChoiceCountNotRepresentable(String),
    ChoiceLabelsNotRepresentable(String),
}

impl fmt::Display for ParameterMappingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingId(parameter) => {
                write!(formatter, "parameter {parameter} has no CLAP ID mapping")
            }
            Self::ExtraId(parameter) => {
                write!(
                    formatter,
                    "CLAP ID mapping targets unknown parameter {parameter}"
                )
            }
            Self::InvalidId(parameter) => {
                write!(formatter, "parameter {parameter} has an invalid CLAP ID")
            }
            Self::DuplicateId(id) => write!(formatter, "CLAP parameter ID {id} is duplicated"),
            Self::ParameterIndexUnrepresentable(index) => write!(
                formatter,
                "CLAP parameter index {index} cannot fit the Chassis runtime index"
            ),
            Self::IntegerRangeNotRepresentable(parameter) => write!(
                formatter,
                "integer parameter {parameter} exceeds the exactly representable CLAP range"
            ),
            Self::ChoiceCountNotRepresentable(parameter) => write!(
                formatter,
                "choice parameter {parameter} has more options than CLAP plain values represent"
            ),
            Self::ChoiceLabelsNotRepresentable(parameter) => write!(
                formatter,
                "choice parameter {parameter} requires distinct CLAP labels without surrounding whitespace or NUL"
            ),
        }
    }
}

impl std::error::Error for ParameterMappingError {}

#[derive(Debug, Clone)]
pub(crate) struct ClapParameterBinding {
    index: ParameterIndex,
    id: ClapId,
    descriptor: ParameterDescriptor,
}

impl ClapParameterBinding {
    pub(crate) fn index(&self) -> ParameterIndex {
        self.index
    }

    pub(crate) fn id(&self) -> ClapId {
        self.id
    }

    pub(crate) fn descriptor(&self) -> &ParameterDescriptor {
        &self.descriptor
    }

    pub(crate) fn default_plain(&self) -> f64 {
        match self.descriptor.kind() {
            ParameterKind::Float { default, .. } => *default,
            ParameterKind::Integer { default, .. } => integer_to_plain(*default),
            ParameterKind::Boolean { default } => f64::from(u8::from(*default)),
            ParameterKind::Choice { options, default } => choice_plain(options, default),
        }
    }

    pub(crate) fn plain_range(&self) -> (f64, f64) {
        match self.descriptor.kind() {
            ParameterKind::Float {
                minimum, maximum, ..
            } => (*minimum, *maximum),
            ParameterKind::Integer {
                minimum, maximum, ..
            } => (integer_to_plain(*minimum), integer_to_plain(*maximum)),
            ParameterKind::Boolean { .. } => (0.0, 1.0),
            ParameterKind::Choice { options, .. } => (0.0, choice_plain_max(options)),
        }
    }

    /// Validate host plain values before CLAP's cast-equivalent stepped conversion.
    /// Semantic values and decoded state do not pass through this normalization.
    pub(crate) fn normalize_host_plain(&self, value: f64) -> Option<f64> {
        let (minimum, maximum) = self.plain_range();
        if !value.is_finite() || !(minimum..=maximum).contains(&value) {
            return None;
        }
        Some(match self.descriptor.kind() {
            ParameterKind::Float { .. } => value,
            _ => value.trunc(),
        })
    }

    pub(crate) fn flags(&self) -> clack_extensions::params::ParamInfoFlags {
        use clack_extensions::params::ParamInfoFlags;

        let mut flags = ParamInfoFlags::IS_AUTOMATABLE | ParamInfoFlags::REQUIRES_PROCESS;
        if matches!(
            self.descriptor.kind(),
            ParameterKind::Integer { .. }
                | ParameterKind::Boolean { .. }
                | ParameterKind::Choice { .. }
        ) {
            flags |= ParamInfoFlags::IS_STEPPED;
        }
        if matches!(self.descriptor.kind(), ParameterKind::Choice { .. }) {
            flags |= ParamInfoFlags::IS_ENUM;
        }
        flags
    }

    pub(crate) fn event_value(&self, value: f64) -> Option<ParameterEventValue> {
        let value = self.normalize_host_plain(value)?;
        match self.descriptor.kind() {
            ParameterKind::Float { .. } => Some(ParameterEventValue::Float(value)),
            ParameterKind::Integer { .. } => {
                integer_from_plain(value).map(ParameterEventValue::Integer)
            }
            ParameterKind::Boolean { .. } => match value {
                0.0 => Some(ParameterEventValue::Boolean(false)),
                1.0 => Some(ParameterEventValue::Boolean(true)),
                _ => None,
            },
            ParameterKind::Choice { options, .. } => {
                choice_option(options, value)?;
                // Preserve the validated plain stepped projection without
                // carrying schema text into the realtime event stream.
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let index = value as usize;
                let index = u32::try_from(index).ok()?;
                Some(ParameterEventValue::Choice(ChoiceIndex::new(index)))
            }
        }
    }

    pub(crate) fn parameter_value(&self, value: f64) -> Option<ParameterValue> {
        match self.descriptor.kind() {
            ParameterKind::Float {
                minimum, maximum, ..
            } if value.is_finite() && (*minimum..=*maximum).contains(&value) => {
                Some(ParameterValue::Float(value))
            }
            ParameterKind::Float { .. } => None,
            ParameterKind::Integer {
                minimum, maximum, ..
            } => integer_from_plain(value)
                .filter(|value| (*minimum..=*maximum).contains(value))
                .map(ParameterValue::Integer),
            ParameterKind::Boolean { .. } => match value {
                0.0 => Some(ParameterValue::Boolean(false)),
                1.0 => Some(ParameterValue::Boolean(true)),
                _ => None,
            },
            ParameterKind::Choice { options, .. } => choice_option(options, value)
                .map(|option| ParameterValue::Choice(option.id().clone())),
        }
    }

    fn state_value(&self, value: f64) -> Option<StateValue> {
        match self.descriptor.kind() {
            ParameterKind::Float { .. } => Some(StateValue::Float(value)),
            ParameterKind::Integer { .. } => integer_from_plain(value).map(StateValue::Signed),
            ParameterKind::Boolean { .. } => match value {
                0.0 => Some(StateValue::Boolean(false)),
                1.0 => Some(StateValue::Boolean(true)),
                _ => None,
            },
            ParameterKind::Choice { options, .. } => choice_option(options, value)
                .map(|option| StateValue::Choice(option.id().as_str().to_owned())),
        }
    }

    fn plain_value(&self, value: &ParameterValue) -> Option<f64> {
        self.descriptor.validate_value(value).ok()?;
        match value {
            ParameterValue::Float(value) => Some(*value),
            ParameterValue::Integer(value) => Some(integer_to_plain(*value)),
            ParameterValue::Boolean(value) => Some(f64::from(u8::from(*value))),
            ParameterValue::Choice(value) => self.choice_plain(value.as_str()),
        }
    }

    fn state_plain_value(&self, value: &StateValue) -> Option<f64> {
        match (self.descriptor.kind(), value) {
            (ParameterKind::Float { .. }, StateValue::Float(value)) if value.is_finite() => {
                Some(*value)
            }
            (
                ParameterKind::Integer {
                    minimum, maximum, ..
                },
                StateValue::Signed(value),
            ) if (*minimum..=*maximum).contains(value) => {
                let plain = integer_to_plain(*value);
                (integer_from_plain(plain) == Some(*value)).then_some(plain)
            }
            (ParameterKind::Boolean { .. }, StateValue::Boolean(value)) => {
                Some(f64::from(u8::from(*value)))
            }
            (ParameterKind::Choice { .. }, StateValue::Choice(value)) => self.choice_plain(value),
            _ => None,
        }
        .filter(|value| self.parameter_value(*value).is_some())
    }

    fn choice_plain(&self, value: &str) -> Option<f64> {
        let ParameterKind::Choice { options, .. } = self.descriptor.kind() else {
            return None;
        };
        options
            .iter()
            .position(|option| option.id().as_str() == value)
            .and_then(|position| u32::try_from(position).ok())
            .map(f64::from)
    }
}

#[allow(clippy::cast_precision_loss)]
fn integer_to_plain(value: i64) -> f64 {
    value as f64
}

fn integer_from_plain(value: f64) -> Option<i64> {
    if !value.is_finite()
        || value.fract() != 0.0
        || !(-EXACT_INTEGER_LIMIT..=EXACT_INTEGER_LIMIT).contains(&value)
    {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)]
    Some(value as i64)
}

/// Dense option index of a schema's default choice.
fn choice_plain(options: &[ChoiceOption], default: &ChoiceId) -> f64 {
    options
        .iter()
        .position(|option| option.id() == default)
        .map_or(0.0, |position| {
            u32::try_from(position).map_or(0.0, f64::from)
        })
}

/// Highest dense option index of a schema's choice set.
fn choice_plain_max(options: &[ChoiceOption]) -> f64 {
    options
        .len()
        .checked_sub(1)
        .and_then(|last| u32::try_from(last).ok())
        .map_or(0.0, f64::from)
}

/// The option at one dense plain index, rejecting negative, fractional, or
/// out-of-range plain values.
fn choice_option(options: &[ChoiceOption], plain: f64) -> Option<&ChoiceOption> {
    if !plain.is_finite() || plain < 0.0 || plain.fract() != 0.0 {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let index = plain as usize;
    options.get(index)
}

pub(crate) struct ClapParameterState {
    bindings: Vec<ClapParameterBinding>,
    id_lookup: Vec<usize>,
    key_lookup: Vec<usize>,
    publication: ScalarPublication,
    maximum_input_events: u32,
}

impl ClapParameterState {
    pub(crate) fn new(
        descriptors: &[ParameterDescriptor],
        mappings: &[(&str, u32)],
    ) -> Result<Self, ParameterMappingError> {
        let mut bindings = Vec::with_capacity(descriptors.len());
        for (index, descriptor) in descriptors.iter().enumerate() {
            let parameter_index = ParameterIndex::new(
                u32::try_from(index)
                    .map_err(|_| ParameterMappingError::ParameterIndexUnrepresentable(index))?,
            );
            let Some((_, raw_id)) = mappings
                .iter()
                .find(|(key, _)| *key == descriptor.key().as_str())
            else {
                return Err(ParameterMappingError::MissingId(
                    descriptor.key().as_str().to_owned(),
                ));
            };
            let Some(id) = ClapId::from_raw(*raw_id) else {
                return Err(ParameterMappingError::InvalidId(
                    descriptor.key().as_str().to_owned(),
                ));
            };
            match descriptor.kind() {
                ParameterKind::Float { .. } | ParameterKind::Boolean { .. } => {}
                ParameterKind::Integer {
                    minimum, maximum, ..
                } if *minimum >= -(1_i64 << 53) && *maximum <= (1_i64 << 53) => {}
                ParameterKind::Integer { .. } => {
                    return Err(ParameterMappingError::IntegerRangeNotRepresentable(
                        descriptor.key().as_str().to_owned(),
                    ));
                }
                ParameterKind::Choice { options, .. } if u32::try_from(options.len()).is_ok() => {
                    if options.iter().enumerate().any(|(index, option)| {
                        option.name().trim() != option.name()
                            || option.name().contains('\0')
                            || options[..index]
                                .iter()
                                .any(|previous| previous.name() == option.name())
                    }) {
                        return Err(ParameterMappingError::ChoiceLabelsNotRepresentable(
                            descriptor.key().as_str().to_owned(),
                        ));
                    }
                }
                ParameterKind::Choice { .. } => {
                    return Err(ParameterMappingError::ChoiceCountNotRepresentable(
                        descriptor.key().as_str().to_owned(),
                    ));
                }
            }
            bindings.push(ClapParameterBinding {
                index: parameter_index,
                id,
                descriptor: descriptor.clone(),
            });
        }

        for (key, _) in mappings {
            if !descriptors
                .iter()
                .any(|descriptor| descriptor.key().as_str() == *key)
            {
                return Err(ParameterMappingError::ExtraId((*key).to_owned()));
            }
        }
        for (index, binding) in bindings.iter().enumerate() {
            if bindings[..index]
                .iter()
                .any(|previous| previous.id == binding.id)
            {
                return Err(ParameterMappingError::DuplicateId(binding.id.get()));
            }
        }
        if mappings.len() != descriptors.len() {
            let key = descriptors.first().map_or_else(
                || "mapping count".to_owned(),
                |descriptor| descriptor.key().as_str().to_owned(),
            );
            return Err(ParameterMappingError::MissingId(key));
        }

        let mut id_lookup: Vec<_> = (0..bindings.len()).collect();
        id_lookup.sort_unstable_by_key(|index| bindings[*index].id);
        let mut key_lookup: Vec<_> = (0..bindings.len()).collect();
        key_lookup.sort_unstable_by(|left, right| {
            bindings[*left]
                .descriptor
                .key()
                .cmp(bindings[*right].descriptor.key())
        });
        let initial_values: Vec<_> = bindings
            .iter()
            .map(ClapParameterBinding::default_plain)
            .collect();

        Ok(Self {
            bindings,
            id_lookup,
            key_lookup,
            publication: ScalarPublication::new(&initial_values),
            maximum_input_events: 1024,
        })
    }

    pub(crate) fn with_input_event_bound(mut self, maximum: u32) -> Self {
        self.maximum_input_events = maximum;
        self
    }

    pub(crate) fn bindings(&self) -> &[ClapParameterBinding] {
        &self.bindings
    }

    fn index_for_id(&self, id: ClapId) -> Option<usize> {
        self.id_lookup
            .binary_search_by_key(&id, |index| self.bindings[*index].id)
            .ok()
            .map(|position| self.id_lookup[position])
    }

    fn index_for_key(&self, key: &str) -> Option<usize> {
        self.key_lookup
            .binary_search_by(|index| self.bindings[*index].descriptor.key().as_str().cmp(key))
            .ok()
            .map(|position| self.key_lookup[position])
    }

    pub(crate) fn value(&self, index: usize) -> f64 {
        self.publication.value(index)
    }

    pub(crate) fn apply_input(&self, input: &InputEvents<'_>) {
        // Flush has no failure return: reject oversized batches before mutation.
        if input.len() > self.maximum_input_events {
            return;
        }
        for event in input {
            let Some(event) = event.as_event::<ParamValueEvent>() else {
                continue;
            };
            if !event.pckn().matches_all() {
                continue;
            }
            let Some(id) = event.param_id() else {
                continue;
            };
            let _ = self.try_apply_plain_value(id, event.value());
        }
    }

    #[cfg(test)]
    pub(crate) fn apply_plain_value(&self, id: ClapId, value: f64) -> bool {
        let Some((index, value)) = self.validated_plain_value(id, value) else {
            return false;
        };
        self.publication.publish_value_control(index, value).is_ok()
    }

    fn try_apply_plain_value(&self, id: ClapId, value: f64) -> bool {
        let Some((index, value)) = self.validated_plain_value(id, value) else {
            return false;
        };
        self.publication.try_publish_value(index, value)
    }

    fn validated_plain_value(&self, id: ClapId, value: f64) -> Option<(usize, f64)> {
        let index = self.index_for_id(id)?;
        let value = self.bindings[index].normalize_host_plain(value)?;
        let value = self.bindings[index].parameter_value(value)?;
        let value = match value {
            ParameterValue::Float(value) => value,
            ParameterValue::Integer(value) => integer_to_plain(value),
            ParameterValue::Boolean(value) => f64::from(u8::from(value)),
            ParameterValue::Choice(value) => self.bindings[index].choice_plain(value.as_str())?,
        };
        Some((index, value))
    }

    /// Synchronize one coherent generation, or keep the last values on contention.
    /// The cache certifies the whole store; callers invalidate it after any local
    /// mutation that was not successfully published.
    pub(crate) fn sync_into(
        &self,
        mut store: ParameterValuesMut<'_>,
        scratch: &mut [f64],
        synchronized_generation: Option<u64>,
    ) -> Result<Option<u64>, ParameterSyncError> {
        if scratch.len() != self.publication.len() {
            return Err(ParameterSyncError::InvalidPublishedValue);
        }
        let current = self.publication.completed_generation();
        if current.is_some() && current == synchronized_generation {
            return Ok(current);
        }
        let Some(generation) = self.publication.try_snapshot_into(scratch) else {
            return Ok(None);
        };
        if self
            .bindings
            .iter()
            .zip(scratch.iter().copied())
            .any(|(binding, value)| binding.parameter_value(value).is_none())
        {
            return Err(ParameterSyncError::InvalidPublishedValue);
        }
        for (binding, value) in self.bindings.iter().zip(scratch.iter().copied()) {
            let parameter = binding
                .parameter_value(value)
                .ok_or(ParameterSyncError::InvalidPublishedValue)?;
            store
                .set_index(binding.index(), parameter)
                .map_err(|_| ParameterSyncError::StoreRejected)?;
        }
        Ok(Some(generation))
    }

    /// Project completed runtime values without replaying automation. The token
    /// must be the baseline actually synchronized before this block's DSP.
    pub(crate) fn publish_parameters(
        &self,
        store: &ParameterStore,
        expected_generation: Option<u64>,
        scratch: &mut [f64],
    ) -> Result<Option<u64>, ParameterSyncError> {
        if scratch.len() != self.publication.len() {
            return Err(ParameterSyncError::InvalidPublishedValue);
        }
        let Some(generation) = expected_generation else {
            return Ok(None);
        };
        for (binding, target) in self.bindings.iter().zip(scratch.iter_mut()) {
            *target = store
                .get_index(binding.index())
                .and_then(|value| binding.plain_value(value))
                .ok_or(ParameterSyncError::StoreRejected)?;
        }
        if self
            .publication
            .try_publish_values_from(generation, scratch)
        {
            // Successful CAS guarantees that generation arithmetic did not wrap.
            Ok(Some(generation + 2))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn encode_state(
        &self,
        identity: &StateIdentity,
        limits: StateLimits,
    ) -> Result<Vec<u8>, ParameterStateError> {
        let mut values = vec![0.0; self.publication.len()];
        self.publication
            .snapshot_control_into(&mut values)
            .map_err(publication_state_error)?;
        let mut document =
            StateDocument::new(identity.component().as_str(), identity.schema().get())
                .map_err(ParameterStateError::Document)?;
        for (binding, value) in self.bindings.iter().zip(values) {
            let state_value = binding
                .state_value(value)
                .ok_or(ParameterStateError::InvalidValue)?;
            document
                .insert(StateEntry::new(
                    parameter_state_key(binding.descriptor.key().as_str()),
                    state_value,
                ))
                .map_err(ParameterStateError::Document)?;
        }
        document
            .encode_with_limits(limits)
            .map_err(ParameterStateError::Encode)
    }

    pub(crate) fn apply_state(
        &self,
        document: &StateDocument,
        identity: &StateIdentity,
    ) -> Result<(), ParameterStateError> {
        if document.product_id() != identity.component().as_str() {
            return Err(ParameterStateError::ProductIdentityMismatch);
        }
        if document.product_schema() != identity.schema().get() {
            return Err(ParameterStateError::ProductSchemaMismatch);
        }
        let mut candidate: Vec<_> = self
            .bindings
            .iter()
            .map(ClapParameterBinding::default_plain)
            .collect();
        let mut parameter_entries = 0_usize;
        for entry in document.entries() {
            let Some(key) = entry.key().strip_prefix("parameter/") else {
                return Err(ParameterStateError::UnsupportedCustomState(
                    entry.key().to_owned(),
                ));
            };
            let Some(index) = self.index_for_key(key) else {
                return Err(ParameterStateError::UnknownParameter(key.to_owned()));
            };
            let Some(value) = self.bindings[index].state_plain_value(entry.value()) else {
                return Err(ParameterStateError::InvalidValue);
            };
            candidate[index] = value;
            parameter_entries += 1;
        }
        if parameter_entries != self.bindings.len() {
            return Err(ParameterStateError::IncompleteParameterState {
                expected: self.bindings.len(),
                actual: parameter_entries,
            });
        }
        self.publication
            .publish_values_control(&candidate)
            .map_err(publication_state_error)
    }
}

fn parameter_state_key(key: &str) -> String {
    let mut state_key = String::from("parameter/");
    state_key.push_str(key);
    state_key
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParameterSyncError {
    InvalidPublishedValue,
    StoreRejected,
}

impl fmt::Display for ParameterSyncError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidPublishedValue => "parameter projection contained an invalid value",
            Self::StoreRejected => "parameter projection was rejected by the active schema",
        })
    }
}

impl std::error::Error for ParameterSyncError {}

#[derive(Debug)]
pub(crate) enum ParameterStateError {
    Document(StateDocumentError),
    Encode(StateEncodeError),
    PublicationGenerationExhausted,
    InvalidPublicationValueCount,
    ProductIdentityMismatch,
    ProductSchemaMismatch,
    UnknownParameter(String),
    UnsupportedCustomState(String),
    IncompleteParameterState { expected: usize, actual: usize },
    InvalidValue,
}

impl fmt::Display for ParameterStateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Document(error) => {
                write!(formatter, "parameter state document is invalid: {error}")
            }
            Self::Encode(error) => {
                write!(formatter, "parameter state could not be encoded: {error}")
            }
            Self::PublicationGenerationExhausted => {
                formatter.write_str("parameter publication generation is exhausted")
            }
            Self::InvalidPublicationValueCount => {
                formatter.write_str("parameter publication has the wrong value count")
            }
            Self::ProductIdentityMismatch => {
                formatter.write_str("parameter state has the wrong product identity")
            }
            Self::ProductSchemaMismatch => {
                formatter.write_str("parameter state has the wrong product schema")
            }
            Self::UnknownParameter(parameter) => write!(
                formatter,
                "parameter state names unknown parameter {parameter}"
            ),
            Self::UnsupportedCustomState(key) => write!(
                formatter,
                "CLAP scalar state projection cannot preserve custom field {key}"
            ),
            Self::IncompleteParameterState { expected, actual } => write!(
                formatter,
                "parameter state contains {actual} parameters but {expected} are required"
            ),
            Self::InvalidValue => formatter.write_str("parameter state contains an invalid value"),
        }
    }
}

impl std::error::Error for ParameterStateError {}

fn publication_state_error(error: PublicationError) -> ParameterStateError {
    match error {
        PublicationError::GenerationExhausted => {
            ParameterStateError::PublicationGenerationExhausted
        }
        PublicationError::InvalidValueCount => ParameterStateError::InvalidPublicationValueCount,
    }
}

pub(crate) fn normalized_event(
    state: &ClapParameterState,
    id: ClapId,
    time: u32,
    value: f64,
) -> Option<ParameterEvent> {
    let index = state.index_for_id(id)?;
    let binding = &state.bindings[index];
    Some(ParameterEvent::set(
        time,
        binding.index(),
        binding.event_value(value)?,
    ))
}

pub(crate) fn normalized_events(
    state: &ClapParameterState,
    input: &InputEvents<'_>,
    output: &mut Vec<ParameterEvent>,
) -> Result<(), &'static str> {
    output.clear();
    if input.len() > state.maximum_input_events {
        return Err("CLAP input event count exceeds the activation bound");
    }
    for event in input {
        let Some(event) = event.as_event::<ParamValueEvent>() else {
            continue;
        };
        if !event.pckn().matches_all() {
            return Err("CLAP targeted parameter events are unsupported");
        }
        let Some(id) = event.param_id() else {
            continue;
        };
        let Some(normalized) = normalized_event(state, id, event.time(), event.value()) else {
            continue;
        };
        if output.len() == output.capacity() {
            return Err("CLAP parameter event count exceeds the activation bound");
        }
        output.push(normalized);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chassis_runtime::{
        automation::{ParameterEventChange, ParameterEvents},
        parameters::{ChoiceId, ChoiceOption, ParameterDescriptor, ParameterStore},
        schema::{ComponentId, StateSchemaVersion},
        state::StateEntry,
    };
    use clack_plugin::events::Pckn;

    fn state_identity() -> StateIdentity {
        StateIdentity::new(
            ComponentId::new("com.example.test").expect("semantic identity is valid"),
            StateSchemaVersion::new(1),
        )
    }

    fn descriptors() -> Vec<ParameterDescriptor> {
        vec![
            ParameterDescriptor::float("gain", "Gain", 0.0, 1.0, 0.5)
                .expect("float parameter is valid"),
            ParameterDescriptor::integer("steps", "Steps", -4, 4, 0)
                .expect("integer parameter is valid"),
            ParameterDescriptor::boolean("bypass", "Bypass", false)
                .expect("boolean parameter is valid"),
        ]
    }

    #[test]
    fn stepped_host_values_truncate_but_semantic_projection_stays_strict() {
        let state =
            ClapParameterState::new(&descriptors(), &[("gain", 1), ("steps", 2), ("bypass", 3)])
                .unwrap();
        let integer = &state.bindings()[1];
        let boolean = &state.bindings()[2];
        for (plain, expected) in [
            (0.0, 0.0),
            (0.0101, 0.0),
            (0.49, 0.0),
            (0.5, 0.0),
            (0.99, 0.0),
            (1.0, 1.0),
        ] {
            // CLAP params.h: stepped doubles convert using a cast (trunc).
            assert_eq!(boolean.normalize_host_plain(plain), Some(expected));
            assert_eq!(
                boolean.event_value(plain),
                Some(ParameterEventValue::Boolean(expected > 0.0))
            );
            assert!(state.apply_plain_value(ClapId::new(3), plain));
            assert_eq!(state.value(2).to_bits(), expected.to_bits());
        }
        for (plain, expected) in [(-2.7, -2), (2.7, 2)] {
            assert_eq!(
                integer.normalize_host_plain(plain),
                Some(integer_to_plain(expected))
            );
            assert_eq!(
                integer.event_value(plain),
                Some(ParameterEventValue::Integer(expected))
            );
            assert!(state.apply_plain_value(ClapId::new(2), plain));
            assert_eq!(
                state.value(1).to_bits(),
                integer_to_plain(expected).to_bits()
            );
        }
        for invalid in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
            assert_eq!(boolean.normalize_host_plain(invalid), None);
            assert_eq!(boolean.event_value(invalid), None);
            assert!(!state.apply_plain_value(ClapId::new(3), invalid));
        }
        for binding in [integer, boolean] {
            assert!(binding.parameter_value(0.25).is_none());
            assert!(binding.state_value(0.25).is_none());
            assert!(
                binding
                    .state_plain_value(&StateValue::Float(0.25))
                    .is_none()
            );
        }
    }

    #[test]
    fn mapping_accepts_choices_and_requires_representable_option_counts() {
        let choice = ParameterDescriptor::choice(
            "mode",
            "Mode",
            vec![
                ChoiceOption::new("clean", "Clean").expect("choice is valid"),
                ChoiceOption::new("warm", "Warm").expect("choice is valid"),
            ],
            "warm",
        )
        .expect("choice parameter is valid");
        let state =
            ClapParameterState::new(&[choice], &[("mode", 1)]).expect("choice parameters map");
        let binding = &state.bindings()[0];
        assert_eq!(binding.plain_range(), (0.0, 1.0));
        assert!((binding.default_plain() - 1.0).abs() <= f64::EPSILON);
        assert!(
            binding
                .flags()
                .contains(clack_extensions::params::ParamInfoFlags::IS_ENUM)
        );
        assert_eq!(binding.normalize_host_plain(0.99), Some(0.0));
        assert_eq!(
            binding.event_value(0.99),
            Some(ParameterEventValue::Choice(ChoiceIndex::new(0)))
        );
        assert!(state.apply_plain_value(ClapId::new(1), 0.99));
        assert_eq!(state.value(0).to_bits(), 0.0_f64.to_bits());

        let descriptors = descriptors();
        assert!(matches!(
            ClapParameterState::new(
                &descriptors,
                &[("gain", u32::MAX), ("steps", 2), ("bypass", 3)]
            ),
            Err(ParameterMappingError::InvalidId(_))
        ));
        assert!(
            ClapParameterState::new(&descriptors, &[("gain", 1), ("steps", 2), ("bypass", 3)])
                .is_ok()
        );
    }

    #[test]
    fn choice_mapping_rejects_labels_that_cannot_text_round_trip() {
        for labels in [
            ["Same", "Same"],
            [" Clean", "Warm"],
            ["Clean", "Warm "],
            ["Clean", "Warm\0"],
        ] {
            let descriptor = ParameterDescriptor::choice(
                "mode",
                "Mode",
                vec![
                    ChoiceOption::new("clean", labels[0]).unwrap(),
                    ChoiceOption::new("warm", labels[1]).unwrap(),
                ],
                "clean",
            )
            .unwrap();
            assert!(ClapParameterState::new(&[descriptor], &[("mode", 1)]).is_err());
        }
    }

    #[test]
    fn integer_mapping_rejects_values_rounded_into_the_exact_boundary() {
        for value in [-(1_i64 << 53) - 1, (1_i64 << 53) + 1] {
            let descriptor = ParameterDescriptor::integer("large", "Large", value, value, value)
                .expect("integer schema is valid");
            assert!(matches!(
                ClapParameterState::new(&[descriptor], &[("large", 1)]),
                Err(ParameterMappingError::IntegerRangeNotRepresentable(_))
            ));
        }
    }

    #[test]
    fn total_input_bound_rejects_unknown_events_and_entire_flush_batch() {
        let descriptor =
            ParameterDescriptor::float("gain", "Gain", 0.0, 1.0, 0.0).expect("schema is valid");
        let state = ClapParameterState::new(&[descriptor], &[("gain", 1)])
            .expect("mapping is valid")
            .with_input_event_bound(1);
        let raw = [
            ParamValueEvent::new(0, ClapId::new(1), Pckn::match_all(), 0.25),
            ParamValueEvent::new(0, ClapId::new(999), Pckn::match_all(), 0.75),
        ];
        let input = InputEvents::from_buffer(&raw);
        let mut normalized = Vec::with_capacity(2);
        assert_eq!(
            normalized_events(&state, &input, &mut normalized),
            Err("CLAP input event count exceeds the activation bound")
        );
        assert_eq!(normalized, []);
        state.apply_input(&input);
        assert_eq!(state.value(0).to_bits(), 0.0_f64.to_bits());
    }

    #[test]
    fn targeted_parameter_events_never_become_global_updates() {
        use clack_plugin::events::Match;
        let descriptor =
            ParameterDescriptor::float("gain", "Gain", 0.0, 1.0, 0.0).expect("schema is valid");
        let state =
            ClapParameterState::new(&[descriptor], &[("gain", 1)]).expect("mapping is valid");
        for target in [
            Pckn::new(0_u16, Match::All, Match::All, Match::All),
            Pckn::new(Match::All, 0_u16, Match::All, Match::All),
            Pckn::new(Match::All, Match::All, 60_u16, Match::All),
            Pckn::new(Match::All, Match::All, Match::All, 1_u32),
        ] {
            let raw = [ParamValueEvent::new(0, ClapId::new(1), target, 0.75)];
            let input = InputEvents::from_buffer(&raw);
            let mut normalized = Vec::with_capacity(1);
            assert_eq!(
                normalized_events(&state, &input, &mut normalized),
                Err("CLAP targeted parameter events are unsupported")
            );
            state.apply_input(&input);
            assert_eq!(state.value(0).to_bits(), 0.0_f64.to_bits());
        }
    }

    #[test]
    fn input_events_normalize_to_dense_parameter_indices() {
        let descriptors = descriptors();
        let state =
            ClapParameterState::new(&descriptors, &[("gain", 1), ("steps", 2), ("bypass", 3)])
                .expect("parameter mapping is valid");
        assert_eq!(state.bindings()[0].index(), ParameterIndex::new(0));
        assert_eq!(state.bindings()[1].index(), ParameterIndex::new(1));
        let raw = [
            ParamValueEvent::new(0, ClapId::new(1), Pckn::match_all(), 0.25),
            ParamValueEvent::new(2, ClapId::new(2), Pckn::match_all(), 3.0),
        ];
        let input = InputEvents::from_buffer(&raw);
        let mut normalized = Vec::with_capacity(2);
        normalized_events(&state, &input, &mut normalized).expect("events normalize");
        let events = ParameterEvents::new(&normalized, 4, 2).expect("events are valid");

        assert_eq!(events.len(), 2);
        assert_eq!(
            events.iter().next().expect("first event").parameter(),
            ParameterIndex::new(0)
        );
        assert_eq!(
            events.iter().nth(1).expect("second event").parameter(),
            ParameterIndex::new(1)
        );
        assert_eq!(
            events.iter().nth(1).expect("second event").change(),
            ParameterEventChange::Set(ParameterEventValue::Integer(3))
        );

        let mut bounded = Vec::with_capacity(1);
        assert_eq!(
            normalized_events(&state, &input, &mut bounded),
            Err("CLAP parameter event count exceeds the activation bound")
        );
    }

    #[test]
    fn control_projection_syncs_transactionally_into_active_store() {
        let descriptors = descriptors();
        let state =
            ClapParameterState::new(&descriptors, &[("gain", 1), ("steps", 2), ("bypass", 3)])
                .expect("parameter mapping is valid");
        assert!(state.apply_plain_value(ClapId::new(1), 0.75));
        assert!(state.apply_plain_value(ClapId::new(2), -2.0));
        let mut store = ParameterStore::new(&descriptors).expect("schema is valid");
        let mut scratch = vec![0.0; 3];
        state
            .sync_into(store.values_mut(), &mut scratch, None)
            .expect("projection syncs");
        assert_eq!(store.get("gain"), Some(&ParameterValue::Float(0.75)));
        assert_eq!(store.get("steps"), Some(&ParameterValue::Integer(-2)));

        store.reset();
        state
            .sync_into(store.values_mut(), &mut scratch, None)
            .expect("new runtime with no cached generation syncs");
        assert_eq!(store.get("gain"), Some(&ParameterValue::Float(0.75)));
        assert_eq!(store.get("steps"), Some(&ParameterValue::Integer(-2)));
    }

    #[test]
    fn completed_runtime_projection_preserves_untouched_values_and_restores_newer_control() {
        let descriptors = descriptors();
        let state =
            ClapParameterState::new(&descriptors, &[("gain", 1), ("steps", 2), ("bypass", 3)])
                .expect("parameter mapping is valid");
        let mut store = ParameterStore::new(&descriptors).expect("schema is valid");
        let mut scratch = vec![0.0; 3];
        assert!(state.apply_plain_value(ClapId::new(2), -2.0));
        let baseline = state
            .sync_into(store.values_mut(), &mut scratch, None)
            .unwrap();
        store.set("gain", ParameterValue::Float(0.75)).unwrap();
        let published = state
            .publish_parameters(&store, baseline, &mut scratch)
            .unwrap();
        assert!(published.is_some());
        assert!((state.value(0) - 0.75).abs() <= f64::EPSILON);
        assert!((state.value(1) + 2.0).abs() <= f64::EPSILON);

        // A control edit after the block baseline wins over the whole completed
        // store, including the untouched integer and local automated float.
        store.set("gain", ParameterValue::Float(0.25)).unwrap();
        assert!(state.apply_plain_value(ClapId::new(2), 4.0));
        let rejected = state
            .publish_parameters(&store, published, &mut scratch)
            .unwrap();
        assert!(rejected.is_none());
        let synchronized = state
            .sync_into(store.values_mut(), &mut scratch, rejected)
            .unwrap();
        assert!(synchronized.is_some());
        assert_eq!(store.get("gain"), Some(&ParameterValue::Float(0.75)));
        assert_eq!(store.get("steps"), Some(&ParameterValue::Integer(4)));

        // A missing start token cannot be reacquired after DSP.
        store.set("gain", ParameterValue::Float(0.0)).unwrap();
        assert!(
            state
                .publish_parameters(&store, None, &mut scratch)
                .unwrap()
                .is_none()
        );
        state
            .sync_into(store.values_mut(), &mut scratch, None)
            .unwrap();
        assert_eq!(store.get("gain"), Some(&ParameterValue::Float(0.75)));
    }

    #[test]
    fn state_projection_rejects_invalid_values_transactionally() {
        let descriptors = descriptors();
        let state =
            ClapParameterState::new(&descriptors, &[("gain", 1), ("steps", 2), ("bypass", 3)])
                .expect("parameter mapping is valid");
        assert!(state.apply_plain_value(ClapId::new(1), 0.75));
        let mut document = StateDocument::new("com.example.test", 1).expect("document is valid");
        document
            .insert(StateEntry::new("parameter/gain", StateValue::Float(2.0)))
            .expect("entry is structurally valid");

        assert!(matches!(
            state.apply_state(&document, &state_identity()),
            Err(ParameterStateError::InvalidValue)
        ));
        assert!((state.value(0) - 0.75).abs() <= f64::EPSILON);
    }

    #[test]
    fn state_projection_requires_a_complete_parameter_snapshot() {
        let descriptors = descriptors();
        let state =
            ClapParameterState::new(&descriptors, &[("gain", 1), ("steps", 2), ("bypass", 3)])
                .expect("parameter mapping is valid");
        let mut document = StateDocument::new("com.example.test", 1).expect("document is valid");
        document
            .insert(StateEntry::new("parameter/gain", StateValue::Float(0.75)))
            .expect("entry is structurally valid");

        assert!(matches!(
            state.apply_state(&document, &state_identity()),
            Err(ParameterStateError::IncompleteParameterState {
                expected: 3,
                actual: 1
            })
        ));
        assert!((state.value(0) - 0.5).abs() <= f64::EPSILON);
    }

    #[test]
    fn state_projection_rejects_custom_fields_without_publishing_parameters() {
        let descriptors = descriptors();
        let state =
            ClapParameterState::new(&descriptors, &[("gain", 1), ("steps", 2), ("bypass", 3)])
                .expect("parameter mapping is valid");
        let encoded = state
            .encode_state(&state_identity(), StateLimits::default())
            .expect("state encodes");
        let mut document = StateDocument::decode(&encoded).expect("state decodes");
        document
            .insert(StateEntry::new(
                "custom/calibration",
                StateValue::Signed(123),
            ))
            .expect("custom entry is structurally valid");
        assert!(state.apply_plain_value(ClapId::new(1), 0.75));

        assert!(state.apply_state(&document, &state_identity()).is_err());
        assert!((state.value(0) - 0.75).abs() <= f64::EPSILON);
    }

    #[test]
    fn state_projection_checks_integer_domain_before_conversion() {
        const LIMIT: i64 = 9_007_199_254_740_992;
        let descriptor = ParameterDescriptor::integer("count", "Count", -LIMIT, LIMIT, 0)
            .expect("integer domain is valid");
        let state = ClapParameterState::new(&[descriptor], &[("count", 1)])
            .expect("integer domain is representable");
        for invalid in [LIMIT + 1, -LIMIT - 1] {
            let mut document =
                StateDocument::new("com.example.test", 1).expect("document is valid");
            document
                .insert(StateEntry::new(
                    "parameter/count",
                    StateValue::Signed(invalid),
                ))
                .expect("entry is structurally valid");
            assert!(state.apply_state(&document, &state_identity()).is_err());
            assert_eq!(state.value(0).to_bits(), 0.0_f64.to_bits());
        }
    }

    #[test]
    fn state_projection_round_trips_scalar_values() {
        let descriptors = descriptors();
        let state =
            ClapParameterState::new(&descriptors, &[("gain", 1), ("steps", 2), ("bypass", 3)])
                .expect("parameter mapping is valid");
        assert!(state.apply_plain_value(ClapId::new(1), 0.75));
        assert!(state.apply_plain_value(ClapId::new(3), 1.0));
        let encoded = state
            .encode_state(&state_identity(), StateLimits::default())
            .expect("state encodes");
        let document = StateDocument::decode(&encoded).expect("state decodes");

        assert_eq!(document.product_id(), "com.example.test");
        let restored =
            ClapParameterState::new(&descriptors, &[("gain", 1), ("steps", 2), ("bypass", 3)])
                .expect("parameter mapping is valid");
        restored
            .apply_state(&document, &state_identity())
            .expect("state applies");
        assert!((restored.value(0) - 0.75).abs() <= f64::EPSILON);
        assert!((restored.value(2) - 1.0).abs() <= f64::EPSILON);
    }

    fn choice_descriptors() -> Vec<ParameterDescriptor> {
        vec![
            ParameterDescriptor::float("gain", "Gain", 0.0, 1.0, 0.5)
                .expect("float parameter is valid"),
            ParameterDescriptor::choice(
                "mode",
                "Mode",
                vec![
                    ChoiceOption::new("clean", "Clean").expect("choice is valid"),
                    ChoiceOption::new("warm", "Warm").expect("choice is valid"),
                    ChoiceOption::new("driven", "Driven").expect("choice is valid"),
                ],
                "warm",
            )
            .expect("choice parameter is valid"),
        ]
    }

    #[test]
    fn choice_events_normalize_to_dense_option_indices() {
        let descriptors = choice_descriptors();
        let state = ClapParameterState::new(&descriptors, &[("gain", 1), ("mode", 2)])
            .expect("parameter mapping is valid");
        let raw = [ParamValueEvent::new(
            0,
            ClapId::new(2),
            Pckn::match_all(),
            0.0,
        )];
        let input = InputEvents::from_buffer(&raw);
        let mut normalized = Vec::with_capacity(1);
        normalized_events(&state, &input, &mut normalized).expect("events normalize");
        let events = ParameterEvents::new(&normalized, 4, 2).expect("events are valid");

        assert_eq!(events.len(), 1);
        assert_eq!(
            events.iter().next().expect("first event").change(),
            ParameterEventChange::Set(ParameterEventValue::Choice(ChoiceIndex::new(0)))
        );

        // Out-of-range option indices are dropped before core validation.
        let bad = [ParamValueEvent::new(
            0,
            ClapId::new(2),
            Pckn::match_all(),
            7.0,
        )];
        let bad_input = InputEvents::from_buffer(&bad);
        let mut bad_normalized = Vec::with_capacity(1);
        normalized_events(&state, &bad_input, &mut bad_normalized).expect("events normalize");
        assert_eq!(bad_normalized, []);
    }

    #[test]
    fn choice_control_and_completed_runtime_values_project_without_changing_identity() {
        let descriptors = choice_descriptors();
        let state = ClapParameterState::new(&descriptors, &[("gain", 1), ("mode", 2)])
            .expect("parameter mapping is valid");
        assert!(state.apply_plain_value(ClapId::new(2), 2.0));
        let mut store = ParameterStore::new(&descriptors).expect("schema is valid");
        let mut scratch = vec![0.0; 2];
        let baseline = state
            .sync_into(store.values_mut(), &mut scratch, None)
            .expect("control choice syncs");
        assert_eq!(
            store.get("mode"),
            Some(&ParameterValue::Choice(
                ChoiceId::new("driven").expect("choice id is valid")
            ))
        );

        store
            .set(
                "mode",
                ParameterValue::Choice(ChoiceId::new("warm").unwrap()),
            )
            .unwrap();
        state
            .publish_parameters(&store, baseline, &mut scratch)
            .expect("completed choice publication succeeds");
        store.reset();
        state
            .sync_into(store.values_mut(), &mut scratch, None)
            .expect("completed choice syncs into a fresh store");
        assert_eq!(
            store.get("mode"),
            Some(&ParameterValue::Choice(
                ChoiceId::new("warm").expect("choice id is valid")
            ))
        );
        assert!((state.value(1) - 1.0).abs() <= f64::EPSILON);
    }

    #[test]
    fn choice_state_round_trips_through_stable_identities() {
        let descriptors = choice_descriptors();
        let state = ClapParameterState::new(&descriptors, &[("gain", 1), ("mode", 2)])
            .expect("parameter mapping is valid");
        assert!(state.apply_plain_value(ClapId::new(2), 2.0));
        let encoded = state
            .encode_state(&state_identity(), StateLimits::default())
            .expect("state encodes");
        let document = StateDocument::decode(&encoded).expect("state decodes");

        assert_eq!(
            document
                .entries()
                .iter()
                .find(|entry| entry.key() == "parameter/mode")
                .map(StateEntry::value),
            Some(&StateValue::Choice("driven".to_owned()))
        );

        let restored = ClapParameterState::new(&descriptors, &[("gain", 1), ("mode", 2)])
            .expect("parameter mapping is valid");
        restored
            .apply_state(&document, &state_identity())
            .expect("state applies");
        assert!((restored.value(1) - 2.0).abs() <= f64::EPSILON);

        // Unknown choice identities are rejected transactionally.
        let mut corrupt = StateDocument::new("com.example.test", 1).expect("document is valid");
        corrupt
            .insert(StateEntry::new("parameter/gain", StateValue::Float(0.5)))
            .expect("entry is structurally valid");
        corrupt
            .insert(StateEntry::new(
                "parameter/mode",
                StateValue::Choice("mystery".to_owned()),
            ))
            .expect("entry is structurally valid");
        assert!(matches!(
            restored.apply_state(&corrupt, &state_identity()),
            Err(ParameterStateError::InvalidValue)
        ));
        assert!((restored.value(1) - 2.0).abs() <= f64::EPSILON);
    }
}
