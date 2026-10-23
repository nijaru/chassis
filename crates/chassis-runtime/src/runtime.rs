//! Explicit component/processor lifecycle contracts.
//!
//! The framework-owned [`InstanceRuntime`] is the durable authority for one
//! component instance's immutable schema, base parameter state, semantic state,
//! and active lifecycle.

use core::fmt;
use std::vec::Vec;

use crate::{
    audio::{
        AudioEndpointError, AudioIoConfiguration, AudioIoConfigurationError, AudioIoPolicyError,
        AudioPortDescriptor, AudioPortIndex, ConfiguredAudioPort, PortKey,
        ResolvedAudioIoConfiguration,
    },
    automation::ParameterEvents,
    buffer::ChannelBuffer,
    events::{
        EventPortDescriptor, EventPortIndex, EventPortKey, EventPortSchemaError, NoteEventPortError,
    },
    parameters::{ParameterStore, ParameterStoreError, ParameterValuesMut},
    process::{
        ActivationConfig, ChannelBufferSlice, ProcessBlock, ProcessBlockError, ProcessBufferSource,
        ProcessConfig, ProcessContext,
    },
    schema::{ComponentSchema, ComponentSchemaError},
    state::{StateEntry, StateMigration},
};

mod audio_buffers;
mod semantic_state;

pub use semantic_state::{
    ComponentStateError, InstanceSemanticStateError, InstanceSemanticStateLoadError,
    InstanceStateError, InstanceStateExportError,
};

use audio_buffers::AudioBufferValidation;

/// Immutable product definition/factory for one Chassis component type.
///
/// The component is not the live mutable processor. [`Self::schema`] supplies
/// its coherent immutable definition; activation creates processor-owned DSP
/// history and resources for one accepted configuration.
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

    /// Default custom entries, combined with descriptor-owned parameter defaults.
    ///
    /// This is a non-realtime construction operation. Entries must be unique,
    /// valid, and outside the reserved `parameter/` namespace.
    fn default_custom_state(&self) -> Vec<StateEntry> {
        Vec::new()
    }

    /// Validate one complete parameter/custom candidate off the realtime thread.
    ///
    /// The default policy supports parameters only and rejects all custom fields.
    /// Products with custom state must check required fields, unknown fields, and
    /// cross-field invariants here. Defaults and reactivation are checked too.
    ///
    /// # Errors
    ///
    /// Returns a component policy error without publishing any candidate.
    fn validate_state(
        &self,
        _parameters: &ParameterStore,
        custom: &[StateEntry],
    ) -> Result<(), ComponentStateError> {
        if custom.is_empty() {
            Ok(())
        } else {
            Err(ComponentStateError::UnsupportedCustomState)
        }
    }

    /// Adjacent semantic migrations owned by this component, never by a loader.
    fn state_migrations(&self) -> &[&dyn StateMigration<Error = ComponentStateError>] {
        &[]
    }

    /// Prepare one processor from the current complete validated state.
    ///
    /// Allocation and resource preparation belong here, before processing.
    ///
    /// # Errors
    ///
    /// Returns the product error without publishing a partial activation.
    fn activate(
        &self,
        config: &ActivationConfig<'_>,
        parameters: &ParameterStore,
        custom_state: &[StateEntry],
    ) -> Result<Self::Processor, Self::ActivationError>;
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
#[derive(Debug)]
pub enum InstanceRuntimeError {
    /// The component's immutable audio-port schema failed validation.
    InvalidAudioPorts(AudioIoConfigurationError),
    /// The component's whole-component audio I/O policy failed validation.
    InvalidAudioIoPolicy(AudioIoPolicyError),
    /// The component's immutable parameter schema failed validation.
    InvalidParameters(ParameterStoreError),
    /// The component's immutable event-port schema failed validation.
    InvalidEventPorts(EventPortSchemaError),
    /// Default complete state failed framework or component validation.
    InvalidState(InstanceSemanticStateError),
}

fn runtime_schema_error(error: ComponentSchemaError) -> InstanceRuntimeError {
    match error {
        ComponentSchemaError::AudioPorts(error) => InstanceRuntimeError::InvalidAudioPorts(error),
        ComponentSchemaError::AudioIoPolicy(error) => {
            InstanceRuntimeError::InvalidAudioIoPolicy(error)
        }
        ComponentSchemaError::EventPorts(error) => InstanceRuntimeError::InvalidEventPorts(error),
        ComponentSchemaError::Parameters(error) => InstanceRuntimeError::InvalidParameters(error),
    }
}

impl fmt::Display for InstanceRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAudioPorts(error) => write!(formatter, "invalid audio ports: {error}"),
            Self::InvalidAudioIoPolicy(error) => {
                write!(formatter, "invalid audio I/O policy: {error}")
            }
            Self::InvalidParameters(error) => write!(formatter, "invalid parameters: {error}"),
            Self::InvalidEventPorts(error) => write!(formatter, "invalid event ports: {error}"),
            Self::InvalidState(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for InstanceRuntimeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidAudioPorts(error) => Some(error),
            Self::InvalidAudioIoPolicy(error) => Some(error),
            Self::InvalidParameters(error) => Some(error),
            Self::InvalidEventPorts(error) => Some(error),
            Self::InvalidState(error) => Some(error),
        }
    }
}

/// Framework activation failure before an active processor is published.
#[derive(Debug)]
pub enum ActivateError<E> {
    /// The instance already owns an active processor.
    AlreadyActive,
    /// The supplied component's schema itself is invalid.
    InvalidComponentSchema(ComponentSchemaError),
    /// The component's audio-port schema no longer matches the instance schema.
    AudioPortSchemaMismatch,
    /// The component whole-audio-I/O policy no longer matches the instance schema.
    AudioIoPolicyMismatch,
    /// The component parameter schema no longer matches the instance schema.
    ParameterSchemaMismatch,
    /// The component event-port schema no longer matches the instance schema.
    EventPortSchemaMismatch,
    /// The component semantic state identity/version no longer matches the instance schema.
    StateIdentityMismatch,
    /// The proposed whole-component I/O configuration failed structural validation.
    InvalidAudioIo(AudioIoConfigurationError),
    /// The structurally valid I/O configuration is not accepted by component policy.
    UnsupportedAudioIo,
    /// The accepted channel topology could not be represented or its validation scratch allocated.
    AudioChannelCapacity,
    /// Complete state no longer satisfies component policy after control edits.
    InvalidState(ComponentStateError),
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
            Self::AudioIoPolicyMismatch => formatter
                .write_str("component audio I/O policy does not match its instance runtime"),
            Self::ParameterSchemaMismatch => formatter
                .write_str("component parameter schema does not match its instance runtime"),
            Self::EventPortSchemaMismatch => formatter
                .write_str("component event-port schema does not match its instance runtime"),
            Self::StateIdentityMismatch => {
                formatter.write_str("component state identity does not match its instance runtime")
            }
            Self::InvalidAudioIo(error) => write!(formatter, "invalid audio I/O: {error}"),
            Self::UnsupportedAudioIo => {
                formatter.write_str("audio I/O configuration is unsupported by component policy")
            }
            Self::AudioChannelCapacity => {
                formatter.write_str("could not prepare audio channel validation capacity")
            }
            Self::InvalidState(error) => write!(formatter, "invalid activation state: {error}"),
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
            | Self::AudioIoPolicyMismatch
            | Self::ParameterSchemaMismatch
            | Self::EventPortSchemaMismatch
            | Self::StateIdentityMismatch
            | Self::UnsupportedAudioIo
            | Self::AudioChannelCapacity => None,
            Self::InvalidComponentSchema(error) => Some(error),
            Self::InvalidAudioIo(error) => Some(error),
            Self::InvalidState(error) => Some(error),
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

struct ActiveRuntime<P> {
    process: ProcessConfig,
    audio_ports: Vec<ConfiguredAudioPort>,
    resolved_audio: ResolvedAudioIoConfiguration,
    audio_buffers: AudioBufferValidation,
    latency: LatencySamples,
    frozen_parameters: Option<ParameterStore>,
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
/// itself remains outside this object and is borrowed for construction,
/// complete state admission, and activation.
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
    /// Construct one inactive runtime from a component's coherent immutable schema.
    ///
    /// # Errors
    ///
    /// Returns [`InstanceRuntimeError`] if the schema, custom defaults, or
    /// component validation of the combined defaults fails. Persistence identity
    /// is not required for construction.
    pub fn for_component<C>(component: &C) -> Result<Self, InstanceRuntimeError>
    where
        C: Component<Processor = P>,
    {
        let schema = component.schema().map_err(runtime_schema_error)?;
        let parameters = ParameterStore::new(schema.parameters())
            .map_err(InstanceRuntimeError::InvalidParameters)?;
        let mut runtime = Self {
            schema,
            parameters,
            custom_state: Vec::new(),
            active: None,
        };
        runtime
            .initialize_state(component)
            .map_err(InstanceRuntimeError::InvalidState)?;
        Ok(runtime)
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
    pub fn event_port_index(&self, key: &EventPortKey) -> Option<EventPortIndex> {
        self.schema.event_port_index(key)
    }

    /// Return durable desired base/control values.
    ///
    /// While custom-state reactivation is pending, DSP instead borrows the
    /// frozen pre-load projection; these values remain the saveable replacement.
    #[must_use]
    pub const fn parameters(&self) -> &ParameterStore {
        &self.parameters
    }

    /// Mutably access durable base/control values from the owning control context.
    ///
    /// Cross-thread adapters must synchronize before entering this single-owner
    /// semantic store; this method itself does not make the runtime `Sync`.
    /// This view checks descriptor domains, not component cross-field policy.
    /// Use complete state replacement for coupled non-realtime edits. During a
    /// pending custom-state restart, edits affect desired state, not frozen DSP.
    /// The returned view cannot replace the immutable schema:
    ///
    /// ```compile_fail
    /// use chassis_runtime::{parameters::ParameterStore, runtime::{Component, InstanceRuntime, Processor}};
    /// struct Dsp;
    /// impl Processor for Dsp {}
    /// struct Definition;
    /// impl Component for Definition {
    ///     type Processor = Dsp;
    ///     type ActivationError = core::convert::Infallible;
    ///     fn schema(&self) -> Result<chassis_runtime::schema::ComponentSchema, chassis_runtime::schema::ComponentSchemaError> {
    ///         chassis_runtime::schema::ComponentSchema::stereo_effect(vec![])
    ///     }
    ///     fn activate(&self, _: &chassis_runtime::process::ActivationConfig<'_>, _: &ParameterStore, _: &[chassis_runtime::state::StateEntry]) -> Result<Dsp, Self::ActivationError> { Ok(Dsp) }
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

    /// Whether active resources require replacement through a new activation.
    ///
    /// Accepted custom-state changes request restart immediately. This runtime
    /// request remains set until deactivation, even if later loads restore the
    /// previous state. The processor may independently request restart; current
    /// resources and the frozen DSP parameter projection remain valid until that
    /// transition. Incoming parameter events are checked but discarded meanwhile.
    #[must_use]
    pub fn restart_requested(&self) -> bool {
        self.active.as_ref().is_some_and(|active| {
            active.frozen_parameters.is_some() || active.processor.restart_requested()
        })
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
    /// [`ActivateError::AudioChannelCapacity`] if topology validation scratch
    /// cannot be prepared, [`ActivateError::InvalidState`] when complete component
    /// policy fails, or [`ActivateError::Product`] when product activation fails.
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
        if self.schema.audio_io_policy() != component_schema.audio_io_policy() {
            return Err(ActivateError::AudioIoPolicyMismatch);
        }
        if self.schema.parameters() != component_schema.parameters() {
            return Err(ActivateError::ParameterSchemaMismatch);
        }
        if self.schema.event_ports() != component_schema.event_ports() {
            return Err(ActivateError::EventPortSchemaMismatch);
        }

        component
            .validate_state(&self.parameters, &self.custom_state)
            .map_err(ActivateError::InvalidState)?;

        let resolved_audio = audio_io
            .resolve(self.schema.audio_ports())
            .map_err(ActivateError::InvalidAudioIo)?;
        if !self.schema.audio_io_policy().accepts(audio_io) {
            return Err(ActivateError::UnsupportedAudioIo);
        }
        let audio_buffers = AudioBufferValidation::new(&resolved_audio)
            .map_err(|()| ActivateError::AudioChannelCapacity)?;
        let audio_ports = audio_io.ports().to_vec();
        let config = ActivationConfig::new(
            process,
            AudioIoConfiguration::new(&audio_ports),
            &self.schema,
        );
        let processor = component
            .activate(&config, &self.parameters, &self.custom_state)
            .map_err(ActivateError::Product)?;
        let latency = processor.latency();
        self.active = Some(ActiveRuntime {
            process,
            audio_ports,
            resolved_audio,
            audio_buffers,
            latency,
            frozen_parameters: None,
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
    /// This slice entry point wraps the slice in [`ChannelBufferSlice`]
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
    /// The runtime validates unique, complete endpoint coverage and
    /// identity/direction/channel legality against the activation-owned dense
    /// audio configuration before product DSP. Source-
    /// specific raw-pointer/alias proofs still belong to adapters/embeddings.
    /// Outside the pending custom-state gate, successful processing retains final
    /// automation endpoints for the next block, state export, and reactivation.
    /// While gated, DSP uses frozen base values and no parameter trajectories;
    /// incoming events are still validated but never retained. Validation
    /// failure or a processor panic retains no endpoints. Control edits require
    /// exclusive runtime access; a subsequent edit overrides completed automation.
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
            audio_buffers,
            latency: _,
            frozen_parameters,
            processor,
        } = active;

        ProcessBlock::<S, B>::validate_context(*process, frame_count, context)
            .map_err(InstanceProcessError::InvalidBlock)?;
        audio_buffers
            .validate(source, resolved_audio)
            .map_err(InstanceProcessError::InvalidAudioEndpoint)?;

        let config =
            ActivationConfig::new(*process, AudioIoConfiguration::new(audio_ports), schema);
        parameters
            .validate_events(context.parameter_events())
            .map_err(ProcessBlockError::InvalidParameterEvents)
            .map_err(InstanceProcessError::InvalidBlock)?;
        context
            .note_events()
            .validate_ports(schema.event_ports())
            .map_err(InstanceProcessError::InvalidNoteEvents)?;
        // While custom resources are stale, even otherwise-valid automation can
        // refer to the replacement resources. Keep the first pre-load base state
        // and withhold trajectories; do not let old DSP complete into pending state.
        let dsp_context = if frozen_parameters.is_some() {
            ProcessContext::with_note_events(
                context.mode(),
                context.transport(),
                ParameterEvents::empty_for_block(frame_count),
                context.note_events(),
            )
        } else {
            context
        };
        let dsp_parameters = frozen_parameters.as_ref().unwrap_or(parameters);
        let mut block =
            ProcessBlock::new(&config, dsp_parameters, frame_count, dsp_context, source)
                .map_err(InstanceProcessError::InvalidBlock)?;
        processor.process(&mut block);
        if frozen_parameters.is_none() {
            parameters.retain_automation_endpoints(context.parameter_events());
        }
        Ok(())
    }

    /// Deactivate this instance while preserving durable base/control state.
    ///
    /// The processor is destroyed in the caller's domain only after the caller
    /// has ended all process/reset borrows. The owned inactive I/O configuration
    /// activation-scoped latency, and frozen parameter projection are dropped
    /// with the active resources and can be replaced on reactivation.
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
}
