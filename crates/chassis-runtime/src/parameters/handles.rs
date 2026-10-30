//! Setup-resolved typed parameter access. Handles belong to the schema that resolved them.

use super::{
    ChoiceIndex, ParameterEventValueError, ParameterIndex, ParameterKind, ParameterStore,
    ParameterStoreError, ParameterType, ParameterValueError,
};
use crate::{
    automation::{
        BooleanParameterCursor, ChoiceParameterCursor, FloatParameterCursor,
        IntegerParameterCursor, ParameterCursorError, ParameterEventValue,
    },
    modulation::{GlobalFloatModulation, ModulatedFloatCursor},
    process::{ProcessBlock, ProcessBufferSource},
};

/// A schema-local floating-point parameter resolved during setup.
///
/// Like dense indices, handles must only be used with the schema that resolved
/// them. Access detects unknown indices and wrong types, not another schema's
/// coincidentally matching index and type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FloatParameter(ParameterIndex);

/// A schema-local integer parameter resolved during setup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntegerParameter(ParameterIndex);

/// A schema-local Boolean parameter resolved during setup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BooleanParameter(ParameterIndex);

/// A schema-local choice parameter resolved during setup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChoiceParameter(ParameterIndex);

impl ParameterStore {
    /// Resolve a floating-point parameter and check its type off the audio thread.
    ///
    /// # Errors
    /// Returns an unknown-key or wrong-type error.
    pub fn float_parameter(
        &self,
        key: impl AsRef<str>,
    ) -> Result<FloatParameter, ParameterStoreError> {
        self.typed_index(key.as_ref(), ParameterType::Float)
            .map(FloatParameter)
    }

    /// Resolve an integer parameter and check its type off the audio thread.
    ///
    /// # Errors
    /// Returns an unknown-key or wrong-type error.
    pub fn integer_parameter(
        &self,
        key: impl AsRef<str>,
    ) -> Result<IntegerParameter, ParameterStoreError> {
        self.typed_index(key.as_ref(), ParameterType::Integer)
            .map(IntegerParameter)
    }

    /// Resolve a Boolean parameter and check its type off the audio thread.
    ///
    /// # Errors
    /// Returns an unknown-key or wrong-type error.
    pub fn boolean_parameter(
        &self,
        key: impl AsRef<str>,
    ) -> Result<BooleanParameter, ParameterStoreError> {
        self.typed_index(key.as_ref(), ParameterType::Boolean)
            .map(BooleanParameter)
    }

    /// Resolve a choice parameter and check its type off the audio thread.
    ///
    /// # Errors
    /// Returns an unknown-key or wrong-type error.
    pub fn choice_parameter(
        &self,
        key: impl AsRef<str>,
    ) -> Result<ChoiceParameter, ParameterStoreError> {
        self.typed_index(key.as_ref(), ParameterType::Choice)
            .map(ChoiceParameter)
    }

    fn typed_index(
        &self,
        key: &str,
        expected: ParameterType,
    ) -> Result<ParameterIndex, ParameterStoreError> {
        let index = self
            .index(key)
            .ok_or_else(|| ParameterStoreError::UnknownParameter(key.to_owned()))?;
        let actual = self.descriptors
            [usize::try_from(index.get()).expect("resolved index fits usize")]
        .kind()
        .value_type();
        if actual != expected {
            return Err(ParameterStoreError::WrongParameterType {
                parameter: key.to_owned(),
                expected,
                actual,
            });
        }
        Ok(index)
    }
}

impl FloatParameter {
    /// Return the schema-local index for low-level event routing.
    #[must_use]
    pub const fn index(self) -> ParameterIndex {
        self.0
    }

    /// Read the current base value without resolving a stable key.
    ///
    /// # Errors
    /// Rejects an unknown index or a handle used with a differently typed schema.
    pub fn value(self, parameters: &ParameterStore) -> Result<f64, ParameterEventValueError> {
        match read(parameters, self.0, ParameterType::Float)? {
            ParameterEventValue::Float(value) => Ok(value),
            _ => unreachable!("read checked the float type"),
        }
    }

    /// Follow base automation without adding modulation or smoothing host ramps.
    ///
    /// # Errors
    /// Rejects a handle that does not identify a float in this block's schema.
    pub fn base_cursor<'events, S, B>(
        self,
        block: &ProcessBlock<'_, 'events, '_, S, B>,
    ) -> Result<FloatParameterCursor<'events>, ParameterCursorError>
    where
        B: ProcessBufferSource<S> + ?Sized,
    {
        block
            .parameter_events()
            .float_cursor(self.0, self.value(block.parameters())?)
    }

    /// Follow the effective trajectory from this block's consumed controls.
    ///
    /// For opted-in parameters this adds held/timed global modulation and clamps
    /// to the declared domain. Other floats follow only their base trajectory.
    /// No desired control state is sampled independently, so resource-transition
    /// freezes apply to both the initial values and the borrowed events.
    ///
    /// # Errors
    /// Rejects a handle that does not identify a float in this block's schema.
    pub fn cursor<'events, S, B>(
        self,
        block: &ProcessBlock<'_, 'events, '_, S, B>,
    ) -> Result<ModulatedFloatCursor<'events>, ParameterCursorError>
    where
        B: ProcessBufferSource<S> + ?Sized,
    {
        let base = self.value(block.parameters())?;
        let descriptor = block
            .parameters()
            .descriptor(self.0)
            .ok_or(ParameterEventValueError::UnknownParameter(self.0))?;
        let ParameterKind::Float {
            minimum, maximum, ..
        } = *descriptor.kind()
        else {
            unreachable!("value checked the float type");
        };
        let domain = GlobalFloatModulation::new(minimum, maximum);
        if descriptor.global_modulation().is_some() {
            let amount = block
                .modulation(self.0)
                .ok_or(ParameterEventValueError::UnknownParameter(self.0))?;
            domain.cursor(block.parameter_events(), self.0, base, amount)
        } else {
            domain.base_cursor(block.parameter_events(), self.0, base)
        }
    }
}

impl IntegerParameter {
    /// Return the schema-local index for low-level event routing.
    #[must_use]
    pub const fn index(self) -> ParameterIndex {
        self.0
    }

    /// Read the current base value without resolving a stable key.
    ///
    /// # Errors
    /// Rejects an unknown index or a handle used with a differently typed schema.
    pub fn value(self, parameters: &ParameterStore) -> Result<i64, ParameterEventValueError> {
        match read(parameters, self.0, ParameterType::Integer)? {
            ParameterEventValue::Integer(value) => Ok(value),
            _ => unreachable!("read checked the integer type"),
        }
    }

    /// Follow this block's sample-accurate integer steps without allocation.
    ///
    /// # Errors
    /// Rejects a handle that does not identify an integer in this block's schema.
    pub fn cursor<'events, S, B>(
        self,
        block: &ProcessBlock<'_, 'events, '_, S, B>,
    ) -> Result<IntegerParameterCursor<'events>, ParameterCursorError>
    where
        B: ProcessBufferSource<S> + ?Sized,
    {
        Ok(block
            .parameter_events()
            .integer_cursor(self.0, self.value(block.parameters())?))
    }
}

impl BooleanParameter {
    /// Return the schema-local index for low-level event routing.
    #[must_use]
    pub const fn index(self) -> ParameterIndex {
        self.0
    }

    /// Read the current base value without resolving a stable key.
    ///
    /// # Errors
    /// Rejects an unknown index or a handle used with a differently typed schema.
    pub fn value(self, parameters: &ParameterStore) -> Result<bool, ParameterEventValueError> {
        match read(parameters, self.0, ParameterType::Boolean)? {
            ParameterEventValue::Boolean(value) => Ok(value),
            _ => unreachable!("read checked the Boolean type"),
        }
    }

    /// Follow this block's sample-accurate Boolean steps without allocation.
    ///
    /// # Errors
    /// Rejects a handle that does not identify a Boolean in this block's schema.
    pub fn cursor<'events, S, B>(
        self,
        block: &ProcessBlock<'_, 'events, '_, S, B>,
    ) -> Result<BooleanParameterCursor<'events>, ParameterCursorError>
    where
        B: ProcessBufferSource<S> + ?Sized,
    {
        Ok(block
            .parameter_events()
            .boolean_cursor(self.0, self.value(block.parameters())?))
    }
}

impl ChoiceParameter {
    /// Return the schema-local index for low-level event routing.
    #[must_use]
    pub const fn index(self) -> ParameterIndex {
        self.0
    }

    /// Read the current parameter-local choice index without string lookup.
    ///
    /// # Errors
    /// Rejects an unknown index or a handle used with a differently typed schema.
    pub fn value(
        self,
        parameters: &ParameterStore,
    ) -> Result<ChoiceIndex, ParameterEventValueError> {
        match read(parameters, self.0, ParameterType::Choice)? {
            ParameterEventValue::Choice(value) => Ok(value),
            _ => unreachable!("read checked the choice type"),
        }
    }

    /// Follow this block's sample-accurate choice steps without allocation.
    ///
    /// # Errors
    /// Rejects a handle that does not identify a choice in this block's schema.
    pub fn cursor<'events, S, B>(
        self,
        block: &ProcessBlock<'_, 'events, '_, S, B>,
    ) -> Result<ChoiceParameterCursor<'events>, ParameterCursorError>
    where
        B: ProcessBufferSource<S> + ?Sized,
    {
        Ok(block
            .parameter_events()
            .choice_cursor(self.0, self.value(block.parameters())?))
    }
}

fn read(
    parameters: &ParameterStore,
    index: ParameterIndex,
    expected: ParameterType,
) -> Result<ParameterEventValue, ParameterEventValueError> {
    let value = parameters
        .get_event_value(index)
        .ok_or(ParameterEventValueError::UnknownParameter(index))?;
    let actual = value.value_type();
    if actual != expected {
        return Err(ParameterEventValueError::InvalidValue(
            ParameterValueError::WrongType { expected, actual },
        ));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parameters::{ChoiceOption, ParameterDescriptor};

    #[test]
    fn handles_resolve_types_and_read_all_four_dense_values() {
        let store = ParameterStore::new(&[
            ParameterDescriptor::float("gain", "Gain", 0.0, 2.0, 0.5).unwrap(),
            ParameterDescriptor::integer("voices", "Voices", 1, 16, 4).unwrap(),
            ParameterDescriptor::boolean("enabled", "Enabled", true).unwrap(),
            ParameterDescriptor::choice(
                "mode",
                "Mode",
                vec![
                    ChoiceOption::new("a", "A").unwrap(),
                    ChoiceOption::new("b", "B").unwrap(),
                ],
                "b",
            )
            .unwrap(),
        ])
        .unwrap();
        assert_eq!(
            store.float_parameter("gain").unwrap().value(&store),
            Ok(0.5)
        );
        assert_eq!(
            store.integer_parameter("voices").unwrap().value(&store),
            Ok(4)
        );
        assert_eq!(
            store.boolean_parameter("enabled").unwrap().value(&store),
            Ok(true)
        );
        assert_eq!(
            store.choice_parameter("mode").unwrap().value(&store),
            Ok(ChoiceIndex::new(1))
        );
        assert!(matches!(
            store.float_parameter("missing"),
            Err(ParameterStoreError::UnknownParameter(_))
        ));
        assert!(matches!(
            store.float_parameter("voices"),
            Err(ParameterStoreError::WrongParameterType {
                expected: ParameterType::Float,
                actual: ParameterType::Integer,
                ..
            })
        ));
        let other =
            ParameterStore::new(&[
                ParameterDescriptor::boolean("enabled", "Enabled", false).unwrap()
            ])
            .unwrap();
        assert!(matches!(
            store.float_parameter("gain").unwrap().value(&other),
            Err(ParameterEventValueError::InvalidValue(
                ParameterValueError::WrongType { .. }
            ))
        ));
        assert_eq!(
            store.choice_parameter("mode").unwrap().value(&other),
            Err(ParameterEventValueError::UnknownParameter(
                ParameterIndex::new(3)
            ))
        );
    }
}
