use std::{
    fmt,
    hint::spin_loop,
    string::String,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    vec::Vec,
};

use chassis_core::{
    automation::{ParameterEvent, ParameterEventChange, ParameterEventValue},
    parameters::{
        ParameterDescriptor, ParameterIndex, ParameterKind, ParameterStore, ParameterType,
        ParameterValue,
    },
    state::{
        StateDocument, StateDocumentError, StateEncodeError, StateEntry, StateLimits, StateValue,
    },
};
use clack_plugin::{
    events::{Event, event_types::ParamValueEvent, io::InputEvents},
    utils::ClapId,
};

const EXACT_INTEGER_LIMIT: f64 = 9_007_199_254_740_992.0;
const SNAPSHOT_RETRIES: u32 = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ParameterMappingError {
    MissingId(String),
    ExtraId(String),
    InvalidId(String),
    DuplicateId(u32),
    ParameterIndexUnrepresentable(usize),
    UnsupportedType {
        parameter: String,
        parameter_type: ParameterType,
    },
    IntegerRangeNotRepresentable(String),
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
            Self::UnsupportedType {
                parameter,
                parameter_type,
            } => write!(
                formatter,
                "parameter {parameter} has unsupported CLAP type {parameter_type}"
            ),
            Self::IntegerRangeNotRepresentable(parameter) => write!(
                formatter,
                "integer parameter {parameter} exceeds the exactly representable CLAP range"
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
            ParameterKind::Choice { .. } => 0.0,
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
            ParameterKind::Choice { .. } => (0.0, 0.0),
        }
    }

    pub(crate) fn flags(&self) -> clack_extensions::params::ParamInfoFlags {
        use clack_extensions::params::ParamInfoFlags;

        let mut flags = ParamInfoFlags::IS_AUTOMATABLE | ParamInfoFlags::REQUIRES_PROCESS;
        if matches!(
            self.descriptor.kind(),
            ParameterKind::Integer { .. } | ParameterKind::Boolean { .. }
        ) {
            flags |= ParamInfoFlags::IS_STEPPED;
        }
        flags
    }

    pub(crate) fn event_value(&self, value: f64) -> ParameterEventValue<'static> {
        match self.descriptor.kind() {
            ParameterKind::Float { .. } | ParameterKind::Choice { .. } => {
                ParameterEventValue::Float(value)
            }
            ParameterKind::Integer { .. } => integer_from_plain(value).map_or(
                ParameterEventValue::Float(value),
                ParameterEventValue::Integer,
            ),
            ParameterKind::Boolean { .. } => match value {
                0.0 => ParameterEventValue::Boolean(false),
                1.0 => ParameterEventValue::Boolean(true),
                _ => ParameterEventValue::Float(value),
            },
        }
    }

    pub(crate) fn parameter_value(&self, value: f64) -> Option<ParameterValue> {
        match self.descriptor.kind() {
            ParameterKind::Float {
                minimum, maximum, ..
            } if value.is_finite() && (*minimum..=*maximum).contains(&value) => {
                Some(ParameterValue::Float(value))
            }
            ParameterKind::Float { .. } | ParameterKind::Choice { .. } => None,
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
            ParameterKind::Choice { .. } => None,
        }
    }

    fn event_plain_value(&self, value: ParameterEventValue<'_>) -> Option<f64> {
        let plain = match value {
            ParameterEventValue::Float(value) => value,
            ParameterEventValue::Integer(value) => integer_to_plain(value),
            ParameterEventValue::Boolean(value) => f64::from(u8::from(value)),
            ParameterEventValue::Choice(_) => return None,
        };
        let value = self.parameter_value(plain)?;
        Some(match value {
            ParameterValue::Float(value) => value,
            ParameterValue::Integer(value) => integer_to_plain(value),
            ParameterValue::Boolean(value) => f64::from(u8::from(value)),
            ParameterValue::Choice(_) => return None,
        })
    }

    fn state_plain_value(&self, value: &StateValue) -> Option<f64> {
        match (self.descriptor.kind(), value) {
            (ParameterKind::Float { .. }, StateValue::Float(value)) if value.is_finite() => {
                Some(*value)
            }
            (ParameterKind::Integer { .. }, StateValue::Signed(value)) => {
                Some(integer_to_plain(*value))
            }
            (ParameterKind::Boolean { .. }, StateValue::Boolean(value)) => {
                Some(f64::from(u8::from(*value)))
            }
            _ => None,
        }
        .filter(|value| self.parameter_value(*value).is_some())
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

pub(crate) struct ClapParameterState {
    bindings: Vec<ClapParameterBinding>,
    id_lookup: Vec<usize>,
    key_lookup: Vec<usize>,
    values: Vec<AtomicU64>,
    generation: AtomicU64,
    pending: AtomicBool,
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
                } if integer_to_plain(*minimum) >= -EXACT_INTEGER_LIMIT
                    && integer_to_plain(*maximum) <= EXACT_INTEGER_LIMIT => {}
                ParameterKind::Integer { .. } => {
                    return Err(ParameterMappingError::IntegerRangeNotRepresentable(
                        descriptor.key().as_str().to_owned(),
                    ));
                }
                ParameterKind::Choice { .. } => {
                    return Err(ParameterMappingError::UnsupportedType {
                        parameter: descriptor.key().as_str().to_owned(),
                        parameter_type: ParameterType::Choice,
                    });
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
        let values = bindings
            .iter()
            .map(|binding| AtomicU64::new(binding.default_plain().to_bits()))
            .collect();

        Ok(Self {
            bindings,
            id_lookup,
            key_lookup,
            values,
            generation: AtomicU64::new(0),
            pending: AtomicBool::new(false),
        })
    }

    pub(crate) fn bindings(&self) -> &[ClapParameterBinding] {
        &self.bindings
    }

    pub(crate) fn matches_descriptors(&self, descriptors: &[ParameterDescriptor]) -> bool {
        descriptors.len() == self.bindings.len()
            && descriptors
                .iter()
                .zip(&self.bindings)
                .all(|(descriptor, binding)| descriptor == binding.descriptor())
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
        f64::from_bits(self.values[index].load(Ordering::Acquire))
    }

    fn try_begin_write(&self, expected: u64) -> bool {
        expected.is_multiple_of(2)
            && self
                .generation
                .compare_exchange(
                    expected,
                    expected.wrapping_add(1),
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
    }

    fn finish_write(&self, expected: u64) {
        self.generation
            .store(expected.wrapping_add(2), Ordering::Release);
        self.pending.store(true, Ordering::Release);
    }

    #[cfg(test)]
    fn publish_value_control(&self, index: usize, value: f64) {
        loop {
            let expected = self.generation.load(Ordering::Acquire);
            if !self.try_begin_write(expected) {
                spin_loop();
                continue;
            }
            self.values[index].store(value.to_bits(), Ordering::Release);
            self.finish_write(expected);
            return;
        }
    }

    fn try_publish_value(&self, index: usize, value: f64) -> bool {
        let expected = self.generation.load(Ordering::Acquire);
        if !self.try_begin_write(expected) {
            return false;
        }
        self.values[index].store(value.to_bits(), Ordering::Release);
        self.finish_write(expected);
        true
    }

    fn publish_values_control(&self, values: &[f64]) {
        loop {
            let expected = self.generation.load(Ordering::Acquire);
            if !self.try_begin_write(expected) {
                spin_loop();
                continue;
            }
            for (slot, value) in self.values.iter().zip(values) {
                slot.store(value.to_bits(), Ordering::Release);
            }
            self.finish_write(expected);
            return;
        }
    }

    fn try_publish_values_from(&self, expected: u64, values: &[f64]) -> bool {
        if values.len() != self.values.len() || !self.try_begin_write(expected) {
            return false;
        }
        for (slot, value) in self.values.iter().zip(values) {
            slot.store(value.to_bits(), Ordering::Release);
        }
        self.finish_write(expected);
        true
    }

    pub(crate) fn apply_input(&self, input: &InputEvents<'_>) {
        for event in input {
            let Some(event) = event.as_event::<ParamValueEvent>() else {
                continue;
            };
            let Some(id) = event.param_id() else {
                continue;
            };
            let _ = self.try_apply_plain_value(id, event.value());
        }
    }

    pub(crate) fn request_sync(&self) {
        self.pending.store(true, Ordering::Release);
    }

    #[cfg(test)]
    pub(crate) fn apply_plain_value(&self, id: ClapId, value: f64) -> bool {
        let Some((index, value)) = self.validated_plain_value(id, value) else {
            return false;
        };
        self.publish_value_control(index, value);
        true
    }

    fn try_apply_plain_value(&self, id: ClapId, value: f64) -> bool {
        let Some((index, value)) = self.validated_plain_value(id, value) else {
            return false;
        };
        self.try_publish_value(index, value)
    }

    fn validated_plain_value(&self, id: ClapId, value: f64) -> Option<(usize, f64)> {
        let index = self.index_for_id(id)?;
        let value = self.bindings[index].parameter_value(value)?;
        let value = match value {
            ParameterValue::Float(value) => value,
            ParameterValue::Integer(value) => integer_to_plain(value),
            ParameterValue::Boolean(value) => f64::from(u8::from(value)),
            ParameterValue::Choice(_) => return None,
        };
        Some((index, value))
    }

    pub(crate) fn sync_into(
        &self,
        store: &mut ParameterStore,
        scratch: &mut [f64],
    ) -> Result<(), ParameterSyncError> {
        if scratch.len() != self.values.len() {
            return Err(ParameterSyncError::InvalidPublishedValue);
        }
        if !self.pending.swap(false, Ordering::AcqRel) {
            return Ok(());
        }
        if self.try_snapshot_into(scratch).is_none() {
            self.pending.store(true, Ordering::Release);
            return Ok(());
        }
        if self
            .bindings
            .iter()
            .zip(scratch.iter().copied())
            .any(|(binding, value)| binding.parameter_value(value).is_none())
        {
            self.pending.store(true, Ordering::Release);
            return Err(ParameterSyncError::InvalidPublishedValue);
        }
        for (binding, value) in self.bindings.iter().zip(scratch.iter().copied()) {
            let Some(parameter) = binding.parameter_value(value) else {
                self.pending.store(true, Ordering::Release);
                return Err(ParameterSyncError::InvalidPublishedValue);
            };
            store
                .set_index(binding.index(), parameter)
                .map_err(|_| ParameterSyncError::StoreRejected)?;
        }
        Ok(())
    }

    pub(crate) fn publish_events(
        &self,
        events: &[ParameterEvent<'_>],
        scratch: &mut [f64],
    ) -> Result<(), ParameterSyncError> {
        if events.is_empty() {
            return Ok(());
        }
        if scratch.len() != self.values.len() {
            return Err(ParameterSyncError::InvalidPublishedValue);
        }
        let Some(generation) = self.try_snapshot_into(scratch) else {
            return Ok(());
        };
        for event in events {
            let index = usize::try_from(event.parameter().get())
                .map_err(|_| ParameterSyncError::InvalidPublishedValue)?;
            let Some(binding) = self.bindings.get(index) else {
                return Err(ParameterSyncError::InvalidPublishedValue);
            };
            let ParameterEventChange::Set(value) = event.change() else {
                return Err(ParameterSyncError::InvalidPublishedValue);
            };
            let Some(value) = binding.event_plain_value(value) else {
                return Err(ParameterSyncError::InvalidPublishedValue);
            };
            scratch[index] = value;
        }
        let _ = self.try_publish_values_from(generation, scratch);
        Ok(())
    }

    fn try_snapshot_into(&self, output: &mut [f64]) -> Option<u64> {
        if output.len() != self.values.len() {
            return None;
        }
        for _ in 0..SNAPSHOT_RETRIES {
            let start = self.generation.load(Ordering::Acquire);
            if !start.is_multiple_of(2) {
                spin_loop();
                continue;
            }
            for (slot, value) in self.values.iter().zip(output.iter_mut()) {
                *value = f64::from_bits(slot.load(Ordering::Acquire));
            }
            let end = self.generation.load(Ordering::Acquire);
            if start == end && end.is_multiple_of(2) {
                return Some(end);
            }
        }
        None
    }

    fn snapshot_control_into(&self, output: &mut [f64]) -> u64 {
        loop {
            if let Some(generation) = self.try_snapshot_into(output) {
                return generation;
            }
            spin_loop();
        }
    }

    pub(crate) fn encode_state(
        &self,
        product_id: &str,
        product_schema: u32,
        limits: StateLimits,
    ) -> Result<Vec<u8>, ParameterStateError> {
        let mut values = vec![0.0; self.values.len()];
        self.snapshot_control_into(&mut values);
        let mut document = StateDocument::new(product_id, product_schema)
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
        product_id: &str,
        product_schema: u32,
    ) -> Result<(), ParameterStateError> {
        if document.product_id() != product_id {
            return Err(ParameterStateError::ProductIdentityMismatch);
        }
        if document.product_schema() != product_schema {
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
                continue;
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
        self.publish_values_control(&candidate);
        Ok(())
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
    ProductIdentityMismatch,
    ProductSchemaMismatch,
    UnknownParameter(String),
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
            Self::IncompleteParameterState { expected, actual } => write!(
                formatter,
                "parameter state contains {actual} parameters but {expected} are required"
            ),
            Self::InvalidValue => formatter.write_str("parameter state contains an invalid value"),
        }
    }
}

impl std::error::Error for ParameterStateError {}

pub(crate) fn normalized_event(
    state: &ClapParameterState,
    id: ClapId,
    time: u32,
    value: f64,
) -> Option<ParameterEvent<'static>> {
    let index = state.index_for_id(id)?;
    let binding = &state.bindings[index];
    Some(ParameterEvent::set(
        time,
        binding.index(),
        binding.event_value(value),
    ))
}

pub(crate) fn normalized_events(
    state: &ClapParameterState,
    input: &InputEvents<'_>,
    output: &mut Vec<ParameterEvent<'static>>,
) -> Result<(), &'static str> {
    output.clear();
    for event in input {
        let Some(event) = event.as_event::<ParamValueEvent>() else {
            continue;
        };
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
    use chassis_core::{
        automation::{ParameterEventChange, ParameterEvents},
        parameters::{ChoiceOption, ParameterDescriptor},
    };
    use clack_plugin::events::Pckn;

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
    fn mapping_requires_stable_ids_and_rejects_unsupported_choices() {
        let choice = ParameterDescriptor::choice(
            "mode",
            "Mode",
            vec![ChoiceOption::new("clean", "Clean").expect("choice is valid")],
            "clean",
        )
        .expect("choice parameter is valid");
        assert!(matches!(
            ClapParameterState::new(&[choice], &[("mode", 1)]),
            Err(ParameterMappingError::UnsupportedType { .. })
        ));

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
            .sync_into(&mut store, &mut scratch)
            .expect("projection syncs");
        assert_eq!(store.get("gain"), Some(&ParameterValue::Float(0.75)));
        assert_eq!(store.get("steps"), Some(&ParameterValue::Integer(-2)));

        store.reset();
        state.request_sync();
        state
            .sync_into(&mut store, &mut scratch)
            .expect("explicit reactivation sync succeeds");
        assert_eq!(store.get("gain"), Some(&ParameterValue::Float(0.75)));
        assert_eq!(store.get("steps"), Some(&ParameterValue::Integer(-2)));
    }

    #[test]
    fn automation_publishes_final_scalar_values_for_the_next_block() {
        let descriptors = descriptors();
        let state =
            ClapParameterState::new(&descriptors, &[("gain", 1), ("steps", 2), ("bypass", 3)])
                .expect("parameter mapping is valid");
        let events = [
            ParameterEvent::set(0, ParameterIndex::new(0), ParameterEventValue::Float(0.25)),
            ParameterEvent::set(2, ParameterIndex::new(0), ParameterEventValue::Float(0.75)),
            ParameterEvent::set(3, ParameterIndex::new(1), ParameterEventValue::Integer(-2)),
        ];
        let mut scratch = vec![0.0; 3];
        state
            .publish_events(&events, &mut scratch)
            .expect("automation publication succeeds");
        assert!((state.value(0) - 0.75).abs() <= f64::EPSILON);
        assert!((state.value(1) + 2.0).abs() <= f64::EPSILON);
    }

    #[test]
    fn stale_realtime_snapshot_cannot_overwrite_newer_control_state() {
        let descriptors = descriptors();
        let state =
            ClapParameterState::new(&descriptors, &[("gain", 1), ("steps", 2), ("bypass", 3)])
                .expect("parameter mapping is valid");
        let mut scratch = vec![0.0; 3];
        let generation = state
            .try_snapshot_into(&mut scratch)
            .expect("initial snapshot is coherent");

        assert!(state.apply_plain_value(ClapId::new(2), 4.0));
        scratch[0] = 0.25;
        assert!(!state.try_publish_values_from(generation, &scratch));
        assert!((state.value(0) - 0.5).abs() <= f64::EPSILON);
        assert!((state.value(1) - 4.0).abs() <= f64::EPSILON);
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
            state.apply_state(&document, "com.example.test", 1),
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
            state.apply_state(&document, "com.example.test", 1),
            Err(ParameterStateError::IncompleteParameterState {
                expected: 3,
                actual: 1
            })
        ));
        assert!((state.value(0) - 0.5).abs() <= f64::EPSILON);
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
            .encode_state("com.example.test", 1, StateLimits::default())
            .expect("state encodes");
        let document = StateDocument::decode(&encoded).expect("state decodes");

        assert_eq!(document.product_id(), "com.example.test");
        let restored =
            ClapParameterState::new(&descriptors, &[("gain", 1), ("steps", 2), ("bypass", 3)])
                .expect("parameter mapping is valid");
        restored
            .apply_state(&document, "com.example.test", 1)
            .expect("state applies");
        assert!((restored.value(0) - 0.75).abs() <= f64::EPSILON);
        assert!((restored.value(2) - 1.0).abs() <= f64::EPSILON);
    }
}
