//! Explicit component/processor lifecycle contracts.
//!
//! The framework-owned [`InstanceRuntime`] is the durable authority for one
//! component instance's base parameter state and active lifecycle. The older
//! [`Activated`] shell remains temporarily for adapter migration; it is an
//! activation-local projection rather than the long-lived state authority.

use core::fmt;
use std::vec::Vec;

use crate::{
    audio::{
        AudioIoConfiguration, AudioIoConfigurationError, AudioPortDescriptor, ConfiguredAudioPort,
        DEFAULT_EFFECT_PORTS,
    },
    buffer::ChannelBuffer,
    parameters::{ParameterDescriptor, ParameterStateError, ParameterStore, ParameterStoreError},
    process::{ActivationConfig, ProcessBlock, ProcessBlockError, ProcessConfig, ProcessContext},
    state::{StateDecodeError, StateDocument, StateLimits, StateMigration, StateMigrationError},
};

/// Immutable product definition/factory for one Chassis component type.
///
/// The component is not the live mutable processor. An ordinary effect can use
/// the default stereo-main/optional-sidechain port schema and only implement
/// activation. Instruments and unusual processors override [`Self::audio_ports`]
/// without changing the processor lifecycle model.
pub trait Component {
    /// Realtime processor created for one successful activation.
    type Processor: Processor;

    /// Product-specific activation failure.
    type ActivationError;

    /// Return the stable audio-port schema for this component.
    ///
    /// The explicit pre-alpha API defaults to the standard effect convention.
    /// Components with different I/O override this method.
    #[must_use]
    fn audio_ports(&self) -> &[AudioPortDescriptor] {
        &DEFAULT_EFFECT_PORTS
    }

    /// Return the immutable parameter schema for this component instance.
    ///
    /// The instance runtime clones this schema once when the instance is
    /// created. The clone is immutable; only the instance's base values are
    /// mutable. Components without parameters use the empty default schema.
    #[must_use]
    fn parameter_descriptors(&self) -> &[ParameterDescriptor] {
        &[]
    }

    /// Create the exclusively-owned realtime processor for one activation.
    ///
    /// All allocation/precomputation required by the processor belongs here or
    /// in product helpers called from here, before realtime processing begins.
    ///
    /// # Errors
    ///
    /// Returns the product-defined activation error when required resources or
    /// product invariants cannot be established.
    fn activate(
        &self,
        config: &ActivationConfig<'_>,
    ) -> Result<Self::Processor, Self::ActivationError>;

    /// Create the realtime processor with the current validated base state.
    ///
    /// This is the durable-instance activation hook. The default delegates to
    /// [`Self::activate`] so existing components that do not need state-aware
    /// preparation remain simple. Components whose activation-time resources
    /// depend on current base values may override this method.
    ///
    /// # Errors
    ///
    /// Returns the same product-defined activation error as [`Self::activate`].
    fn activate_with_parameters(
        &self,
        config: &ActivationConfig<'_>,
        parameters: &ParameterStore,
    ) -> Result<Self::Processor, Self::ActivationError> {
        let _ = parameters;
        self.activate(config)
    }
}

/// Mutable realtime DSP/runtime-history owner while a component is active.
///
/// The format-independent core does not require [`Send`] because a Chassis
/// component may be embedded in a deliberately same-thread runtime. A deployment
/// adapter that transfers processor ownership across threads must add the
/// appropriate `Send` bound at that boundary. `Sync` is not a normal processing
/// requirement because processor mutation remains exclusive.
///
/// `Processor` deliberately contains lifecycle/reset semantics only. Sample
/// precision is expressed by implementing [`Process`], so adding an f64 path
/// later does not require a second processor architecture.
pub trait Processor {
    /// Reset transient DSP history while preserving persistent/control state.
    ///
    /// Stateless processors may use the default no-op implementation.
    fn reset(&mut self) {}
}

/// Processing capability for one sample representation.
pub trait Process<S>: Processor {
    /// Process one already-validated realtime block.
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, '_, S>);
}

/// Failure while constructing a durable [`InstanceRuntime`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstanceRuntimeError {
    /// The component's immutable parameter schema failed validation.
    InvalidParameters(ParameterStoreError),
}

impl fmt::Display for InstanceRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidParameters(error) => write!(formatter, "invalid parameters: {error}"),
        }
    }
}

impl std::error::Error for InstanceRuntimeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidParameters(error) => Some(error),
        }
    }
}

/// Framework activation failure before an active processor is published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivateError<E> {
    /// The instance already owns an active processor.
    AlreadyActive,
    /// The component schema no longer matches the instance schema.
    ParameterSchemaMismatch,
    /// The proposed whole-component I/O configuration failed structural validation.
    InvalidAudioIo(AudioIoConfigurationError),
    /// The component's immutable parameter schema failed validation.
    ///
    /// This variant remains for the temporary [`activate`] convenience path.
    /// [`InstanceRuntime::new`] validates the schema before activation instead.
    InvalidParameters(ParameterStoreError),
    /// Product activation failed after the framework configuration was validated.
    Product(E),
}

impl<E> fmt::Display for ActivateError<E>
where
    E: fmt::Display,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyActive => formatter.write_str("component instance is already active"),
            Self::ParameterSchemaMismatch => formatter
                .write_str("component parameter schema does not match its instance runtime"),
            Self::InvalidAudioIo(error) => write!(formatter, "invalid audio I/O: {error}"),
            Self::InvalidParameters(error) => write!(formatter, "invalid parameters: {error}"),
            Self::Product(error) => write!(formatter, "product activation failed: {error}"),
        }
    }
}

impl<E> std::error::Error for ActivateError<E>
where
    E: std::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::AlreadyActive | Self::ParameterSchemaMismatch => None,
            Self::InvalidAudioIo(error) => Some(error),
            Self::InvalidParameters(error) => Some(error),
            Self::Product(error) => Some(error),
        }
    }
}

/// Lifecycle operation attempted in the wrong instance phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstanceLifecycleError {
    /// No active processor exists for the requested operation.
    NotActive,
}

impl fmt::Display for InstanceLifecycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotActive => formatter.write_str("component instance is not active"),
        }
    }
}

impl std::error::Error for InstanceLifecycleError {}

/// Processing failure through a durable [`InstanceRuntime`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstanceProcessError {
    /// No processor is active.
    NotActive,
    /// The process block violated the active configuration or parameter schema.
    InvalidBlock(ProcessBlockError),
}

impl fmt::Display for InstanceProcessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotActive => formatter.write_str("component instance is not active"),
            Self::InvalidBlock(error) => write!(formatter, "invalid process block: {error}"),
        }
    }
}

impl std::error::Error for InstanceProcessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NotActive => None,
            Self::InvalidBlock(error) => Some(error),
        }
    }
}

/// Failure while replacing the durable instance's framework-managed parameter state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstanceStateError {
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
            Self::InvalidParameters(error) => Some(error),
            Self::ParameterState(error) => Some(error),
            Self::IncompleteParameterState { .. } => None,
        }
    }
}

/// Failure while decoding, migrating, or applying a complete parameter state.
#[derive(Debug)]
pub enum InstanceStateLoadError<E> {
    /// The encoded state was malformed or exceeded its bounds.
    Decode(StateDecodeError),
    /// Product-schema migration failed before live state was touched.
    Migration(StateMigrationError<E>),
    /// The migrated state failed current parameter validation.
    Apply(InstanceStateError),
}

impl<E> fmt::Display for InstanceStateLoadError<E>
where
    E: fmt::Display,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(error) => write!(formatter, "could not decode state: {error}"),
            Self::Migration(error) => write!(formatter, "could not migrate state: {error}"),
            Self::Apply(error) => write!(formatter, "could not apply state: {error}"),
        }
    }
}

impl<E> std::error::Error for InstanceStateLoadError<E>
where
    E: std::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Decode(error) => Some(error),
            Self::Migration(error) => Some(error),
            Self::Apply(error) => Some(error),
        }
    }
}

struct ActiveRuntime<P> {
    process: ProcessConfig,
    audio_ports: Vec<ConfiguredAudioPort>,
    processor: P,
}

impl<P> ActiveRuntime<P> {
    fn config(&self) -> ActivationConfig<'_> {
        ActivationConfig::new(self.process, AudioIoConfiguration::new(&self.audio_ports))
    }
}

/// Durable framework-owned runtime for one component instance.
///
/// The runtime owns canonical base parameter state across activation cycles and
/// optionally owns the active processor plus its accepted I/O configuration.
/// The component definition itself remains outside this object and is borrowed
/// only while activating, so deployment boundaries need to transfer only the
/// concrete processor/runtime state rather than requiring `Component: Send`.
///
/// The accepted port list is copied once while inactive. Dynamically negotiated
/// layouts therefore do not create self-referential lifetimes and require no
/// callback-time port allocation.
pub struct InstanceRuntime<P>
where
    P: Processor,
{
    parameters: ParameterStore,
    active: Option<ActiveRuntime<P>>,
}

impl<P> InstanceRuntime<P>
where
    P: Processor,
{
    /// Construct one inactive instance from a validated parameter schema.
    ///
    /// # Errors
    ///
    /// Returns [`InstanceRuntimeError::InvalidParameters`] if the immutable
    /// parameter schema is invalid.
    pub fn new(descriptors: &[ParameterDescriptor]) -> Result<Self, InstanceRuntimeError> {
        let parameters =
            ParameterStore::new(descriptors).map_err(InstanceRuntimeError::InvalidParameters)?;
        Ok(Self {
            parameters,
            active: None,
        })
    }

    /// Construct one inactive runtime from a component's immutable schema.
    ///
    /// # Errors
    ///
    /// Returns [`InstanceRuntimeError::InvalidParameters`] if the component's
    /// parameter schema is invalid.
    pub fn for_component<C>(component: &C) -> Result<Self, InstanceRuntimeError>
    where
        C: Component<Processor = P>,
    {
        Self::new(component.parameter_descriptors())
    }

    /// Return the durable current base/control parameter values.
    #[must_use]
    pub const fn parameters(&self) -> &ParameterStore {
        &self.parameters
    }

    /// Mutably access durable base/control values from the owning control context.
    ///
    /// Cross-thread adapters must synchronize before entering this single-owner
    /// semantic store; this method itself does not make the runtime `Sync`.
    pub fn parameters_mut(&mut self) -> &mut ParameterStore {
        &mut self.parameters
    }

    /// Return whether the instance currently owns an active processor.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active.is_some()
    }

    /// Return the current activation configuration when active.
    #[must_use]
    pub fn active_config(&self) -> Option<ActivationConfig<'_>> {
        self.active.as_ref().map(ActiveRuntime::config)
    }

    /// Activate this instance after validating and owning its accepted I/O layout.
    ///
    /// The component receives the runtime's current base parameter state during
    /// preparation, so state loaded before activation can affect precomputed DSP
    /// resources without moving persistent authority into the processor.
    ///
    /// # Errors
    ///
    /// Returns [`ActivateError::AlreadyActive`] if already active,
    /// [`ActivateError::ParameterSchemaMismatch`] if the supplied component no
    /// longer matches the instance schema, [`ActivateError::InvalidAudioIo`] for
    /// malformed configuration, or [`ActivateError::Product`] when product
    /// activation fails.
    pub fn activate<C>(
        &mut self,
        component: &C,
        process: ProcessConfig,
        audio_io: AudioIoConfiguration<'_>,
    ) -> Result<(), ActivateError<C::ActivationError>>
    where
        C: Component<Processor = P>,
    {
        if self.active.is_some() {
            return Err(ActivateError::AlreadyActive);
        }
        if self.parameters.descriptors() != component.parameter_descriptors() {
            return Err(ActivateError::ParameterSchemaMismatch);
        }
        audio_io
            .validate(component.audio_ports())
            .map_err(ActivateError::InvalidAudioIo)?;

        let audio_ports = audio_io.ports().to_vec();
        let config = ActivationConfig::new(process, AudioIoConfiguration::new(&audio_ports));
        let processor = component
            .activate_with_parameters(&config, &self.parameters)
            .map_err(ActivateError::Product)?;
        self.active = Some(ActiveRuntime {
            process,
            audio_ports,
            processor,
        });
        Ok(())
    }

    /// Reset transient DSP history while preserving durable base state.
    ///
    /// # Errors
    ///
    /// Returns [`InstanceLifecycleError::NotActive`] when inactive.
    pub fn reset(&mut self) -> Result<(), InstanceLifecycleError> {
        let active = self
            .active
            .as_mut()
            .ok_or(InstanceLifecycleError::NotActive)?;
        active.processor.reset();
        Ok(())
    }

    /// Process one block through the active processor.
    ///
    /// # Errors
    ///
    /// Returns [`InstanceProcessError::NotActive`] when inactive or
    /// [`InstanceProcessError::InvalidBlock`] before product DSP runs when the
    /// callback violates the active process/parameter contract.
    pub fn process<S>(
        &mut self,
        frame_count: u32,
        context: ProcessContext<'_>,
        buffers: &mut [ChannelBuffer<'_, S>],
    ) -> Result<(), InstanceProcessError>
    where
        P: Process<S>,
    {
        let Self { parameters, active } = self;
        let active = active.as_mut().ok_or(InstanceProcessError::NotActive)?;
        let ActiveRuntime {
            process,
            audio_ports,
            processor,
        } = active;
        let config = ActivationConfig::new(*process, AudioIoConfiguration::new(audio_ports));
        let mut block = ProcessBlock::new(&config, parameters, frame_count, context, buffers)
            .map_err(InstanceProcessError::InvalidBlock)?;
        parameters
            .validate_events(context.parameter_events())
            .map_err(ProcessBlockError::InvalidParameterEvents)
            .map_err(InstanceProcessError::InvalidBlock)?;
        processor.process(&mut block);
        Ok(())
    }

    /// Deactivate this instance while preserving durable base/control state.
    ///
    /// The processor is destroyed in the caller's domain only after the caller
    /// has ended all process/reset borrows. The owned inactive I/O configuration
    /// is dropped with the active resources and can be replaced on reactivation.
    ///
    /// # Errors
    ///
    /// Returns [`InstanceLifecycleError::NotActive`] when already inactive.
    pub fn deactivate(&mut self) -> Result<(), InstanceLifecycleError> {
        let active = self
            .active
            .take()
            .ok_or(InstanceLifecycleError::NotActive)?;
        drop(active.processor);
        Ok(())
    }

    /// Build a semantic document containing all durable parameter base values.
    ///
    /// # Errors
    ///
    /// Returns [`ParameterStateError`] if document construction fails.
    pub fn parameter_state_document(
        &self,
        product_id: impl Into<String>,
        product_schema: u32,
    ) -> Result<StateDocument, ParameterStateError> {
        self.parameters.state_document(product_id, product_schema)
    }

    /// Encode all durable parameter base values under explicit state bounds.
    ///
    /// # Errors
    ///
    /// Returns [`ParameterStateError`] if semantic document construction or
    /// encoding fails.
    pub fn encode_parameter_state(
        &self,
        product_id: impl Into<String>,
        product_schema: u32,
        limits: StateLimits,
    ) -> Result<Vec<u8>, ParameterStateError> {
        self.parameters
            .encode_state(product_id, product_schema, limits)
    }

    /// Replace durable parameter state transactionally from a complete document.
    ///
    /// A normal instance/project/preset load is a complete replacement. Custom
    /// non-parameter entries are ignored here for future product-state handling,
    /// but every framework-managed parameter must be present after migration.
    /// The current runtime remains unchanged on failure.
    ///
    /// # Errors
    ///
    /// Returns [`InstanceStateError`] for identity/schema/value errors, unknown
    /// parameter entries, or an incomplete parameter snapshot.
    pub fn apply_parameter_state_for_product(
        &mut self,
        document: &StateDocument,
        product_id: &str,
        product_schema: u32,
    ) -> Result<(), InstanceStateError> {
        let descriptors = self.parameters.descriptors().to_vec();
        let mut candidate =
            ParameterStore::new(&descriptors).map_err(InstanceStateError::InvalidParameters)?;
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

        self.parameters = candidate;
        Ok(())
    }

    /// Decode, migrate, and apply a complete parameter state transactionally.
    ///
    /// The encoded document and every migration result remain temporary until
    /// current-schema parameter validation succeeds. Custom entries are
    /// preserved by [`StateDocument::migrate_to`] for a future product-state
    /// owner, while this boundary publishes framework-managed parameters only.
    /// Decoding and migration are control/non-realtime operations.
    ///
    /// # Errors
    ///
    /// Returns [`InstanceStateLoadError`] when decoding, migration, or current
    /// schema validation fails. The runtime is unchanged on every failure.
    pub fn apply_parameter_state_bytes<M>(
        &mut self,
        bytes: &[u8],
        product_id: &str,
        product_schema: u32,
        limits: StateLimits,
        migrations: &[&M],
    ) -> Result<(), InstanceStateLoadError<M::Error>>
    where
        M: StateMigration + ?Sized,
        M::Error: std::error::Error + 'static,
    {
        let document = StateDocument::decode_with_limits(bytes, limits)
            .map_err(InstanceStateLoadError::Decode)?;
        let document = document
            .migrate_to(product_schema, migrations)
            .map_err(InstanceStateLoadError::Migration)?;
        self.apply_parameter_state_for_product(&document, product_id, product_schema)
            .map_err(InstanceStateLoadError::Apply)
    }
}

/// Temporary activation-local processor shell retained while adapters migrate.
///
/// Unlike [`InstanceRuntime`], this type recreates its base store for every
/// activation. It must therefore be treated as an active projection rather than
/// the durable instance authority. New format-independent clients should prefer
/// [`InstanceRuntime`].
pub struct Activated<'a, P> {
    config: ActivationConfig<'a>,
    processor: P,
    parameters: ParameterStore,
}

impl<'a, P> Activated<'a, P>
where
    P: Processor,
{
    /// Return the immutable activation configuration.
    #[must_use]
    pub const fn config(&self) -> ActivationConfig<'a> {
        self.config
    }

    /// Return the activation-local base parameter projection.
    #[must_use]
    pub const fn parameters(&self) -> &ParameterStore {
        &self.parameters
    }

    /// Mutably access the activation-local base projection.
    pub fn parameters_mut(&mut self) -> &mut ParameterStore {
        &mut self.parameters
    }

    /// Reset transient realtime processor history.
    pub fn reset(&mut self) {
        self.processor.reset();
    }

    /// Process one block after validating dimensions, automation, and context.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessBlockError`] before product DSP runs if the callback
    /// dimensions, event context, or parameter values violate the activation
    /// contract.
    pub fn process<S>(
        &mut self,
        frame_count: u32,
        context: ProcessContext<'_>,
        buffers: &mut [ChannelBuffer<'_, S>],
    ) -> Result<(), ProcessBlockError>
    where
        P: Process<S>,
    {
        let mut block = ProcessBlock::new(
            &self.config,
            &self.parameters,
            frame_count,
            context,
            buffers,
        )?;
        self.parameters
            .validate_events(context.parameter_events())
            .map_err(ProcessBlockError::InvalidParameterEvents)?;
        self.processor.process(&mut block);
        Ok(())
    }

    /// Consume the active lifecycle shell and destroy its processor in the caller's domain.
    pub fn deactivate(self) {
        let Self { processor, .. } = self;
        drop(processor);
    }
}

/// Temporary convenience activation path retained while adapters migrate to
/// [`InstanceRuntime`].
///
/// # Errors
///
/// Returns [`ActivateError::InvalidAudioIo`] before calling product activation
/// for malformed configurations, [`ActivateError::InvalidParameters`] for an
/// invalid component schema, or [`ActivateError::Product`] when the product
/// rejects activation.
pub fn activate<'a, C>(
    component: &C,
    process: ProcessConfig,
    audio_io: AudioIoConfiguration<'a>,
) -> Result<Activated<'a, C::Processor>, ActivateError<C::ActivationError>>
where
    C: Component,
{
    audio_io
        .validate(component.audio_ports())
        .map_err(ActivateError::InvalidAudioIo)?;

    let parameters = ParameterStore::new(component.parameter_descriptors())
        .map_err(ActivateError::InvalidParameters)?;
    let config = ActivationConfig::new(process, audio_io);
    let processor = component
        .activate_with_parameters(&config, &parameters)
        .map_err(ActivateError::Product)?;

    Ok(Activated {
        config,
        processor,
        parameters,
    })
}
