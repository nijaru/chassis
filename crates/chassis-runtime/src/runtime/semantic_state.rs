//! Complete semantic state admission and non-realtime component policy.

use super::{Component, FrozenControls, InstanceRuntime, Processor};
use crate::{
    parameters::{
        ChoiceId, ParameterStore, ParameterStoreError, ParameterValue, ParameterValueError,
    },
    schema::ComponentSchemaError,
    state::{
        StateDecodeError, StateDocument, StateDocumentError, StateEncodeError, StateEntry,
        StateLimits, StateMigrationError, StateValue,
    },
};
use core::fmt;

/// Non-realtime component validation or migration failure.
///
/// Product errors are erased only at the control-domain policy boundary; they
/// never enter deterministic callbacks.
#[derive(Debug)]
pub enum ComponentStateError {
    /// A parameter-only component received nonempty custom state.
    UnsupportedCustomState,
    /// The component rejected its complete candidate.
    Product(Box<dyn std::error::Error + Send + Sync>),
}

impl ComponentStateError {
    /// Retain a product-specific non-realtime failure and its source.
    pub fn new(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Product(Box::new(error))
    }
}

impl fmt::Display for ComponentStateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedCustomState => f.write_str("component does not support custom state"),
            Self::Product(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ComponentStateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::UnsupportedCustomState => None,
            Self::Product(error) => Some(error.as_ref()),
        }
    }
}

/// Failure while constructing or encoding complete instance state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateExportError {
    /// This component schema has no semantic persistence identity.
    MissingStateIdentity,
    /// Semantic document construction failed.
    Document(StateDocumentError),
    /// Bounded byte encoding failed.
    Encode(StateEncodeError),
}

impl fmt::Display for StateExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingStateIdentity => {
                f.write_str("component schema has no semantic state identity")
            }
            Self::Document(error) => write!(f, "could not construct state: {error}"),
            Self::Encode(error) => write!(f, "could not encode state: {error}"),
        }
    }
}

impl std::error::Error for StateExportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::MissingStateIdentity => None,
            Self::Document(error) => Some(error),
            Self::Encode(error) => Some(error),
        }
    }
}

/// Failure while admitting complete component defaults or replacement state.
#[derive(Debug)]
pub enum StateValidationError {
    /// This component schema has no semantic persistence identity.
    MissingStateIdentity,
    /// The document belongs to another component.
    ComponentIdentityMismatch {
        /// Expected component identity.
        expected: String,
        /// Actual document identity.
        actual: String,
    },
    /// A document applied without migration uses another schema version.
    SchemaVersionMismatch {
        /// Expected schema version.
        expected: u32,
        /// Actual document schema version.
        actual: u32,
    },
    /// Rebuilding the validated parameter schema failed.
    InvalidParameters(ParameterStoreError),
    /// The parameter namespace contains an unknown key.
    UnknownParameter(String),
    /// A parameter value fails conversion or descriptor validation.
    InvalidParameterValue {
        /// Stable parameter key.
        parameter: String,
        /// Specific domain/type failure.
        error: ParameterValueError,
    },
    /// A complete document omitted framework-managed parameters.
    IncompleteParameterState {
        /// Required parameter count.
        expected: usize,
        /// Supplied parameter count.
        actual: usize,
    },
    /// The borrowed component no longer describes this instance generation.
    ComponentSchemaMismatch,
    /// The borrowed component returned an invalid schema.
    InvalidComponentSchema(ComponentSchemaError),
    /// Custom defaults contain invalid or duplicate entries.
    InvalidDefaults(StateDocumentError),
    /// Custom defaults cannot override descriptor-owned parameters.
    ReservedCustomKey(String),
    /// Component-defined complete-state validation failed.
    Product(ComponentStateError),
}

impl fmt::Display for StateValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingStateIdentity => {
                f.write_str("component schema has no semantic state identity")
            }
            Self::ComponentIdentityMismatch { expected, actual } => write!(
                f,
                "state component identity is {actual:?}, expected {expected:?}"
            ),
            Self::SchemaVersionMismatch { expected, actual } => {
                write!(f, "state schema version is {actual}, expected {expected}")
            }
            Self::InvalidParameters(error) => write!(f, "invalid parameters: {error}"),
            Self::UnknownParameter(key) => write!(f, "unknown parameter {key}"),
            Self::InvalidParameterValue { parameter, error } => {
                write!(f, "invalid value for parameter {parameter}: {error}")
            }
            Self::IncompleteParameterState { expected, actual } => write!(
                f,
                "parameter state contains {actual} parameters but {expected} are required"
            ),
            Self::ComponentSchemaMismatch => {
                f.write_str("component schema does not match instance")
            }
            Self::InvalidComponentSchema(error) => error.fmt(f),
            Self::InvalidDefaults(error) => write!(f, "invalid custom defaults: {error}"),
            Self::ReservedCustomKey(key) => write!(f, "custom default uses reserved key {key}"),
            Self::Product(error) => write!(f, "invalid component state: {error}"),
        }
    }
}

impl std::error::Error for StateValidationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidParameters(error) => Some(error),
            Self::InvalidParameterValue { error, .. } => Some(error),
            Self::InvalidComponentSchema(error) => Some(error),
            Self::InvalidDefaults(error) => Some(error),
            Self::Product(error) => Some(error),
            Self::MissingStateIdentity
            | Self::ComponentIdentityMismatch { .. }
            | Self::SchemaVersionMismatch { .. }
            | Self::UnknownParameter(_)
            | Self::IncompleteParameterState { .. }
            | Self::ComponentSchemaMismatch
            | Self::ReservedCustomKey(_) => None,
        }
    }
}

/// Failure during bounded decode, adjacent migration, or complete validation.
#[derive(Debug)]
pub enum StateLoadError {
    /// The encoded state was malformed or exceeded its bounds.
    Decode(StateDecodeError),
    /// Component-schema migration failed before publication.
    Migration(StateMigrationError<ComponentStateError>),
    /// The complete candidate failed framework or component validation.
    Validation(StateValidationError),
}

impl fmt::Display for StateLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(error) => write!(f, "could not decode state: {error}"),
            Self::Migration(error) => write!(f, "could not migrate state: {error}"),
            Self::Validation(error) => write!(f, "could not validate state: {error}"),
        }
    }
}

impl std::error::Error for StateLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Decode(error) => Some(error),
            Self::Migration(error) => Some(error),
            Self::Validation(error) => Some(error),
        }
    }
}

impl<P: Processor> InstanceRuntime<P> {
    /// Build complete durable state using the component schema's identity.
    ///
    /// # Errors
    ///
    /// Returns [`StateExportError`] for missing identity or document failure.
    pub fn state_document(&self) -> Result<StateDocument, StateExportError> {
        let identity = self
            .schema
            .state_identity()
            .ok_or(StateExportError::MissingStateIdentity)?;
        let mut document = StateDocument::new(
            identity.component().as_str(),
            identity.schema_version().get(),
        )
        .map_err(StateExportError::Document)?;
        for (descriptor, value) in self.parameters.iter() {
            document
                .insert(StateEntry::parameter(
                    descriptor.key().as_str(),
                    state_value(value),
                ))
                .map_err(StateExportError::Document)?;
        }
        for entry in &self.custom_state {
            document
                .insert(entry.clone())
                .map_err(StateExportError::Document)?;
        }
        Ok(document)
    }

    /// Encode complete durable state under explicit bounds.
    ///
    /// # Errors
    ///
    /// Returns [`StateExportError`] for identity, document, or encoding failure.
    pub fn encode_state(&self, limits: StateLimits) -> Result<Vec<u8>, StateExportError> {
        self.state_document()?
            .encode_with_limits(limits)
            .map_err(StateExportError::Encode)
    }

    fn parameter_state_candidate(
        &self,
        document: &StateDocument,
    ) -> Result<ParameterStore, StateValidationError> {
        let identity = self
            .schema
            .state_identity()
            .ok_or(StateValidationError::MissingStateIdentity)?;
        check_identity(document, identity.component().as_str())?;
        if document.schema_version() != identity.schema_version().get() {
            return Err(StateValidationError::SchemaVersionMismatch {
                expected: identity.schema_version().get(),
                actual: document.schema_version(),
            });
        }
        let mut candidate = ParameterStore::new(self.schema.parameters())
            .map_err(StateValidationError::InvalidParameters)?;
        let mut actual = 0;
        for entry in document.entries() {
            let Some(key) = entry.parameter_key() else {
                continue;
            };
            if candidate.get(key).is_none() {
                return Err(StateValidationError::UnknownParameter(key.to_owned()));
            }
            let value = parameter_value(entry.value()).map_err(|error| {
                StateValidationError::InvalidParameterValue {
                    parameter: key.to_owned(),
                    error,
                }
            })?;
            candidate.set(key, value).map_err(|error| match error {
                ParameterStoreError::InvalidValue { parameter, error } => {
                    StateValidationError::InvalidParameterValue { parameter, error }
                }
                error => StateValidationError::InvalidParameters(error),
            })?;
            actual += 1;
        }
        let expected = self.schema.parameters().len();
        if actual != expected {
            return Err(StateValidationError::IncompleteParameterState { expected, actual });
        }
        Ok(candidate)
    }

    /// Replace complete state after framework and component validation.
    ///
    /// Changed custom state freezes active DSP's pre-load controls/resources
    /// until reactivation. Desired state remains independently saveable.
    ///
    /// # Errors
    ///
    /// Returns [`StateValidationError`] without publishing either state domain
    /// or changing restart status if any admission check fails.
    pub fn apply_state<C>(
        &mut self,
        component: &C,
        document: &StateDocument,
    ) -> Result<(), StateValidationError>
    where
        C: Component<Processor = P>,
    {
        self.check_component(component)?;
        let candidate_parameters = self.parameter_state_candidate(document)?;
        let mut candidate_custom: Vec<_> = document
            .entries()
            .iter()
            .filter(|entry| entry.parameter_key().is_none())
            .cloned()
            .collect();
        candidate_custom
            .sort_unstable_by(|left, right| left.key().as_bytes().cmp(right.key().as_bytes()));
        component
            .validate_state(&candidate_parameters, &candidate_custom)
            .map_err(StateValidationError::Product)?;

        if let Some(active) = &mut self.active {
            if active.frozen_controls.is_none() && self.custom_state != candidate_custom {
                // Snapshot the consumed signal off-thread and move the first
                // pre-load base generation. Later loads cannot retarget old DSP.
                let modulation = self.modulation.clone();
                active.frozen_controls = Some(FrozenControls {
                    parameters: core::mem::replace(&mut self.parameters, candidate_parameters),
                    modulation,
                });
            } else {
                self.parameters = candidate_parameters;
            }
        } else {
            self.parameters = candidate_parameters;
        }
        self.custom_state = candidate_custom;
        Ok(())
    }

    /// Decode, migrate, validate, and publish complete semantic state.
    ///
    /// # Errors
    ///
    /// Returns [`StateLoadError`] identifying the failed phase. The runtime is
    /// unchanged on failure; migrations never see another component's state.
    pub fn apply_state_bytes<C>(
        &mut self,
        component: &C,
        bytes: &[u8],
        limits: StateLimits,
    ) -> Result<(), StateLoadError>
    where
        C: Component<Processor = P>,
    {
        self.check_component(component)
            .map_err(StateLoadError::Validation)?;
        let identity = self
            .schema
            .state_identity()
            .ok_or(StateLoadError::Validation(
                StateValidationError::MissingStateIdentity,
            ))?;
        let document =
            StateDocument::decode_with_limits(bytes, limits).map_err(StateLoadError::Decode)?;
        check_identity(&document, identity.component().as_str())
            .map_err(StateLoadError::Validation)?;
        let document = document
            .migrate_to(
                identity.schema_version().get(),
                component.state_migrations(),
            )
            .map_err(StateLoadError::Migration)?;
        self.apply_state(component, &document)
            .map_err(StateLoadError::Validation)
    }

    fn check_component<C>(&self, component: &C) -> Result<(), StateValidationError>
    where
        C: Component<Processor = P>,
    {
        let schema = component
            .schema()
            .map_err(StateValidationError::InvalidComponentSchema)?;
        if self.schema != schema {
            return Err(StateValidationError::ComponentSchemaMismatch);
        }
        Ok(())
    }

    pub(super) fn initialize_state<C>(&mut self, component: &C) -> Result<(), StateValidationError>
    where
        C: Component<Processor = P>,
    {
        let mut custom = component.default_custom_state();
        for entry in &custom {
            entry
                .validate()
                .map_err(StateValidationError::InvalidDefaults)?;
            if entry.parameter_key().is_some() {
                return Err(StateValidationError::ReservedCustomKey(
                    entry.key().to_owned(),
                ));
            }
        }
        custom.sort_unstable_by(|left, right| left.key().as_bytes().cmp(right.key().as_bytes()));
        for pair in custom.windows(2) {
            if pair[0].key() == pair[1].key() {
                return Err(StateValidationError::InvalidDefaults(
                    StateDocumentError::DuplicateKey(pair[0].key().to_owned()),
                ));
            }
        }
        component
            .validate_state(&self.parameters, &custom)
            .map_err(StateValidationError::Product)?;
        self.custom_state = custom;
        Ok(())
    }
}

fn check_identity(document: &StateDocument, expected: &str) -> Result<(), StateValidationError> {
    if document.component_id() != expected {
        return Err(StateValidationError::ComponentIdentityMismatch {
            expected: expected.to_owned(),
            actual: document.component_id().to_owned(),
        });
    }
    Ok(())
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
