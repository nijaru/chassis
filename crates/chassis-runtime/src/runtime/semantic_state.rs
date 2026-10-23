//! Complete semantic state admission and non-realtime component policy.

use super::{Component, InstanceRuntime, Processor};
use crate::{
    parameters::{ParameterStateError, ParameterStore, ParameterStoreError},
    schema::ComponentSchemaError,
    state::{
        StateDecodeError, StateDocument, StateDocumentError, StateLimits, StateMigrationError,
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

/// Failure while exporting complete semantic state through schema-owned identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstanceStateExportError {
    /// This component schema has no semantic persistence identity.
    MissingStateIdentity,
    /// Parameter/state document construction or encoding failed.
    State(ParameterStateError),
}

impl fmt::Display for InstanceStateExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingStateIdentity => {
                formatter.write_str("component schema has no semantic state identity")
            }
            Self::State(error) => write!(formatter, "state export failed: {error}"),
        }
    }
}

impl std::error::Error for InstanceStateExportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::MissingStateIdentity => None,
            Self::State(error) => Some(error),
        }
    }
}

/// Failure while replacing the durable instance's framework-managed parameter state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstanceStateError {
    /// The component schema has no semantic persistence identity.
    MissingStateIdentity,
    /// Rebuilding the already-validated parameter schema unexpectedly failed.
    InvalidParameters(ParameterStoreError),
    /// The semantic parameter state was invalid for this product/schema.
    ParameterState(ParameterStateError),
    /// A full state document omitted one or more framework-managed parameters.
    IncompleteParameterState {
        /// Number of parameters required by the current schema.
        expected: usize,
        /// Number of parameter entries supplied by the document.
        actual: usize,
    },
}

impl fmt::Display for InstanceStateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingStateIdentity => {
                formatter.write_str("component schema has no semantic state identity")
            }
            Self::InvalidParameters(error) => write!(formatter, "invalid parameters: {error}"),
            Self::ParameterState(error) => write!(formatter, "invalid parameter state: {error}"),
            Self::IncompleteParameterState { expected, actual } => write!(
                formatter,
                "parameter state contains {actual} parameters but {expected} are required"
            ),
        }
    }
}

impl std::error::Error for InstanceStateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::MissingStateIdentity | Self::IncompleteParameterState { .. } => None,
            Self::InvalidParameters(error) => Some(error),
            Self::ParameterState(error) => Some(error),
        }
    }
}

/// Failure while validating a complete semantic instance state.
#[derive(Debug)]
pub enum InstanceSemanticStateError {
    /// Framework-managed parameter validation failed.
    Parameters(InstanceStateError),
    /// The borrowed component no longer describes this instance generation.
    ComponentSchemaMismatch,
    /// The borrowed component returned an invalid schema.
    InvalidComponentSchema(ComponentSchemaError),
    /// Custom defaults contain invalid, duplicate, or reserved parameter keys.
    InvalidDefaults(StateDocumentError),
    /// Custom defaults cannot override descriptor-owned parameter values.
    ReservedCustomKey(String),
    /// Product-defined validation of the complete candidate state failed.
    Product(ComponentStateError),
}

impl fmt::Display for InstanceSemanticStateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parameters(error) => write!(formatter, "invalid parameter state: {error}"),
            Self::Product(error) => write!(formatter, "invalid product state: {error}"),
            Self::ComponentSchemaMismatch => {
                formatter.write_str("component schema does not match instance")
            }
            Self::InvalidComponentSchema(error) => error.fmt(formatter),
            Self::InvalidDefaults(error) => write!(formatter, "invalid custom defaults: {error}"),
            Self::ReservedCustomKey(key) => {
                write!(formatter, "custom default uses reserved key {key}")
            }
        }
    }
}

impl std::error::Error for InstanceSemanticStateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parameters(error) => Some(error),
            Self::Product(error) => Some(error),
            Self::ComponentSchemaMismatch | Self::ReservedCustomKey(_) => None,
            Self::InvalidComponentSchema(error) => Some(error),
            Self::InvalidDefaults(error) => Some(error),
        }
    }
}

/// Failure while decoding, migrating, or applying a complete semantic state.
#[derive(Debug)]
pub enum InstanceSemanticStateLoadError {
    /// The encoded state was malformed or exceeded its bounds.
    Decode(StateDecodeError),
    /// Product-schema migration failed before live state was touched.
    Migration(StateMigrationError<ComponentStateError>),
    /// The migrated complete state failed framework or product validation.
    Apply(InstanceSemanticStateError),
}

impl fmt::Display for InstanceSemanticStateLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(error) => write!(formatter, "could not decode state: {error}"),
            Self::Migration(error) => write!(formatter, "could not migrate state: {error}"),
            Self::Apply(error) => write!(formatter, "could not apply state: {error}"),
        }
    }
}

impl std::error::Error for InstanceSemanticStateLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Decode(error) => Some(error),
            Self::Migration(error) => Some(error),
            Self::Apply(error) => Some(error),
        }
    }
}

impl<P: Processor> InstanceRuntime<P> {
    fn semantic_state_identity(&self) -> Option<(String, u32)> {
        self.schema.state_identity().map(|identity| {
            (
                identity.component().as_str().to_owned(),
                identity.schema().get(),
            )
        })
    }

    /// Build the complete durable semantic state using schema-owned identity.
    ///
    /// Framework-managed parameters are emitted from the canonical parameter
    /// store and validated custom entries are appended. Encoding remains
    /// deterministic because [`StateDocument`] canonicalizes entry order.
    ///
    /// # Errors
    ///
    /// Returns [`InstanceStateExportError::MissingStateIdentity`] when this
    /// component is intentionally identity-less, or wraps a parameter/state
    /// document error.
    pub fn state_document(&self) -> Result<StateDocument, InstanceStateExportError> {
        let (product_id, product_schema) = self
            .semantic_state_identity()
            .ok_or(InstanceStateExportError::MissingStateIdentity)?;
        let mut document = self
            .parameters
            .state_document(product_id, product_schema)
            .map_err(InstanceStateExportError::State)?;
        for entry in &self.custom_state {
            document
                .insert(entry.clone())
                .map_err(ParameterStateError::Document)
                .map_err(InstanceStateExportError::State)?;
        }
        Ok(document)
    }

    /// Encode complete durable semantic state under schema-owned identity.
    ///
    /// # Errors
    ///
    /// Returns [`InstanceStateExportError`] if identity, document construction,
    /// or bounded encoding fails.
    pub fn encode_state(&self, limits: StateLimits) -> Result<Vec<u8>, InstanceStateExportError> {
        let document = self.state_document()?;
        document
            .encode_with_limits(limits)
            .map_err(ParameterStateError::Encode)
            .map_err(InstanceStateExportError::State)
    }

    fn parameter_state_candidate(
        &self,
        document: &StateDocument,
        product_id: &str,
        product_schema: u32,
    ) -> Result<ParameterStore, InstanceStateError> {
        let descriptors = self.schema.parameters();
        let mut candidate =
            ParameterStore::new(descriptors).map_err(InstanceStateError::InvalidParameters)?;
        candidate
            .apply_state_for_product(document, product_id, product_schema)
            .map_err(InstanceStateError::ParameterState)?;

        let actual = document
            .entries()
            .iter()
            .filter(|entry| entry.key().starts_with("parameter/"))
            .count();
        let expected = descriptors.len();
        if actual != expected {
            return Err(InstanceStateError::IncompleteParameterState { expected, actual });
        }
        Ok(candidate)
    }

    /// Replace complete semantic instance state using schema-owned identity.
    ///
    /// Framework parameters and custom entries are validated into temporary
    /// candidates. Product validation sees both candidates, and neither is
    /// published until every check succeeds. Changed custom state requires a
    /// new activation to reach DSP: an active instance requests restart while
    /// retaining its pre-load parameter projection and resources until
    /// deactivation/reactivation. Parameter events are checked but discarded
    /// throughout that interval; no old automation endpoints are retained.
    ///
    /// # Errors
    ///
    /// Returns [`InstanceSemanticStateError`] for missing identity, framework
    /// state validation failures, or product validation failure. Publication is
    /// atomic across parameter and custom state.
    pub fn apply_state<C>(
        &mut self,
        component: &C,
        document: &StateDocument,
    ) -> Result<(), InstanceSemanticStateError>
    where
        C: Component<Processor = P>,
    {
        self.check_component(component)?;
        let (product_id, product_schema) =
            self.semantic_state_identity()
                .ok_or(InstanceSemanticStateError::Parameters(
                    InstanceStateError::MissingStateIdentity,
                ))?;
        let candidate_parameters = self
            .parameter_state_candidate(document, &product_id, product_schema)
            .map_err(InstanceSemanticStateError::Parameters)?;
        let mut candidate_custom: Vec<_> = document
            .entries()
            .iter()
            .filter(|entry| !entry.key().starts_with("parameter/"))
            .cloned()
            .collect();
        candidate_custom
            .sort_unstable_by(|left, right| left.key().as_bytes().cmp(right.key().as_bytes()));
        component
            .validate_state(&candidate_parameters, &candidate_custom)
            .map_err(InstanceSemanticStateError::Product)?;

        if let Some(active) = &mut self.active {
            if active.frozen_parameters.is_none() && self.custom_state != candidate_custom {
                // Move, rather than clone, the first pre-load generation. Further
                // loads cannot retarget DSP until the active resources are dropped.
                active.frozen_parameters = Some(core::mem::replace(
                    &mut self.parameters,
                    candidate_parameters,
                ));
            } else {
                self.parameters = candidate_parameters;
            }
        } else {
            self.parameters = candidate_parameters;
        }
        self.custom_state = candidate_custom;
        Ok(())
    }

    /// Decode, migrate, validate, and publish complete semantic state using
    /// schema-owned identity and schema version.
    ///
    /// # Errors
    ///
    /// Returns [`InstanceSemanticStateLoadError`] for missing state identity,
    /// bounded decode failures, migration failures, framework validation, or
    /// product validation. The runtime is unchanged on failure.
    pub fn apply_state_bytes<C>(
        &mut self,
        component: &C,
        bytes: &[u8],
        limits: StateLimits,
    ) -> Result<(), InstanceSemanticStateLoadError>
    where
        C: Component<Processor = P>,
    {
        self.check_component(component)
            .map_err(InstanceSemanticStateLoadError::Apply)?;
        let target_schema =
            self.schema
                .state_identity()
                .ok_or(InstanceSemanticStateLoadError::Apply(
                    InstanceSemanticStateError::Parameters(
                        InstanceStateError::MissingStateIdentity,
                    ),
                ))?;
        let document = StateDocument::decode_with_limits(bytes, limits)
            .map_err(InstanceSemanticStateLoadError::Decode)?;
        // Never invoke a product migration for another component's document.
        if document.product_id() != target_schema.component().as_str() {
            return Err(InstanceSemanticStateLoadError::Apply(
                InstanceSemanticStateError::Parameters(InstanceStateError::ParameterState(
                    ParameterStateError::ProductIdentityMismatch {
                        expected: target_schema.component().as_str().to_owned(),
                        actual: document.product_id().to_owned(),
                    },
                )),
            ));
        }
        let document = document
            .migrate_to(target_schema.schema().get(), component.state_migrations())
            .map_err(InstanceSemanticStateLoadError::Migration)?;
        self.apply_state(component, &document)
            .map_err(InstanceSemanticStateLoadError::Apply)
    }

    fn check_component<C>(&self, component: &C) -> Result<(), InstanceSemanticStateError>
    where
        C: Component<Processor = P>,
    {
        let schema = component
            .schema()
            .map_err(InstanceSemanticStateError::InvalidComponentSchema)?;
        if self.schema != schema {
            return Err(InstanceSemanticStateError::ComponentSchemaMismatch);
        }
        Ok(())
    }

    pub(super) fn initialize_state<C>(
        &mut self,
        component: &C,
    ) -> Result<(), InstanceSemanticStateError>
    where
        C: Component<Processor = P>,
    {
        let mut custom = component.default_custom_state();
        for entry in &custom {
            entry
                .validate()
                .map_err(InstanceSemanticStateError::InvalidDefaults)?;
            if entry.key().starts_with("parameter/") {
                return Err(InstanceSemanticStateError::ReservedCustomKey(
                    entry.key().to_owned(),
                ));
            }
        }
        custom.sort_unstable_by(|left, right| left.key().as_bytes().cmp(right.key().as_bytes()));
        for pair in custom.windows(2) {
            if pair[0].key() == pair[1].key() {
                return Err(InstanceSemanticStateError::InvalidDefaults(
                    StateDocumentError::DuplicateKey(pair[0].key().to_owned()),
                ));
            }
        }
        component
            .validate_state(&self.parameters, &custom)
            .map_err(InstanceSemanticStateError::Product)?;
        self.custom_state = custom;
        Ok(())
    }
}
