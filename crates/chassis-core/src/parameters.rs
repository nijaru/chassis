//! Typed parameter schemas and non-realtime base-value storage.
//!
//! [`ParameterStore`] is the semantic authority for a component's current
//! base/control values. It validates every replacement and applies decoded
//! state transactionally. Host event queues remain adapter-owned; normalized
//! borrowed process events are validated here against the active schema.

use core::fmt;
use std::{string::String, vec::Vec};

use crate::{
    automation::{ParameterEventChange, ParameterEventValue, ParameterEvents},
    state::{
        StateDocument, StateDocumentError, StateEncodeError, StateEntry, StateLimits, StateValue,
    },
};

/// Stable identity of a parameter.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ParameterKey(String);

impl ParameterKey {
    /// Construct a canonical parameter key.
    ///
    /// Keys are non-empty and may not contain whitespace or control characters.
    /// Namespace separators such as `.` and `/` remain product conventions.
    ///
    /// # Errors
    ///
    /// Returns [`ParameterKeyError`] when the text is empty or contains
    /// whitespace or a control character.
    pub fn new(value: impl Into<String>) -> Result<Self, ParameterKeyError> {
        let value = value.into();
        validate_identifier(&value).map_err(|reason| ParameterKeyError { reason })?;
        Ok(Self(value))
    }

    /// Return the stable key text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume the key and return its text.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl AsRef<str> for ParameterKey {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for ParameterKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Dense schema-local identity used only by runtime/process paths.
///
/// A `ParameterIndex` is meaningful only relative to one validated immutable
/// parameter schema. It is never persistent identity and must not be serialized
/// into presets/projects or derived from backend numeric IDs without schema
/// resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ParameterIndex(u32);

impl ParameterIndex {
    /// Construct a raw schema-local index.
    ///
    /// The active [`ParameterStore`] validates that an index is in range before
    /// product DSP receives its event stream.
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    /// Return the raw schema-local index.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Stable identity of one choice option.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChoiceId(String);

impl ChoiceId {
    /// Construct a non-empty choice identity.
    ///
    /// # Errors
    ///
    /// Returns [`ChoiceIdError`] when the text is empty or contains whitespace
    /// or a control character.
    pub fn new(value: impl Into<String>) -> Result<Self, ChoiceIdError> {
        let value = value.into();
        validate_identifier(&value).map_err(|reason| ChoiceIdError { reason })?;
        Ok(Self(value))
    }

    /// Return the stable choice identity text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume the choice identity and return its text.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl AsRef<str> for ChoiceId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for ChoiceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

fn validate_identifier(value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::Empty);
    }
    if let Some(character) = value.chars().find(|character| character.is_whitespace()) {
        return Err(IdentifierError::Whitespace(character));
    }
    if let Some(character) = value.chars().find(|character| character.is_control()) {
        return Err(IdentifierError::Control(character));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum IdentifierError {
    Empty,
    Whitespace(char),
    Control(char),
}

impl fmt::Display for IdentifierError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("identifier is empty"),
            Self::Whitespace(character) => {
                write!(formatter, "identifier contains whitespace {character:?}")
            }
            Self::Control(character) => {
                write!(
                    formatter,
                    "identifier contains control character {character:?}"
                )
            }
        }
    }
}

/// Failure to construct a stable parameter key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParameterKeyError {
    reason: IdentifierError,
}

impl fmt::Display for ParameterKeyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid parameter key: {}", self.reason)
    }
}

impl std::error::Error for ParameterKeyError {}

/// Failure to construct a stable choice identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceIdError {
    reason: IdentifierError,
}

impl fmt::Display for ChoiceIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid choice identity: {}", self.reason)
    }
}

impl std::error::Error for ChoiceIdError {}

/// Metadata for one choice option.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceOption {
    id: ChoiceId,
    name: String,
}

impl ChoiceOption {
    /// Construct one stable choice option and its display label.
    ///
    /// # Errors
    ///
    /// Returns [`ChoiceIdError`] when `id` is empty or contains whitespace or
    /// control characters.
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Result<Self, ChoiceIdError> {
        let name = name.into();
        let id = ChoiceId::new(id)?;
        Ok(Self { id, name })
    }

    /// Return the stable choice identity.
    #[must_use]
    pub fn id(&self) -> &ChoiceId {
        &self.id
    }

    /// Return the display label.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Semantic parameter value type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterType {
    /// Finite plain-unit floating-point value.
    Float,
    /// Bounded signed integer value.
    Integer,
    /// Boolean value.
    Boolean,
    /// Stable choice identity.
    Choice,
}

impl fmt::Display for ParameterType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Float => "float",
            Self::Integer => "integer",
            Self::Boolean => "boolean",
            Self::Choice => "choice",
        })
    }
}

/// One typed parameter value.
#[derive(Debug, Clone, PartialEq)]
pub enum ParameterValue {
    /// Finite plain-unit floating-point value.
    Float(f64),
    /// Signed integer value.
    Integer(i64),
    /// Boolean value.
    Boolean(bool),
    /// Stable choice identity.
    Choice(ChoiceId),
}

impl ParameterValue {
    /// Return the semantic type of this value.
    #[must_use]
    pub const fn value_type(&self) -> ParameterType {
        match self {
            Self::Float(_) => ParameterType::Float,
            Self::Integer(_) => ParameterType::Integer,
            Self::Boolean(_) => ParameterType::Boolean,
            Self::Choice(_) => ParameterType::Choice,
        }
    }
}

/// Parameter domain and default metadata.
#[derive(Debug, Clone, PartialEq)]
pub enum ParameterKind {
    /// Inclusive finite floating-point domain.
    Float {
        /// Inclusive minimum.
        minimum: f64,
        /// Inclusive maximum.
        maximum: f64,
        /// Default value.
        default: f64,
    },
    /// Inclusive signed integer domain.
    Integer {
        /// Inclusive minimum.
        minimum: i64,
        /// Inclusive maximum.
        maximum: i64,
        /// Default value.
        default: i64,
    },
    /// Boolean domain and default.
    Boolean {
        /// Default value.
        default: bool,
    },
    /// Finite stable choice set and default identity.
    Choice {
        /// Stable options in product display order.
        options: Vec<ChoiceOption>,
        /// Default option identity.
        default: ChoiceId,
    },
}

impl ParameterKind {
    /// Return the semantic value type represented by this domain.
    #[must_use]
    pub const fn value_type(&self) -> ParameterType {
        match self {
            Self::Float { .. } => ParameterType::Float,
            Self::Integer { .. } => ParameterType::Integer,
            Self::Boolean { .. } => ParameterType::Boolean,
            Self::Choice { .. } => ParameterType::Choice,
        }
    }
}

/// Immutable descriptor for one parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct ParameterDescriptor {
    key: ParameterKey,
    name: String,
    kind: ParameterKind,
}

impl ParameterDescriptor {
    /// Construct and validate a descriptor from an already-built domain.
    ///
    /// # Errors
    ///
    /// Returns [`ParameterDefinitionError`] when the key, display name, domain,
    /// or default is invalid.
    pub fn new(
        key: impl Into<String>,
        name: impl Into<String>,
        kind: ParameterKind,
    ) -> Result<Self, ParameterDefinitionError> {
        let key = ParameterKey::new(key).map_err(ParameterDefinitionError::InvalidKey)?;
        let name = name.into();
        if name.trim().is_empty() {
            return Err(ParameterDefinitionError::EmptyName(key));
        }
        let descriptor = Self { key, name, kind };
        descriptor.validate()?;
        Ok(descriptor)
    }

    /// Construct a finite floating-point parameter.
    ///
    /// # Errors
    ///
    /// Returns [`ParameterDefinitionError`] when the range or default is invalid.
    pub fn float(
        key: impl Into<String>,
        name: impl Into<String>,
        minimum: f64,
        maximum: f64,
        default: f64,
    ) -> Result<Self, ParameterDefinitionError> {
        Self::new(
            key,
            name,
            ParameterKind::Float {
                minimum,
                maximum,
                default,
            },
        )
    }

    /// Construct a bounded signed-integer parameter.
    ///
    /// # Errors
    ///
    /// Returns [`ParameterDefinitionError`] when the range or default is invalid.
    pub fn integer(
        key: impl Into<String>,
        name: impl Into<String>,
        minimum: i64,
        maximum: i64,
        default: i64,
    ) -> Result<Self, ParameterDefinitionError> {
        Self::new(
            key,
            name,
            ParameterKind::Integer {
                minimum,
                maximum,
                default,
            },
        )
    }

    /// Construct a boolean parameter.
    ///
    /// # Errors
    ///
    /// Returns [`ParameterDefinitionError`] when the key or display name is invalid.
    pub fn boolean(
        key: impl Into<String>,
        name: impl Into<String>,
        default: bool,
    ) -> Result<Self, ParameterDefinitionError> {
        Self::new(key, name, ParameterKind::Boolean { default })
    }

    /// Construct a stable finite choice parameter.
    ///
    /// # Errors
    ///
    /// Returns [`ParameterDefinitionError`] when the key, display name, choice
    /// set, or default is invalid.
    pub fn choice(
        key: impl Into<String>,
        name: impl Into<String>,
        options: Vec<ChoiceOption>,
        default: impl Into<String>,
    ) -> Result<Self, ParameterDefinitionError> {
        let default = ChoiceId::new(default).map_err(ParameterDefinitionError::InvalidChoiceId)?;
        Self::new(key, name, ParameterKind::Choice { options, default })
    }

    /// Return the stable parameter key.
    #[must_use]
    pub fn key(&self) -> &ParameterKey {
        &self.key
    }

    /// Return the human-readable display name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the immutable parameter domain metadata.
    #[must_use]
    pub const fn kind(&self) -> &ParameterKind {
        &self.kind
    }

    /// Return the parameter's semantic value type.
    #[must_use]
    pub const fn value_type(&self) -> ParameterType {
        self.kind.value_type()
    }

    /// Return the validated default value.
    #[must_use]
    pub fn default_value(&self) -> ParameterValue {
        match &self.kind {
            ParameterKind::Float { default, .. } => ParameterValue::Float(*default),
            ParameterKind::Integer { default, .. } => ParameterValue::Integer(*default),
            ParameterKind::Boolean { default } => ParameterValue::Boolean(*default),
            ParameterKind::Choice { default, .. } => ParameterValue::Choice(default.clone()),
        }
    }

    /// Validate this descriptor's complete domain and default.
    ///
    /// # Errors
    ///
    /// Returns [`ParameterDefinitionError`] when a range, default, or choice
    /// set violates the descriptor contract.
    pub fn validate(&self) -> Result<(), ParameterDefinitionError> {
        match &self.kind {
            ParameterKind::Float {
                minimum,
                maximum,
                default,
            } => {
                if !minimum.is_finite() || !maximum.is_finite() || minimum > maximum {
                    return Err(ParameterDefinitionError::InvalidFloatRange(
                        self.key.clone(),
                    ));
                }
                if !default.is_finite() {
                    return Err(ParameterDefinitionError::NonFiniteFloat(self.key.clone()));
                }
                if !(*minimum..=*maximum).contains(default) {
                    return Err(ParameterDefinitionError::FloatDefaultOutOfRange(
                        self.key.clone(),
                    ));
                }
            }
            ParameterKind::Integer {
                minimum,
                maximum,
                default,
            } => {
                if minimum > maximum {
                    return Err(ParameterDefinitionError::InvalidIntegerRange(
                        self.key.clone(),
                    ));
                }
                if !(*minimum..=*maximum).contains(default) {
                    return Err(ParameterDefinitionError::IntegerDefaultOutOfRange(
                        self.key.clone(),
                    ));
                }
            }
            ParameterKind::Boolean { .. } => {}
            ParameterKind::Choice { options, default } => {
                if options.is_empty() {
                    return Err(ParameterDefinitionError::EmptyChoices(self.key.clone()));
                }
                for (index, option) in options.iter().enumerate() {
                    if option.name.trim().is_empty() {
                        return Err(ParameterDefinitionError::EmptyChoiceName {
                            parameter: self.key.clone(),
                            index,
                        });
                    }
                    if options[..index]
                        .iter()
                        .any(|previous| previous.id == option.id)
                    {
                        return Err(ParameterDefinitionError::DuplicateChoice {
                            parameter: self.key.clone(),
                            choice: option.id.clone(),
                        });
                    }
                }
                if !options.iter().any(|option| option.id == *default) {
                    return Err(ParameterDefinitionError::UnknownChoiceDefault {
                        parameter: self.key.clone(),
                        choice: default.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Validate one proposed value against this descriptor.
    ///
    /// # Errors
    ///
    /// Returns [`ParameterValueError`] when the value has the wrong type, is
    /// outside its domain, or is not a known choice.
    pub fn validate_value(&self, value: &ParameterValue) -> Result<(), ParameterValueError> {
        match (&self.kind, value) {
            (
                ParameterKind::Float {
                    minimum, maximum, ..
                },
                ParameterValue::Float(value),
            ) => {
                if !value.is_finite() {
                    return Err(ParameterValueError::NonFiniteFloat);
                }
                if !(*minimum..=*maximum).contains(value) {
                    return Err(ParameterValueError::FloatOutOfRange);
                }
            }
            (
                ParameterKind::Integer {
                    minimum, maximum, ..
                },
                ParameterValue::Integer(value),
            ) => {
                if !(*minimum..=*maximum).contains(value) {
                    return Err(ParameterValueError::IntegerOutOfRange);
                }
            }
            (ParameterKind::Boolean { .. }, ParameterValue::Boolean(_)) => {}
            (ParameterKind::Choice { options, .. }, ParameterValue::Choice(value)) => {
                if !options.iter().any(|option| option.id == *value) {
                    return Err(ParameterValueError::UnknownChoice);
                }
            }
            (kind, value) => {
                return Err(ParameterValueError::WrongType {
                    expected: kind.value_type(),
                    actual: value.value_type(),
                });
            }
        }
        Ok(())
    }

    /// Validate one borrowed process-time event value against this descriptor.
    ///
    /// # Errors
    ///
    /// Returns [`ParameterValueError`] when the event has the wrong type, is
    /// outside this domain, or names an unknown choice.
    pub fn validate_event_value(
        &self,
        value: &ParameterEventValue<'_>,
    ) -> Result<(), ParameterValueError> {
        match (&self.kind, value) {
            (
                ParameterKind::Float {
                    minimum, maximum, ..
                },
                ParameterEventValue::Float(value),
            ) => {
                if !value.is_finite() {
                    return Err(ParameterValueError::NonFiniteFloat);
                }
                if !(*minimum..=*maximum).contains(value) {
                    return Err(ParameterValueError::FloatOutOfRange);
                }
            }
            (
                ParameterKind::Integer {
                    minimum, maximum, ..
                },
                ParameterEventValue::Integer(value),
            ) => {
                if !(*minimum..=*maximum).contains(value) {
                    return Err(ParameterValueError::IntegerOutOfRange);
                }
            }
            (ParameterKind::Boolean { .. }, ParameterEventValue::Boolean(_)) => {}
            (ParameterKind::Choice { options, .. }, ParameterEventValue::Choice(value)) => {
                if !options.iter().any(|option| option.id.as_str() == *value) {
                    return Err(ParameterValueError::UnknownChoice);
                }
            }
            (kind, value) => {
                return Err(ParameterValueError::WrongType {
                    expected: kind.value_type(),
                    actual: match value {
                        ParameterEventValue::Float(_) => ParameterType::Float,
                        ParameterEventValue::Integer(_) => ParameterType::Integer,
                        ParameterEventValue::Boolean(_) => ParameterType::Boolean,
                        ParameterEventValue::Choice(_) => ParameterType::Choice,
                    },
                });
            }
        }
        Ok(())
    }
}

/// Invalid immutable parameter schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParameterDefinitionError {
    /// Key text was not a valid stable identifier.
    InvalidKey(ParameterKeyError),
    /// Choice identity was not a valid stable identifier.
    InvalidChoiceId(ChoiceIdError),
    /// Display name was empty or whitespace-only.
    EmptyName(ParameterKey),
    /// Floating-point range was non-finite or reversed.
    InvalidFloatRange(ParameterKey),
    /// Floating-point default was non-finite.
    NonFiniteFloat(ParameterKey),
    /// Floating-point default was outside its inclusive range.
    FloatDefaultOutOfRange(ParameterKey),
    /// Integer range was reversed.
    InvalidIntegerRange(ParameterKey),
    /// Integer default was outside its inclusive range.
    IntegerDefaultOutOfRange(ParameterKey),
    /// Choice parameter had no options.
    EmptyChoices(ParameterKey),
    /// Choice option display name was empty.
    EmptyChoiceName {
        /// Parameter containing the option.
        parameter: ParameterKey,
        /// Zero-based option index.
        index: usize,
    },
    /// Choice identities were duplicated.
    DuplicateChoice {
        /// Parameter containing the duplicate.
        parameter: ParameterKey,
        /// Duplicated identity.
        choice: ChoiceId,
    },
    /// Choice default did not identify an option.
    UnknownChoiceDefault {
        /// Parameter containing the default.
        parameter: ParameterKey,
        /// Unknown default identity.
        choice: ChoiceId,
    },
    /// Two descriptors used one stable parameter key.
    DuplicateParameter(ParameterKey),
}

impl fmt::Display for ParameterDefinitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidKey(error) => error.fmt(formatter),
            Self::InvalidChoiceId(error) => error.fmt(formatter),
            Self::EmptyName(key) => write!(formatter, "parameter {key} has an empty display name"),
            Self::InvalidFloatRange(key) => {
                write!(formatter, "parameter {key} has an invalid float range")
            }
            Self::NonFiniteFloat(key) => {
                write!(formatter, "parameter {key} has a non-finite float default")
            }
            Self::FloatDefaultOutOfRange(key) => write!(
                formatter,
                "parameter {key} has a float default outside its range"
            ),
            Self::InvalidIntegerRange(key) => {
                write!(formatter, "parameter {key} has an invalid integer range")
            }
            Self::IntegerDefaultOutOfRange(key) => write!(
                formatter,
                "parameter {key} has an integer default outside its range"
            ),
            Self::EmptyChoices(key) => write!(formatter, "choice parameter {key} has no options"),
            Self::EmptyChoiceName { parameter, index } => write!(
                formatter,
                "choice parameter {parameter} option {index} has an empty display name"
            ),
            Self::DuplicateChoice { parameter, choice } => write!(
                formatter,
                "choice parameter {parameter} repeats option {choice}"
            ),
            Self::UnknownChoiceDefault { parameter, choice } => write!(
                formatter,
                "choice parameter {parameter} defaults to unknown option {choice}"
            ),
            Self::DuplicateParameter(key) => write!(formatter, "parameter key {key} is duplicated"),
        }
    }
}

impl std::error::Error for ParameterDefinitionError {}

/// Invalid proposed parameter value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterValueError {
    /// Proposed value had the wrong semantic type.
    WrongType {
        /// Descriptor's expected type.
        expected: ParameterType,
        /// Proposed value type.
        actual: ParameterType,
    },
    /// Float value was not finite.
    NonFiniteFloat,
    /// Float value was outside its inclusive range.
    FloatOutOfRange,
    /// Integer value was outside its inclusive range.
    IntegerOutOfRange,
    /// Choice identity was not in the descriptor's option set.
    UnknownChoice,
    /// State contained a value type that is not a parameter value type.
    UnsupportedStateValue(&'static str),
}

impl fmt::Display for ParameterValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongType { expected, actual } => {
                write!(formatter, "parameter expects {expected}, received {actual}")
            }
            Self::NonFiniteFloat => formatter.write_str("parameter float must be finite"),
            Self::FloatOutOfRange => formatter.write_str("parameter float is outside its range"),
            Self::IntegerOutOfRange => {
                formatter.write_str("parameter integer is outside its range")
            }
            Self::UnknownChoice => formatter.write_str("unknown parameter choice"),
            Self::UnsupportedStateValue(value_type) => {
                write!(
                    formatter,
                    "state value type {value_type} cannot be a parameter"
                )
            }
        }
    }
}

impl std::error::Error for ParameterValueError {}

/// Bounds for a parameter schema/store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParameterLimits {
    /// Maximum number of parameters in one schema.
    pub max_parameters: u32,
    /// Maximum number of choices on one parameter.
    pub max_choices_per_parameter: u32,
}

impl Default for ParameterLimits {
    fn default() -> Self {
        Self {
            max_parameters: 4096,
            max_choices_per_parameter: 1024,
        }
    }
}

/// Failure while constructing or updating a [`ParameterStore`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParameterStoreError {
    /// Schema definition was invalid.
    InvalidDefinition(ParameterDefinitionError),
    /// Schema exceeded an explicit bound.
    SchemaTooLarge {
        /// Actual count.
        actual: usize,
        /// Configured maximum count.
        maximum: u32,
    },
    /// One choice list exceeded an explicit bound.
    ChoicesTooLarge {
        /// Parameter containing the choices.
        parameter: ParameterKey,
        /// Actual count.
        actual: usize,
        /// Configured maximum count.
        maximum: u32,
    },
    /// No descriptor matched the requested key.
    UnknownParameter(String),
    /// Proposed value failed descriptor validation.
    InvalidValue {
        /// Requested parameter key.
        parameter: String,
        /// Domain validation failure.
        error: ParameterValueError,
    },
}

impl fmt::Display for ParameterStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDefinition(error) => {
                write!(formatter, "invalid parameter definition: {error}")
            }
            Self::SchemaTooLarge { actual, maximum } => write!(
                formatter,
                "parameter schema has {actual} entries but {maximum} are allowed"
            ),
            Self::ChoicesTooLarge {
                parameter,
                actual,
                maximum,
            } => write!(
                formatter,
                "parameter {parameter} has {actual} choices but {maximum} are allowed"
            ),
            Self::UnknownParameter(key) => write!(formatter, "unknown parameter {key}"),
            Self::InvalidValue { parameter, error } => write!(
                formatter,
                "invalid value for parameter {parameter}: {error}"
            ),
        }
    }
}

impl std::error::Error for ParameterStoreError {}

/// One component instance's validated, mutable base/control values.
///
/// The schema is owned immutably and values are kept aligned with it. This
/// store is intended for control/non-realtime ownership; normalized process
/// events are validated against it but remain borrowed process-local data.
pub struct ParameterStore {
    descriptors: Vec<ParameterDescriptor>,
    values: Vec<ParameterValue>,
    lookup: Vec<usize>,
}

impl ParameterStore {
    /// Construct an owned store with conventional bounds and descriptor defaults.
    ///
    /// # Errors
    ///
    /// Returns [`ParameterStoreError`] when the schema is invalid or exceeds a
    /// conventional bound.
    pub fn new(descriptors: &[ParameterDescriptor]) -> Result<Self, ParameterStoreError> {
        Self::new_with_limits(descriptors, ParameterLimits::default())
    }

    /// Construct a store after validating schema and explicit bounds.
    ///
    /// # Errors
    ///
    /// Returns [`ParameterStoreError`] when a descriptor is invalid, a key is
    /// duplicated, or a configured count is exceeded.
    pub fn new_with_limits(
        descriptors: &[ParameterDescriptor],
        limits: ParameterLimits,
    ) -> Result<Self, ParameterStoreError> {
        if descriptors.len() > limits.max_parameters as usize {
            return Err(ParameterStoreError::SchemaTooLarge {
                actual: descriptors.len(),
                maximum: limits.max_parameters,
            });
        }
        for descriptor in descriptors {
            descriptor
                .validate()
                .map_err(ParameterStoreError::InvalidDefinition)?;
            if let ParameterKind::Choice { options, .. } = descriptor.kind()
                && options.len() > limits.max_choices_per_parameter as usize
            {
                return Err(ParameterStoreError::ChoicesTooLarge {
                    parameter: descriptor.key().clone(),
                    actual: options.len(),
                    maximum: limits.max_choices_per_parameter,
                });
            }
        }
        for (index, descriptor) in descriptors.iter().enumerate() {
            if descriptors[..index]
                .iter()
                .any(|previous| previous.key() == descriptor.key())
            {
                return Err(ParameterStoreError::InvalidDefinition(
                    ParameterDefinitionError::DuplicateParameter(descriptor.key().clone()),
                ));
            }
        }
        let descriptors = descriptors.to_vec();
        let values = descriptors
            .iter()
            .map(ParameterDescriptor::default_value)
            .collect();
        let mut lookup: Vec<_> = (0..descriptors.len()).collect();
        lookup.sort_unstable_by(|left, right| {
            descriptors[*left].key().cmp(descriptors[*right].key())
        });
        Ok(Self {
            descriptors,
            values,
            lookup,
        })
    }

    /// Return the immutable parameter schema.
    #[must_use]
    pub fn descriptors(&self) -> &[ParameterDescriptor] {
        &self.descriptors
    }

    /// Resolve a stable key to its dense schema-local runtime index.
    #[must_use]
    pub fn index(&self, key: impl AsRef<str>) -> Option<ParameterIndex> {
        let position = self.position_for_key(key.as_ref())?;
        let index = u32::try_from(position).ok()?;
        Some(ParameterIndex::new(index))
    }

    /// Return the descriptor at one dense schema-local index.
    #[must_use]
    pub fn descriptor(&self, index: ParameterIndex) -> Option<&ParameterDescriptor> {
        let position = usize::try_from(index.get()).ok()?;
        self.descriptors.get(position)
    }

    /// Return the current value at one dense schema-local index.
    #[must_use]
    pub fn get_index(&self, index: ParameterIndex) -> Option<&ParameterValue> {
        let position = usize::try_from(index.get()).ok()?;
        self.values.get(position)
    }

    /// Return the current value for a stable key.
    #[must_use]
    pub fn get(&self, key: impl AsRef<str>) -> Option<&ParameterValue> {
        self.position_for_key(key.as_ref())
            .map(|index| &self.values[index])
    }

    /// Replace one value after validating it against its descriptor.
    ///
    /// The store is unchanged if the key is unknown or the value is invalid.
    ///
    /// # Errors
    ///
    /// Returns [`ParameterStoreError`] when the key is unknown or the value
    /// fails the descriptor's type/domain validation.
    pub fn set(
        &mut self,
        key: impl AsRef<str>,
        value: ParameterValue,
    ) -> Result<(), ParameterStoreError> {
        let key = key.as_ref();
        let index = self
            .position_for_key(key)
            .ok_or_else(|| ParameterStoreError::UnknownParameter(key.to_owned()))?;
        self.descriptors[index]
            .validate_value(&value)
            .map_err(|error| ParameterStoreError::InvalidValue {
                parameter: key.to_owned(),
                error,
            })?;
        self.values[index] = value;
        Ok(())
    }

    /// Restore all values to their validated defaults.
    pub fn reset(&mut self) {
        for (value, descriptor) in self.values.iter_mut().zip(self.descriptors.iter()) {
            *value = descriptor.default_value();
        }
    }

    /// Iterate over descriptors and current values in declaration order.
    pub fn iter(&self) -> impl Iterator<Item = (&ParameterDescriptor, &ParameterValue)> {
        self.descriptors.iter().zip(&self.values)
    }

    /// Build a semantic state document containing all parameter base values.
    ///
    /// # Errors
    ///
    /// Returns [`ParameterStateError::Document`] if the product identity or an
    /// entry key is invalid.
    pub fn state_document(
        &self,
        product_id: impl Into<String>,
        product_schema: u32,
    ) -> Result<StateDocument, ParameterStateError> {
        let mut document = StateDocument::new(product_id, product_schema)
            .map_err(ParameterStateError::Document)?;
        for (descriptor, value) in self.iter() {
            document
                .insert(StateEntry::new(
                    parameter_state_key(descriptor.key()),
                    state_value(value),
                ))
                .map_err(ParameterStateError::Document)?;
        }
        Ok(document)
    }

    /// Validate normalized process-time parameter events against this schema.
    ///
    /// This performs no allocation or stable-key lookup on the successful path
    /// and leaves the store unchanged. Adapters resolve persistent/backend
    /// identity to [`ParameterIndex`] before constructing the process event
    /// stream.
    ///
    /// # Errors
    ///
    /// Returns [`ParameterAutomationError`] for out-of-range indices or values
    /// outside their descriptor domains.
    pub fn validate_events(
        &self,
        events: ParameterEvents<'_>,
    ) -> Result<(), ParameterAutomationError> {
        for (event_index, event) in events.iter().enumerate() {
            let Some(descriptor) = self.descriptor(event.parameter()) else {
                return Err(ParameterAutomationError::UnknownParameter { index: event_index });
            };
            let value = match event.change() {
                ParameterEventChange::Set(value) => value,
                ParameterEventChange::Linear { value } => ParameterEventValue::Float(value),
            };
            descriptor.validate_event_value(&value).map_err(|error| {
                ParameterAutomationError::InvalidValue {
                    index: event_index,
                    error,
                }
            })?;
        }
        Ok(())
    }

    /// Encode all parameter base values using explicit state bounds.
    ///
    /// # Errors
    ///
    /// Returns [`ParameterStateError`] when document construction or bounded
    /// state encoding fails.
    pub fn encode_state(
        &self,
        product_id: impl Into<String>,
        product_schema: u32,
        limits: StateLimits,
    ) -> Result<Vec<u8>, ParameterStateError> {
        self.state_document(product_id, product_schema)?
            .encode_with_limits(limits)
            .map_err(ParameterStateError::Encode)
    }

    fn apply_state_entries(&mut self, document: &StateDocument) -> Result<(), ParameterStateError> {
        let mut candidate = self.values.clone();
        for entry in document.entries() {
            let Some(key) = entry.key().strip_prefix("parameter/") else {
                continue;
            };
            let index = self
                .position_for_key(key)
                .ok_or_else(|| ParameterStateError::UnknownParameter(key.to_owned()))?;
            let value = parameter_value(entry.value()).map_err(|error| {
                ParameterStateError::InvalidValue {
                    parameter: key.to_owned(),
                    error,
                }
            })?;
            self.descriptors[index]
                .validate_value(&value)
                .map_err(|error| ParameterStateError::InvalidValue {
                    parameter: key.to_owned(),
                    error,
                })?;
            candidate[index] = value;
        }
        self.values = candidate;
        Ok(())
    }

    /// Apply state after checking product identity and schema version.
    ///
    /// # Errors
    ///
    /// Returns [`ParameterStateError`] for an identity/schema mismatch or an
    /// invalid parameter entry. The store remains unchanged on failure.
    pub fn apply_state_for_product(
        &mut self,
        document: &StateDocument,
        product_id: &str,
        product_schema: u32,
    ) -> Result<(), ParameterStateError> {
        if document.product_id() != product_id {
            return Err(ParameterStateError::ProductIdentityMismatch {
                expected: product_id.to_owned(),
                actual: document.product_id().to_owned(),
            });
        }
        if document.product_schema() != product_schema {
            return Err(ParameterStateError::ProductSchemaMismatch {
                expected: product_schema,
                actual: document.product_schema(),
            });
        }
        self.apply_state_entries(document)
    }

    fn position_for_key(&self, key: &str) -> Option<usize> {
        self.lookup
            .binary_search_by(|index| self.descriptors[*index].key().as_str().cmp(key))
            .ok()
            .map(|position| self.lookup[position])
    }
}

fn parameter_state_key(key: &ParameterKey) -> String {
    let mut state_key = String::from("parameter/");
    state_key.push_str(key.as_str());
    state_key
}

fn state_value(value: &ParameterValue) -> StateValue {
    match value {
        ParameterValue::Float(value) => StateValue::Float(*value),
        ParameterValue::Integer(value) => StateValue::Signed(*value),
        ParameterValue::Boolean(value) => StateValue::Boolean(*value),
        ParameterValue::Choice(value) => StateValue::Choice(value.as_str().to_owned()),
    }
}

fn parameter_value(value: &StateValue) -> Result<ParameterValue, ParameterValueError> {
    match value {
        StateValue::Float(value) => Ok(ParameterValue::Float(*value)),
        StateValue::Signed(value) => Ok(ParameterValue::Integer(*value)),
        StateValue::Boolean(value) => Ok(ParameterValue::Boolean(*value)),
        StateValue::Choice(value) => ChoiceId::new(value.clone())
            .map(ParameterValue::Choice)
            .map_err(|_| ParameterValueError::UnknownChoice),
        StateValue::Unsigned(_) => Err(ParameterValueError::UnsupportedStateValue("unsigned")),
        StateValue::Text(_) => Err(ParameterValueError::UnsupportedStateValue("text")),
        StateValue::Bytes(_) => Err(ParameterValueError::UnsupportedStateValue("bytes")),
    }
}

/// Failure while validating normalized process-time parameter events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterAutomationError {
    /// Event index did not identify a descriptor in the active schema.
    UnknownParameter {
        /// Zero-based event index.
        index: usize,
    },
    /// Event value failed descriptor validation.
    InvalidValue {
        /// Zero-based event index.
        index: usize,
        /// Domain/type failure.
        error: ParameterValueError,
    },
}

impl fmt::Display for ParameterAutomationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownParameter { index } => {
                write!(
                    formatter,
                    "parameter event {index} targets an unknown parameter index"
                )
            }
            Self::InvalidValue { index, error } => {
                write!(
                    formatter,
                    "invalid value for parameter event {index}: {error}"
                )
            }
        }
    }
}

impl std::error::Error for ParameterAutomationError {}

/// Failure while exporting or applying parameter state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParameterStateError {
    /// Semantic document construction failed.
    Document(StateDocumentError),
    /// State byte encoding failed its bounds or value checks.
    Encode(StateEncodeError),
    /// State belonged to another product identity.
    ProductIdentityMismatch {
        /// Expected product identity.
        expected: String,
        /// Actual document identity.
        actual: String,
    },
    /// State used another product schema version.
    ProductSchemaMismatch {
        /// Expected product schema.
        expected: u32,
        /// Actual document schema.
        actual: u32,
    },
    /// Parameter namespace contained an unknown key.
    UnknownParameter(String),
    /// Parameter namespace contained an invalid value.
    InvalidValue {
        /// Parameter key.
        parameter: String,
        /// Value validation failure.
        error: ParameterValueError,
    },
}

impl fmt::Display for ParameterStateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Document(error) => write!(formatter, "invalid parameter state document: {error}"),
            Self::Encode(error) => write!(formatter, "could not encode parameter state: {error}"),
            Self::ProductIdentityMismatch { expected, actual } => write!(
                formatter,
                "state product identity is {actual:?}, expected {expected:?}"
            ),
            Self::ProductSchemaMismatch { expected, actual } => write!(
                formatter,
                "state product schema is {actual}, expected {expected}"
            ),
            Self::UnknownParameter(key) => write!(formatter, "unknown parameter {key}"),
            Self::InvalidValue { parameter, error } => write!(
                formatter,
                "invalid value for parameter {parameter}: {error}"
            ),
        }
    }
}

impl std::error::Error for ParameterStateError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::ParameterEvent;

    fn choices() -> Vec<ChoiceOption> {
        vec![
            ChoiceOption::new("clean", "Clean").expect("choice id is valid"),
            ChoiceOption::new("warm", "Warm").expect("choice id is valid"),
        ]
    }

    fn descriptors() -> Vec<ParameterDescriptor> {
        vec![
            ParameterDescriptor::float("input.gain", "Input gain", -1.0, 1.0, 0.0)
                .expect("float definition is valid"),
            ParameterDescriptor::integer("quality.level", "Quality", 1, 4, 2)
                .expect("integer definition is valid"),
            ParameterDescriptor::boolean("bypass", "Bypass", false)
                .expect("boolean definition is valid"),
            ParameterDescriptor::choice("mode", "Mode", choices(), "clean")
                .expect("choice definition is valid"),
        ]
    }

    #[test]
    fn schema_validates_domains_defaults_and_duplicate_keys() {
        assert!(matches!(
            ParameterDescriptor::float("gain", "Gain", 1.0, 0.0, 0.5),
            Err(ParameterDefinitionError::InvalidFloatRange(_))
        ));
        assert!(matches!(
            ParameterDescriptor::float("gain", "Gain", 0.0, 1.0, f64::NAN),
            Err(ParameterDefinitionError::NonFiniteFloat(_))
        ));
        assert!(matches!(
            ParameterDescriptor::choice("mode", "Mode", choices(), "missing"),
            Err(ParameterDefinitionError::UnknownChoiceDefault { .. })
        ));

        let definitions = descriptors();
        let duplicate = vec![definitions[0].clone(), definitions[0].clone()];
        assert!(matches!(
            ParameterStore::new(&duplicate),
            Err(ParameterStoreError::InvalidDefinition(
                ParameterDefinitionError::DuplicateParameter(_)
            ))
        ));
    }

    #[test]
    fn store_starts_at_defaults_and_resolves_dense_indices() {
        let definitions = descriptors();
        let mut store = ParameterStore::new(&definitions).expect("schema is valid");
        assert_eq!(store.index("input.gain"), Some(ParameterIndex::new(0)));
        assert_eq!(store.index("mode"), Some(ParameterIndex::new(3)));
        assert_eq!(store.index("missing"), None);
        assert_eq!(
            store.get_index(ParameterIndex::new(0)),
            Some(&ParameterValue::Float(0.0))
        );
        assert_eq!(store.get_index(ParameterIndex::new(99)), None);
        assert_eq!(
            store.get("mode"),
            Some(&ParameterValue::Choice(
                ChoiceId::new("clean").expect("choice id is valid")
            ))
        );

        store
            .set("input.gain", ParameterValue::Float(0.5))
            .expect("value is in range");
        assert_eq!(store.get("input.gain"), Some(&ParameterValue::Float(0.5)));
        let error = store.set("input.gain", ParameterValue::Float(2.0));
        assert_eq!(
            error,
            Err(ParameterStoreError::InvalidValue {
                parameter: "input.gain".into(),
                error: ParameterValueError::FloatOutOfRange,
            })
        );
        assert_eq!(store.get("input.gain"), Some(&ParameterValue::Float(0.5)));
        assert!(matches!(
            store.set("input.gain", ParameterValue::Boolean(true)),
            Err(ParameterStoreError::InvalidValue {
                error: ParameterValueError::WrongType { .. },
                ..
            })
        ));
    }

    #[test]
    fn state_round_trip_uses_parameter_namespace_and_identity() {
        let definitions = descriptors();
        let mut store = ParameterStore::new(&definitions).expect("schema is valid");
        store
            .set("input.gain", ParameterValue::Float(0.75))
            .expect("value is in range");
        store
            .set(
                "mode",
                ParameterValue::Choice(ChoiceId::new("warm").expect("choice id is valid")),
            )
            .expect("choice is known");

        let encoded = store
            .encode_state("com.example.effect", 3, StateLimits::default())
            .expect("parameter state encodes");
        let document = StateDocument::decode(&encoded).expect("parameter state decodes");
        assert!(
            document
                .entries()
                .iter()
                .any(|entry| entry.key() == "parameter/input.gain")
        );

        let mut restored = ParameterStore::new(&definitions).expect("schema is valid");
        restored
            .apply_state_for_product(&document, "com.example.effect", 3)
            .expect("state applies");
        assert_eq!(restored.get("input.gain"), store.get("input.gain"));
        assert_eq!(restored.get("mode"), store.get("mode"));
        assert_eq!(
            restored.apply_state_for_product(&document, "other.product", 3),
            Err(ParameterStateError::ProductIdentityMismatch {
                expected: "other.product".into(),
                actual: "com.example.effect".into(),
            })
        );
    }

    #[test]
    fn state_application_is_transactional_and_ignores_custom_entries() {
        let definitions = descriptors();
        let mut store = ParameterStore::new(&definitions).expect("schema is valid");
        store
            .set("input.gain", ParameterValue::Float(0.5))
            .expect("value is in range");

        let mut invalid = StateDocument::new("com.example.effect", 1).expect("identity is valid");
        invalid
            .insert(StateEntry::new(
                "state/custom",
                StateValue::Bytes(vec![1, 2]),
            ))
            .expect("key is unique");
        invalid
            .insert(StateEntry::new(
                "parameter/input.gain",
                StateValue::Float(0.25),
            ))
            .expect("key is unique");
        invalid
            .insert(StateEntry::new(
                "parameter/quality.level",
                StateValue::Signed(99),
            ))
            .expect("key is unique");
        assert!(matches!(
            store.apply_state_for_product(&invalid, "com.example.effect", 1),
            Err(ParameterStateError::InvalidValue {
                parameter,
                error: ParameterValueError::IntegerOutOfRange,
            }) if parameter == "quality.level"
        ));
        assert_eq!(store.get("input.gain"), Some(&ParameterValue::Float(0.5)));

        invalid
            .insert(StateEntry::new(
                "parameter/missing",
                StateValue::Boolean(true),
            ))
            .expect("key is unique");
        assert!(matches!(
            store.apply_state_for_product(&invalid, "com.example.effect", 1),
            Err(ParameterStateError::InvalidValue { .. } | ParameterStateError::UnknownParameter(_))
        ));
    }

    #[test]
    fn automation_validation_uses_dense_indices_and_parameter_domains() {
        let definitions = descriptors();
        let store = ParameterStore::new(&definitions).expect("schema is valid");
        let gain = store.index("input.gain").expect("gain index exists");
        let quality = store.index("quality.level").expect("quality index exists");
        let mode = store.index("mode").expect("mode index exists");
        let raw_events = [
            ParameterEvent::set(0, gain, ParameterEventValue::Float(0.5)),
            ParameterEvent::linear(2, gain, 1.0),
            ParameterEvent::set(3, mode, ParameterEventValue::Choice("warm")),
        ];
        let events = ParameterEvents::new(&raw_events, 4, 4).expect("events are valid");
        store.validate_events(events).expect("events match schema");

        let invalid = [ParameterEvent::linear(0, quality, 2.0)];
        let invalid = ParameterEvents::new(&invalid, 1, 1).expect("event shape is valid");
        assert!(matches!(
            store.validate_events(invalid),
            Err(ParameterAutomationError::InvalidValue {
                index: 0,
                error: ParameterValueError::WrongType {
                    expected: ParameterType::Integer,
                    actual: ParameterType::Float,
                },
            })
        ));

        let unknown = [ParameterEvent::set(
            0,
            ParameterIndex::new(99),
            ParameterEventValue::Boolean(true),
        )];
        let unknown = ParameterEvents::new(&unknown, 1, 1).expect("event shape is valid");
        assert!(matches!(
            store.validate_events(unknown),
            Err(ParameterAutomationError::UnknownParameter { index: 0 })
        ));
    }

    #[test]
    fn explicit_parameter_bounds_are_enforced() {
        let definitions = descriptors();
        assert!(matches!(
            ParameterStore::new_with_limits(
                &definitions,
                ParameterLimits {
                    max_parameters: 2,
                    max_choices_per_parameter: 2,
                }
            ),
            Err(ParameterStoreError::SchemaTooLarge { .. })
        ));
    }
}
