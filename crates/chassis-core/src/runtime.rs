//! Explicit component/processor lifecycle contracts.
//!
//! The framework-owned [`InstanceRuntime`] is the durable authority for one
//! component instance's immutable schema, base parameter state, semantic state,
//! and active lifecycle.

use core::fmt;
use std::vec::Vec;

use crate::{
    audio::{
        AudioEndpointError, AudioIoConfiguration, AudioIoConfigurationError, AudioPortDescriptor,
        AudioPortIndex, ConfiguredAudioPort, PortDirection, PortKey, ResolvedAudioIoConfiguration,
    },
    buffer::ChannelBuffer,
    events::{
        EventPortDescriptor, EventPortIndex, EventPortKey, EventPortSchemaError, NoteEventPortError,
    },
    parameters::{ParameterStateError, ParameterStore, ParameterStoreError, ParameterValuesMut},
    process::{
        ActivationConfig, ChannelBufferSlice, ProcessBlock, ProcessBlockError, ProcessBufferSource,
        ProcessChannel, ProcessConfig, ProcessContext,
    },
    schema::{ComponentSchema, ComponentSchemaError},
    state::{
        StateDecodeError, StateDocument, StateEntry, StateLimits, StateMigration,
        StateMigrationError,
    },
};

/// Immutable product definition/factory for one Chassis component type.
///
/// The component is not the live mutable processor. During the current pre-alpha
/// migration, the separate audio/event/parameter accessors remain as authoring
/// compatibility hooks and [`Self::schema`] folds them into one coherent schema.
/// The target API makes that coherent schema the direct component authority.
pub trait Component {
    /// Realtime processor created for one successful activation.
    type Processor: Processor;

    /// Product-specific activation failure.
    type ActivationError;

    /// Build one coherent immutable component schema for this generation.
    ///
    /// Schema construction is a setup/control-domain operation. The runtime
    /// validates and owns the returned snapshot; activation receives that exact
    /// frozen generation through [`ActivationConfig::schema`].
    ///
    /// # Errors
    ///
    /// Returns [`ComponentSchemaError`] when any immutable schema is invalid.
    fn schema(&self) -> Result<ComponentSchema, ComponentSchemaError>;

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

    /// Create the realtime processor with the current complete semantic state.
    ///
    /// Custom entries are the canonical non-`parameter/` entries retained by
    /// the durable instance. The default delegates to
    /// [`Self::activate_with_parameters`] so products that only use framework
    /// parameters remain simple. Products that derive activation-time resources
    /// from custom persistent fields may override this hook.
    ///
    /// # Errors
    ///
    /// Returns the same product-defined activation error as [`Self::activate`].
    fn activate_with_state(
        &self,
        config: &ActivationConfig<'_>,
        parameters: &ParameterStore,
        custom_state: &[StateEntry],
    ) -> Result<Self::Processor, Self::ActivationError> {
        let _ = custom_state;
        self.activate_with_parameters(config, parameters)
    }
}

/// Processing latency reported by one successfully activated processor.
///
/// Latency is measured in samples at the active sample rate. The runtime
/// snapshots this value during activation so host-visible latency cannot drift
/// while the same activation remains live.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LatencySamples(u32);

impl LatencySamples {
    /// No processing latency.
    pub const ZERO: Self = Self(0);

    /// Construct a latency value from a sample count.
    #[must_use]
    pub const fn new(samples: u32) -> Self {
        Self(samples)
    }

    /// Return the latency in samples.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
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

    /// Request a new activation after processing observes an incompatible control change.
    ///
    /// Keep the current activation's resources and latency intact until deactivation.
    /// Deployment adapters inspect this after a completed block and publish its
    /// parameter endpoints before requesting a host restart. Control or state changes
    /// received between blocks are observed on the next process call. This request
    /// does not guarantee that the host will restart immediately.
    #[must_use]
    fn restart_requested(&self) -> bool {
        false
    }

    /// Return the latency established by this activation.
    ///
    /// Products that allocate lookahead, oversampling, convolution, or other
    /// delayed processing resources during activation override this method.
    /// Chassis snapshots the returned value when activation succeeds; changing
    /// processor internals later does not change the active runtime's latency.
    #[must_use]
    fn latency(&self) -> LatencySamples {
        LatencySamples::ZERO
    }
}

/// Processing capability for one sample representation.
pub trait Process<S>: Processor {
    /// Process one already-validated realtime block from any safe buffer source.
    fn process<B>(&mut self, block: &mut ProcessBlock<'_, '_, '_, S, B>)
    where
        B: ProcessBufferSource<S> + ?Sized;
}

/// Failure while constructing a durable [`InstanceRuntime`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstanceRuntimeError {
    /// The component's immutable audio-port schema failed validation.
    InvalidAudioPorts(AudioIoConfigurationError),
    /// The component's immutable parameter schema failed validation.
    InvalidParameters(ParameterStoreError),
    /// The component's immutable event-port schema failed validation.
    InvalidEventPorts(EventPortSchemaError),
}

fn runtime_schema_error(error: ComponentSchemaError) -> InstanceRuntimeError {
    match error {
        ComponentSchemaError::AudioPorts(error) => InstanceRuntimeError::InvalidAudioPorts(error),
        ComponentSchemaError::EventPorts(error) => InstanceRuntimeError::InvalidEventPorts(error),
        ComponentSchemaError::Parameters(error) => InstanceRuntimeError::InvalidParameters(error),
    }
}

impl fmt::Display for InstanceRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAudioPorts(error) => write!(formatter, "invalid audio ports: {error}"),
            Self::InvalidParameters(error) => write!(formatter, "invalid parameters: {error}"),
            Self::InvalidEventPorts(error) => write!(formatter, "invalid event ports: {error}"),
        }
    }
}

impl std::error::Error for InstanceRuntimeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidAudioPorts(error) => Some(error),
            Self::InvalidParameters(error) => Some(error),
            Self::InvalidEventPorts(error) => Some(error),
        }
    }
}

/// Framework activation failure before an active processor is published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivateError<E> {
    /// The instance already owns an active processor.
    AlreadyActive,
    /// The supplied component's schema itself is invalid.
    InvalidComponentSchema(ComponentSchemaError),
    /// The component's audio-port schema no longer matches the instance schema.
    AudioPortSchemaMismatch,
    /// The component parameter schema no longer matches the instance schema.
    ParameterSchemaMismatch,
    /// The component event-port schema no longer matches the instance schema.
    EventPortSchemaMismatch,
    /// The component semantic state identity/version no longer matches the instance schema.
    StateIdentityMismatch,
    /// The proposed whole-component I/O configuration failed structural validation.
    InvalidAudioIo(AudioIoConfigurationError),
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
            Self::InvalidComponentSchema(error) => {
                write!(formatter, "component schema is invalid: {error}")
            }
            Self::AudioPortSchemaMismatch => formatter
                .write_str("component audio-port schema does not match its instance runtime"),
            Self::ParameterSchemaMismatch => formatter
                .write_str("component parameter schema does not match its instance runtime"),
            Self::EventPortSchemaMismatch => formatter
                .write_str("component event-port schema does not match its instance runtime"),
            Self::StateIdentityMismatch => {
                formatter.write_str("component state identity does not match its instance runtime")
            }
            Self::InvalidAudioIo(error) => write!(formatter, "invalid audio I/O: {error}"),
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
            Self::AlreadyActive
            | Self::AudioPortSchemaMismatch
            | Self::ParameterSchemaMismatch
            | Self::EventPortSchemaMismatch
            | Self::StateIdentityMismatch => None,
            Self::InvalidComponentSchema(error) => Some(error),
            Self::InvalidAudioIo(error) => Some(error),
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
    /// A process-time audio endpoint is illegal for this activation.
    InvalidAudioEndpoint(AudioEndpointError),
    /// The process block violated the active configuration or parameter schema.
    InvalidBlock(ProcessBlockError),
    /// Semantic note events targeted invalid event-port capabilities.
    InvalidNoteEvents(NoteEventPortError),
}

impl fmt::Display for InstanceProcessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotActive => formatter.write_str("component instance is not active"),
            Self::InvalidAudioEndpoint(error) => {
                write!(formatter, "invalid audio endpoint: {error}")
            }
            Self::InvalidBlock(error) => write!(formatter, "invalid process block: {error}"),
            Self::InvalidNoteEvents(error) => write!(formatter, "invalid note events: {error}"),
        }
    }
}

impl std::error::Error for InstanceProcessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NotActive => None,
            Self::InvalidAudioEndpoint(error) => Some(error),
            Self::InvalidBlock(error) => Some(error),
            Self::InvalidNoteEvents(error) => Some(error),
        }
    }
}

/// Failure while exporting complete semantic state through schema-owned identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstanceStateExportError {
    /// This transitional component schema does not yet own semantic state identity.
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
    /// The component schema has not yet been migrated to semantic state identity.
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

/// Failure while validating a complete semantic instance state.
#[derive(Debug)]
pub enum InstanceSemanticStateError<E> {
    /// Framework-managed parameter validation failed.
    Parameters(InstanceStateError),
    /// Product-defined validation of the complete candidate state failed.
    Product(E),
}

impl<E> fmt::Display for InstanceSemanticStateError<E>
where
    E: fmt::Display,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parameters(error) => write!(formatter, "invalid parameter state: {error}"),
            Self::Product(error) => write!(formatter, "invalid product state: {error}"),
        }
    }
}

impl<E> std::error::Error for InstanceSemanticStateError<E>
where
    E: std::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parameters(error) => Some(error),
            Self::Product(error) => Some(error),
        }
    }
}

/// Failure while decoding, migrating, or applying a complete semantic state.
#[derive(Debug)]
pub enum InstanceSemanticStateLoadError<M, E> {
    /// The encoded state was malformed or exceeded its bounds.
    Decode(StateDecodeError),
    /// Product-schema migration failed before live state was touched.
    Migration(StateMigrationError<M>),
    /// The migrated complete state failed framework or product validation.
    Apply(InstanceSemanticStateError<E>),
}

impl<M, E> fmt::Display for InstanceSemanticStateLoadError<M, E>
where
    M: fmt::Display,
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

impl<M, E> std::error::Error for InstanceSemanticStateLoadError<M, E>
where
    M: std::error::Error + 'static,
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
    resolved_audio: ResolvedAudioIoConfiguration,
    latency: LatencySamples,
    processor: P,
}

impl<P> ActiveRuntime<P> {
    fn config<'a>(&'a self, schema: &'a ComponentSchema) -> ActivationConfig<'a> {
        ActivationConfig::new(
            self.process,
            AudioIoConfiguration::new(&self.audio_ports),
            schema,
        )
    }
}

/// Durable framework-owned runtime for one component instance.
///
/// The runtime owns one validated immutable [`ComponentSchema`], canonical base
/// parameter state derived from that schema, validated custom semantic state,
/// and optionally an active processor/configuration. The component definition
/// itself remains outside this object and is borrowed only while activating.
pub struct InstanceRuntime<P>
where
    P: Processor,
{
    schema: ComponentSchema,
    parameters: ParameterStore,
    custom_state: Vec<StateEntry>,
    active: Option<ActiveRuntime<P>>,
}

impl<P> InstanceRuntime<P>
where
    P: Processor,
{
    /// Construct one inactive runtime from a complete validated component schema.
    ///
    /// # Errors
    ///
    /// Returns [`InstanceRuntimeError`] only if rebuilding the parameter base
    /// store from an already validated schema unexpectedly fails.
    pub fn from_schema(schema: ComponentSchema) -> Result<Self, InstanceRuntimeError> {
        let parameters = ParameterStore::new(schema.parameters())
            .map_err(InstanceRuntimeError::InvalidParameters)?;
        Ok(Self {
            schema,
            parameters,
            custom_state: Vec::new(),
            active: None,
        })
    }

    /// Construct one inactive runtime from a component's coherent immutable schema.
    ///
    /// # Errors
    ///
    /// Returns [`InstanceRuntimeError`] if the component schema is invalid.
    pub fn for_component<C>(component: &C) -> Result<Self, InstanceRuntimeError>
    where
        C: Component<Processor = P>,
    {
        let schema = component.schema().map_err(runtime_schema_error)?;
        Self::from_schema(schema)
    }

    /// Return the immutable validated component schema owned by this instance.
    #[must_use]
    pub const fn schema(&self) -> &ComponentSchema {
        &self.schema
    }

    /// Return the immutable validated audio-port schema.
    #[must_use]
    pub fn audio_ports(&self) -> &[AudioPortDescriptor] {
        self.schema.audio_ports()
    }

    /// Resolve a stable audio-port key to its dense schema-local index.
    #[must_use]
    pub fn audio_port_index(&self, key: &PortKey) -> Option<AudioPortIndex> {
        self.schema.audio_port_index(key)
    }

    /// Return the immutable validated event-port schema.
    #[must_use]
    pub fn event_ports(&self) -> &[EventPortDescriptor] {
        self.schema.event_ports()
    }

    /// Resolve a stable event-port key to the schema-local dense index.
    #[must_use]
    pub fn event_port_index(&self, key: EventPortKey) -> Option<EventPortIndex> {
        self.schema.event_port_index(key)
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
    /// The returned view cannot replace the immutable schema:
    ///
    /// ```compile_fail
    /// use chassis_core::{parameters::ParameterStore, runtime::{Component, InstanceRuntime, Processor}};
    /// struct Dsp;
    /// impl Processor for Dsp {}
    /// struct Definition;
    /// impl Component for Definition {
    ///     type Processor = Dsp;
    ///     type ActivationError = core::convert::Infallible;
    ///     fn activate(&self, _: &chassis_core::process::ActivationConfig<'_>) -> Result<Dsp, Self::ActivationError> { Ok(Dsp) }
    /// }
    /// let definition = Definition;
    /// let mut runtime = InstanceRuntime::<Dsp>::for_component(&definition).unwrap();
    /// *runtime.parameters_mut() = ParameterStore::new(&[]).unwrap();
    /// ```
    pub fn parameters_mut(&mut self) -> ParameterValuesMut<'_> {
        self.parameters.values_mut()
    }

    /// Return the durable validated non-parameter semantic state entries.
    ///
    /// Entries are kept in canonical key order. Mutation is intentionally not
    /// exposed as an unrestricted slice; complete state replacement validates a
    /// candidate parameter/custom generation before publishing either half.
    #[must_use]
    pub fn custom_state(&self) -> &[StateEntry] {
        &self.custom_state
    }

    /// Return whether the instance currently owns an active processor.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active.is_some()
    }

    /// Return the current activation configuration when active.
    #[must_use]
    pub fn active_config(&self) -> Option<ActivationConfig<'_>> {
        self.active
            .as_ref()
            .map(|active| active.config(&self.schema))
    }

    /// Return the activation-owned dense audio configuration when active.
    #[must_use]
    pub fn active_audio(&self) -> Option<&ResolvedAudioIoConfiguration> {
        self.active.as_ref().map(|active| &active.resolved_audio)
    }

    /// Return the latency captured for the current activation.
    #[must_use]
    pub fn active_latency(&self) -> Option<LatencySamples> {
        self.active.as_ref().map(|active| active.latency)
    }

    /// Whether the active processor requests replacement through a new activation.
    #[must_use]
    pub fn restart_requested(&self) -> bool {
        self.active
            .as_ref()
            .is_some_and(|active| active.processor.restart_requested())
    }

    /// Activate this instance after validating the component generation and accepted I/O layout.
    ///
    /// The component receives the runtime's current complete validated semantic
    /// state during preparation, so state loaded before activation can affect
    /// precomputed DSP resources without moving persistent authority into the
    /// processor. The processor's reported latency is captured after successful
    /// construction and remains fixed for the lifetime of this activation.
    ///
    /// # Errors
    ///
    /// Returns [`ActivateError::AlreadyActive`] if already active, a schema
    /// mismatch/error if the supplied component no longer describes this instance
    /// generation, [`ActivateError::InvalidAudioIo`] for malformed configuration,
    /// or [`ActivateError::Product`] when product activation fails.
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

        let component_schema = component
            .schema()
            .map_err(ActivateError::InvalidComponentSchema)?;
        if self.schema.state_identity() != component_schema.state_identity() {
            return Err(ActivateError::StateIdentityMismatch);
        }
        if self.schema.audio_ports() != component_schema.audio_ports() {
            return Err(ActivateError::AudioPortSchemaMismatch);
        }
        if self.schema.parameters() != component_schema.parameters() {
            return Err(ActivateError::ParameterSchemaMismatch);
        }
        if self.schema.event_ports() != component_schema.event_ports() {
            return Err(ActivateError::EventPortSchemaMismatch);
        }

        let resolved_audio = audio_io
            .resolve(self.schema.audio_ports())
            .map_err(ActivateError::InvalidAudioIo)?;
        let audio_ports = audio_io.ports().to_vec();
        let config = ActivationConfig::new(
            process,
            AudioIoConfiguration::new(&audio_ports),
            &self.schema,
        );
        let processor = component
            .activate_with_state(&config, &self.parameters, &self.custom_state)
            .map_err(ActivateError::Product)?;
        let latency = processor.latency();
        self.active = Some(ActiveRuntime {
            process,
            audio_ports,
            resolved_audio,
            latency,
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

    /// Process one materialized channel slice through the active processor.
    ///
    /// This compatibility entry point wraps the slice in [`ChannelBufferSlice`]
    /// and delegates to [`Self::process_source`]. It performs no allocation.
    ///
    /// # Errors
    ///
    /// Returns [`InstanceProcessError::NotActive`] when inactive or a validated
    /// process-boundary error before product DSP runs.
    pub fn process<S>(
        &mut self,
        frame_count: u32,
        context: ProcessContext<'_>,
        buffers: &mut [ChannelBuffer<'_, S>],
    ) -> Result<(), InstanceProcessError>
    where
        P: Process<S>,
    {
        let mut source = ChannelBufferSlice::new(buffers);
        self.process_source(frame_count, context, &mut source)
    }

    /// Process one allocation-free safe buffer source through the active processor.
    ///
    /// The runtime validates endpoint identity/direction/channel legality against
    /// the activation-owned dense audio configuration before product DSP. Source-
    /// specific raw-pointer/alias proofs still belong to adapters/embeddings.
    ///
    /// # Errors
    ///
    /// Returns [`InstanceProcessError::NotActive`] when inactive,
    /// [`InstanceProcessError::InvalidAudioEndpoint`] for endpoint/configuration
    /// mismatches, [`InstanceProcessError::InvalidBlock`] for callback
    /// shape/parameter failures, or [`InstanceProcessError::InvalidNoteEvents`]
    /// for incompatible note-event port capabilities.
    pub fn process_source<S, B>(
        &mut self,
        frame_count: u32,
        context: ProcessContext<'_>,
        source: &mut B,
    ) -> Result<(), InstanceProcessError>
    where
        P: Process<S>,
        B: ProcessBufferSource<S> + ?Sized,
    {
        let Self {
            schema,
            parameters,
            custom_state: _,
            active,
        } = self;
        let active = active.as_mut().ok_or(InstanceProcessError::NotActive)?;
        let ActiveRuntime {
            process,
            audio_ports,
            resolved_audio,
            latency: _,
            processor,
        } = active;

        validate_audio_endpoints(source, resolved_audio)
            .map_err(InstanceProcessError::InvalidAudioEndpoint)?;

        let config =
            ActivationConfig::new(*process, AudioIoConfiguration::new(audio_ports), schema);
        let mut block = ProcessBlock::new(&config, parameters, frame_count, context, source)
            .map_err(InstanceProcessError::InvalidBlock)?;
        parameters
            .validate_events(context.parameter_events())
            .map_err(ProcessBlockError::InvalidParameterEvents)
            .map_err(InstanceProcessError::InvalidBlock)?;
        context
            .note_events()
            .validate_ports(schema.event_ports())
            .map_err(InstanceProcessError::InvalidNoteEvents)?;
        processor.process(&mut block);
        Ok(())
    }

    /// Deactivate this instance while preserving durable base/control state.
    ///
    /// The processor is destroyed in the caller's domain only after the caller
    /// has ended all process/reset borrows. The owned inactive I/O configuration
    /// and activation-scoped latency are dropped with the active resources and
    /// can be replaced on reactivation.
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

    fn semantic_state_identity(&self) -> Option<(String, u32)> {
        self.schema.state_identity().map(|identity| {
            (
                identity.component().as_str().to_owned(),
                identity.schema().get(),
            )
        })
    }

    /// Build a semantic document containing all durable parameter base values.
    ///
    /// This compatibility helper excludes custom product state and still accepts
    /// explicit identity. Complete instance state should use [`Self::state_document`].
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
    /// This compatibility helper excludes custom product state and still accepts
    /// explicit identity. Complete instance state should use [`Self::encode_state`].
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

    /// Build the complete durable semantic state using schema-owned identity.
    ///
    /// Framework-managed parameters are emitted from the canonical parameter
    /// store and validated custom entries are appended. Encoding remains
    /// deterministic because [`StateDocument`] canonicalizes entry order.
    ///
    /// # Errors
    ///
    /// Returns [`InstanceStateExportError::MissingStateIdentity`] while a
    /// transitional component still uses an unidentified schema, or wraps a
    /// parameter/state document error.
    pub fn state_document(&self) -> Result<StateDocument, InstanceStateExportError> {
        let (product_id, product_schema) = self
            .semantic_state_identity()
            .ok_or(InstanceStateExportError::MissingStateIdentity)?;
        self.state_document_for_product(product_id, product_schema)
            .map_err(InstanceStateExportError::State)
    }

    /// Historical explicit-identity complete-state export.
    ///
    /// This remains only while deployment adapters migrate to schema-owned
    /// semantic state identity.
    ///
    /// # Errors
    ///
    /// Returns [`ParameterStateError`] if document construction fails.
    pub fn state_document_for_product(
        &self,
        product_id: impl Into<String>,
        product_schema: u32,
    ) -> Result<StateDocument, ParameterStateError> {
        let mut document = self.parameters.state_document(product_id, product_schema)?;
        for entry in &self.custom_state {
            document
                .insert(entry.clone())
                .map_err(ParameterStateError::Document)?;
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

    /// Historical explicit-identity complete-state encoding.
    ///
    /// # Errors
    ///
    /// Returns [`ParameterStateError`] if document construction or encoding fails.
    pub fn encode_state_for_product(
        &self,
        product_id: impl Into<String>,
        product_schema: u32,
        limits: StateLimits,
    ) -> Result<Vec<u8>, ParameterStateError> {
        self.state_document_for_product(product_id, product_schema)?
            .encode_with_limits(limits)
            .map_err(ParameterStateError::Encode)
    }

    fn parameter_state_candidate(
        &self,
        document: &StateDocument,
        product_id: &str,
        product_schema: u32,
    ) -> Result<ParameterStore, InstanceStateError> {
        let descriptors = self.schema.parameters().to_vec();
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
        Ok(candidate)
    }

    /// Replace durable parameter state transactionally from a complete document.
    ///
    /// This compatibility path publishes framework-managed parameters only and
    /// leaves the current custom product state unchanged. Complete semantic state
    /// should use [`Self::apply_state`].
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
        let candidate = self.parameter_state_candidate(document, product_id, product_schema)?;
        self.parameters = candidate;
        Ok(())
    }

    /// Replace complete semantic instance state using schema-owned identity.
    ///
    /// # Errors
    ///
    /// Returns [`InstanceSemanticStateError`] for missing identity, framework
    /// state validation failures, or product validation failure. Publication is
    /// atomic across parameter and custom state.
    pub fn apply_state<E, F>(
        &mut self,
        document: &StateDocument,
        validate_product_state: F,
    ) -> Result<(), InstanceSemanticStateError<E>>
    where
        F: FnOnce(&ParameterStore, &[StateEntry]) -> Result<(), E>,
    {
        let (product_id, product_schema) =
            self.semantic_state_identity()
                .ok_or(InstanceSemanticStateError::Parameters(
                    InstanceStateError::MissingStateIdentity,
                ))?;
        self.apply_state_for_product(
            document,
            &product_id,
            product_schema,
            validate_product_state,
        )
    }

    /// Historical explicit-identity complete semantic-state replacement.
    ///
    /// Framework parameters are validated into a temporary store. All
    /// non-`parameter/` entries are copied into a canonical temporary custom
    /// state projection. `validate_product_state` then receives both candidates
    /// so products can validate cross-field invariants. Neither candidate is
    /// published until every framework and product check succeeds.
    ///
    /// # Errors
    ///
    /// Returns [`InstanceSemanticStateError`] on framework or product rejection.
    pub fn apply_state_for_product<E, F>(
        &mut self,
        document: &StateDocument,
        product_id: &str,
        product_schema: u32,
        validate_product_state: F,
    ) -> Result<(), InstanceSemanticStateError<E>>
    where
        F: FnOnce(&ParameterStore, &[StateEntry]) -> Result<(), E>,
    {
        let candidate_parameters = self
            .parameter_state_candidate(document, product_id, product_schema)
            .map_err(InstanceSemanticStateError::Parameters)?;
        let mut candidate_custom: Vec<_> = document
            .entries()
            .iter()
            .filter(|entry| !entry.key().starts_with("parameter/"))
            .cloned()
            .collect();
        candidate_custom
            .sort_unstable_by(|left, right| left.key().as_bytes().cmp(right.key().as_bytes()));
        validate_product_state(&candidate_parameters, &candidate_custom)
            .map_err(InstanceSemanticStateError::Product)?;

        self.parameters = candidate_parameters;
        self.custom_state = candidate_custom;
        Ok(())
    }

    /// Decode, migrate, and apply a complete parameter state transactionally.
    ///
    /// This compatibility path publishes framework-managed parameters only.
    /// Custom entries remain unchanged even though migration preserves them.
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

    /// Decode, migrate, validate, and publish complete semantic state using
    /// schema-owned identity and schema version.
    ///
    /// # Errors
    ///
    /// Returns [`InstanceSemanticStateLoadError`] for missing state identity,
    /// bounded decode failures, migration failures, framework validation, or
    /// product validation. The runtime is unchanged on failure.
    pub fn apply_state_bytes<M, E, F>(
        &mut self,
        bytes: &[u8],
        limits: StateLimits,
        migrations: &[&M],
        validate_product_state: F,
    ) -> Result<(), InstanceSemanticStateLoadError<M::Error, E>>
    where
        M: StateMigration + ?Sized,
        M::Error: std::error::Error + 'static,
        E: std::error::Error + 'static,
        F: FnOnce(&ParameterStore, &[StateEntry]) -> Result<(), E>,
    {
        let (product_id, product_schema) =
            self.semantic_state_identity()
                .ok_or(InstanceSemanticStateLoadError::Apply(
                    InstanceSemanticStateError::Parameters(
                        InstanceStateError::MissingStateIdentity,
                    ),
                ))?;
        self.apply_state_bytes_for_product(
            bytes,
            &product_id,
            product_schema,
            limits,
            migrations,
            validate_product_state,
        )
    }

    /// Historical explicit-identity decoded semantic-state replacement.
    ///
    /// # Errors
    ///
    /// Returns [`InstanceSemanticStateLoadError`] for bounded decode failures,
    /// migration failures, framework parameter failures, or product-defined
    /// semantic validation failures.
    pub fn apply_state_bytes_for_product<M, E, F>(
        &mut self,
        bytes: &[u8],
        product_id: &str,
        product_schema: u32,
        limits: StateLimits,
        migrations: &[&M],
        validate_product_state: F,
    ) -> Result<(), InstanceSemanticStateLoadError<M::Error, E>>
    where
        M: StateMigration + ?Sized,
        M::Error: std::error::Error + 'static,
        E: std::error::Error + 'static,
        F: FnOnce(&ParameterStore, &[StateEntry]) -> Result<(), E>,
    {
        let document = StateDocument::decode_with_limits(bytes, limits)
            .map_err(InstanceSemanticStateLoadError::Decode)?;
        let document = document
            .migrate_to(product_schema, migrations)
            .map_err(InstanceSemanticStateLoadError::Migration)?;
        self.apply_state_for_product(
            &document,
            product_id,
            product_schema,
            validate_product_state,
        )
        .map_err(InstanceSemanticStateLoadError::Apply)
    }
}

fn validate_audio_endpoints<S, B>(
    source: &mut B,
    resolved: &ResolvedAudioIoConfiguration,
) -> Result<(), AudioEndpointError>
where
    B: ProcessBufferSource<S> + ?Sized,
{
    for channel in source.channels() {
        if let Some(endpoint) = channel.input_endpoint() {
            let port = endpoint.port_index();
            resolved.validate_endpoint(port, endpoint.channel(), PortDirection::Input)?;
        }
        if let Some(endpoint) = channel.output_endpoint() {
            let port = endpoint.port_index();
            resolved.validate_endpoint(port, endpoint.channel(), PortDirection::Output)?;
        }
    }
    Ok(())
}
