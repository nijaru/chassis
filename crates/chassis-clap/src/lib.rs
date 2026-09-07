//! Initial CLAP deployment adapter for Chassis.
//!
//! This adapter projects explicit stable audio/parameter IDs and a bounded scalar
//! parameter publication into CLAP. Process-time audio is driven by the validated
//! setup mapping rather than a hard-coded stereo buffer shape. The audio path
//! supports f32 exports and explicit f64-capable exports; note/event projections
//! remain explicit follow-up work.

mod audio;
mod parameters;
mod process_audio;

use core::{fmt::Write as _, marker::PhantomData, num::NonZeroU32};
use std::{
    io::{Read as _, Write as _},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    vec::Vec,
};

use audio::{AudioMappingError, ClapAudioConfiguration, ClapProcessSlot};
use chassis_core::{
    audio::PortDirection,
    automation::ParameterEvents,
    parameters::{ChoiceOption, ParameterKind, ParameterStore},
    process::{ProcessConfig, ProcessContext, ProcessMode, TransportSnapshot},
    runtime::{Component, InstanceRuntime, Process as ChassisProcess, Processor},
    state::{StateDocument, StateLimits},
};
use clack_extensions::{
    audio_ports::{AudioPortInfoWriter, PluginAudioPorts, PluginAudioPortsImpl},
    latency::{HostLatency, PluginLatency, PluginLatencyImpl},
    params::{
        HostParams, ParamDisplayWriter, ParamInfo, ParamRescanFlags, PluginAudioProcessorParams,
        PluginMainThreadParams, PluginParams,
    },
    render::{PluginRender, PluginRenderImpl, RenderMode},
    state::{PluginState, PluginStateImpl},
};
use clack_plugin::{
    entry::{DefaultPluginFactory, SinglePluginEntry},
    events::{
        event_types::{TransportEvent, TransportFlags},
        io::{InputEvents, OutputEvents},
    },
    plugin::features::{AUDIO_EFFECT, STEREO},
    prelude::{
        Audio, ClapId, Events, HostAudioProcessorHandle, HostMainThreadHandle, HostSharedHandle,
        Plugin, PluginAudioConfiguration, PluginAudioProcessor, PluginDescriptor, PluginError,
        PluginExtensions, PluginMainThread, PluginShared, Process as ClapProcess, ProcessStatus,
    },
    stream::{InputStream, OutputStream},
    utils::Cookie,
};
use parameters::{
    ClapParameterState, ParameterMappingError, ParameterStateError, ParameterSyncError,
    normalized_events,
};
use process_audio::{ClapBufferSource, ClapSample, ClapSamplePrecision, sample_precision};

/// Explicit CLAP audio-port projection metadata.
pub use audio::{ClapAudioPort, DEFAULT_CLAP_AUDIO_PORTS};
/// Re-export of Clack's CLAP entry macro for Chassis export crates.
pub use clack_plugin::clack_export_entry;

/// Temporary CLAP metadata contract for the conventional effect export.
///
/// Parameter and audio-port IDs are explicit because stable Chassis keys cannot
/// be silently hashed or assigned from declaration order without creating a
/// persistence compatibility hazard. Audio layouts and in-place relationships
/// are validated at setup and projected into an allocation-free process plan.
pub trait ClapStereoEffect: Component + Default + 'static {
    /// Stable reverse-domain CLAP plugin identifier.
    const CLAP_ID: &'static str;

    /// Human-readable CLAP plugin name.
    const CLAP_NAME: &'static str;

    /// Stable CLAP audio-port IDs and layouts keyed by canonical Chassis port key.
    ///
    /// The default projects conventional stereo main input/output plus an
    /// optional stereo sidechain. Products may provide another mapping accepted
    /// by their Chassis audio-port schema. Reciprocal in-place pairs are aligned
    /// into one safe CLAP process slot when the dense host layout permits it;
    /// unrelated ports sharing an index remain semantically independent.
    const CLAP_AUDIO_PORTS: &'static [ClapAudioPort] = &DEFAULT_CLAP_AUDIO_PORTS;

    /// Stable CLAP parameter IDs keyed by canonical Chassis parameter key.
    ///
    /// The mapping must contain exactly one entry for every descriptor returned
    /// by [`Component::parameter_descriptors`]. Float, integer, boolean, and
    /// choice descriptors are projected; choices use their stepped plain index
    /// for CLAP while Chassis retains stable semantic option identity in state.
    const CLAP_PARAMETER_IDS: &'static [(&'static str, u32)] = &[];

    /// Maximum number of known CLAP parameter value events accepted per block.
    ///
    /// This is an explicit product/deployment bound. Zero is correct for a
    /// component that has no parameter projection.
    const CLAP_MAX_PARAMETER_EVENTS: u32 = 0;

    /// Maximum total input events inspected by process or parameter flush.
    ///
    /// Includes unknown events and IDs, independently of normalized parameter
    /// capacity. Process rejects oversized batches; flush ignores the complete
    /// batch because CLAP provides no failure return for that callback.
    const CLAP_MAX_INPUT_EVENTS: u32 = 1024;

    /// Chassis parameter-state schema version projected through CLAP state.
    const CLAP_STATE_SCHEMA: u32 = 1;
}

/// Precision capability marker for a CLAP export that accepts only f32 audio.
pub struct F32Only;

/// Precision capability marker for a CLAP export that accepts f32 and f64 audio.
pub struct F32AndF64;

/// Clack plugin marker that exports one [`ClapStereoEffect`] through Chassis.
pub struct ChassisPlugin<C, M = F32Only>(PhantomData<fn() -> (C, M)>);

/// Single-plugin CLAP entry type for an f32-only Chassis stereo effect.
pub type SingleComponentEntry<C> = SinglePluginEntry<ChassisPlugin<C, F32Only>>;

/// Single-plugin CLAP entry type for a Chassis stereo effect with f64 support.
pub type SingleComponentEntryWithF64<C> = SinglePluginEntry<ChassisPlugin<C, F32AndF64>>;

#[derive(Default)]
struct RenderState {
    offline: AtomicBool,
}

impl RenderState {
    fn set(&self, mode: RenderMode) {
        self.offline
            .store(matches!(mode, RenderMode::Offline), Ordering::Release);
    }

    fn process_mode(&self) -> ProcessMode {
        if self.offline.load(Ordering::Acquire) {
            ProcessMode::Offline
        } else {
            ProcessMode::Realtime
        }
    }
}

/// Thread-safe adapter state shared across CLAP domains.
///
/// Scalar parameter publication and the ephemeral host render-mode projection are
/// independent adapter-local synchronization paths. The audio-domain
/// [`InstanceRuntime`] remains the semantic runtime authority while active.
pub struct ChassisShared {
    parameters: Arc<ClapParameterState>,
    render: Arc<RenderState>,
}

impl PluginShared<'_> for ChassisShared {}

/// Main-thread owner of the format-independent Chassis component definition.
pub struct ChassisMainThread<'host, C> {
    component: C,
    audio: ClapAudioConfiguration,
    shared: Arc<ClapParameterState>,
    render: Arc<RenderState>,
    latency: AtomicU32,
    host: HostMainThreadHandle<'host>,
}

/// CLAP audio-thread wrapper around an activated Chassis instance runtime.
pub struct ChassisAudioProcessor<'a, P, M = F32Only>
where
    P: Processor,
{
    runtime: InstanceRuntime<P>,
    shared: &'a ChassisShared,
    normalized_events: Vec<chassis_core::automation::ParameterEvent<'a>>,
    control_values: Vec<f64>,
    process_slots: Vec<ClapProcessSlot>,
    _precision: PhantomData<fn() -> M>,
}

impl<C> Plugin for ChassisPlugin<C, F32Only>
where
    C: ClapStereoEffect,
    C::Processor: ChassisProcess<f32> + Send + 'static,
{
    type AudioProcessor<'a> = ChassisAudioProcessor<'a, C::Processor, F32Only>;
    type Shared<'a> = ChassisShared;
    type MainThread<'a> = ChassisMainThread<'a, C>;

    fn declare_extensions(builder: &mut PluginExtensions<Self>, shared: Option<&Self::Shared<'_>>) {
        builder.register::<PluginAudioPorts>();
        builder.register::<PluginLatency>();
        if shared.is_none_or(|shared| !shared.parameters.bindings().is_empty()) {
            builder.register::<PluginParams>();
        }
        builder.register::<PluginRender>();
        builder.register::<PluginState>();
    }
}

impl<C> DefaultPluginFactory for ChassisPlugin<C>
where
    C: ClapStereoEffect,
    C::Processor: ChassisProcess<f32> + Send + 'static,
{
    fn get_descriptor() -> PluginDescriptor {
        PluginDescriptor::new(C::CLAP_ID, C::CLAP_NAME).with_features([AUDIO_EFFECT, STEREO])
    }

    fn new_shared(_host: HostSharedHandle) -> Result<ChassisShared, PluginError> {
        new_shared::<C>()
    }

    fn new_main_thread<'a>(
        host: HostMainThreadHandle<'a>,
        shared: &'a ChassisShared,
    ) -> Result<ChassisMainThread<'a, C>, PluginError> {
        new_main_thread::<C>(shared, false, host)
    }
}

impl<C> Plugin for ChassisPlugin<C, F32AndF64>
where
    C: ClapStereoEffect,
    C::Processor: ChassisProcess<f32> + ChassisProcess<f64> + Send + 'static,
{
    type AudioProcessor<'a> = ChassisAudioProcessor<'a, C::Processor, F32AndF64>;
    type Shared<'a> = ChassisShared;
    type MainThread<'a> = ChassisMainThread<'a, C>;

    fn declare_extensions(builder: &mut PluginExtensions<Self>, shared: Option<&Self::Shared<'_>>) {
        builder.register::<PluginAudioPorts>();
        builder.register::<PluginLatency>();
        if shared.is_none_or(|shared| !shared.parameters.bindings().is_empty()) {
            builder.register::<PluginParams>();
        }
        builder.register::<PluginRender>();
        builder.register::<PluginState>();
    }
}

impl<C> DefaultPluginFactory for ChassisPlugin<C, F32AndF64>
where
    C: ClapStereoEffect,
    C::Processor: ChassisProcess<f32> + ChassisProcess<f64> + Send + 'static,
{
    fn get_descriptor() -> PluginDescriptor {
        PluginDescriptor::new(C::CLAP_ID, C::CLAP_NAME).with_features([AUDIO_EFFECT, STEREO])
    }

    fn new_shared(_host: HostSharedHandle) -> Result<ChassisShared, PluginError> {
        new_shared::<C>()
    }

    fn new_main_thread<'a>(
        host: HostMainThreadHandle<'a>,
        shared: &'a ChassisShared,
    ) -> Result<ChassisMainThread<'a, C>, PluginError> {
        new_main_thread::<C>(shared, true, host)
    }
}

fn new_shared<C>() -> Result<ChassisShared, PluginError>
where
    C: ClapStereoEffect,
{
    let component = C::default();
    ParameterStore::new(component.parameter_descriptors())
        .map_err(|_| PluginError::Message("Invalid Chassis parameter schema"))?;
    if !component.parameter_descriptors().is_empty() && C::CLAP_MAX_PARAMETER_EVENTS == 0 {
        return Err(PluginError::Message(
            "CLAP parameter projection requires a positive event bound",
        ));
    }
    if C::CLAP_MAX_PARAMETER_EVENTS > C::CLAP_MAX_INPUT_EVENTS {
        return Err(PluginError::Message(
            "CLAP parameter event bound exceeds total input event bound",
        ));
    }
    let parameters =
        ClapParameterState::new(component.parameter_descriptors(), C::CLAP_PARAMETER_IDS)
            .map_err(|error| parameter_mapping_error(&error))?
            .with_input_event_bound(C::CLAP_MAX_INPUT_EVENTS);
    Ok(ChassisShared {
        parameters: Arc::new(parameters),
        render: Arc::new(RenderState::default()),
    })
}

fn new_main_thread<'a, C>(
    shared: &ChassisShared,
    supports_f64: bool,
    host: HostMainThreadHandle<'a>,
) -> Result<ChassisMainThread<'a, C>, PluginError>
where
    C: ClapStereoEffect,
{
    let component = C::default();
    if !shared
        .parameters
        .matches_descriptors(component.parameter_descriptors())
    {
        return Err(PluginError::Message(
            "CLAP component schema changed between shared and main-thread construction",
        ));
    }
    let mut audio = ClapAudioConfiguration::new(component.audio_ports(), C::CLAP_AUDIO_PORTS)
        .map_err(|error| audio_mapping_error(&error))?;
    if supports_f64 {
        audio = audio.with_f64_support();
    }
    Ok(ChassisMainThread {
        component,
        audio,
        shared: Arc::clone(&shared.parameters),
        render: Arc::clone(&shared.render),
        latency: AtomicU32::new(0),
        host,
    })
}

impl<'host, C> PluginMainThread<'host, ChassisShared> for ChassisMainThread<'host, C> where
    C: ClapStereoEffect
{
}

fn activate_processor<'a, C, P, M>(
    main_thread: &ChassisMainThread<'_, C>,
    shared: &'a ChassisShared,
    audio_config: PluginAudioConfiguration,
) -> Result<ChassisAudioProcessor<'a, P, M>, PluginError>
where
    C: ClapStereoEffect<Processor = P>,
    P: Processor,
{
    if !shared
        .parameters
        .matches_descriptors(main_thread.component.parameter_descriptors())
    {
        return Err(PluginError::Message(
            "CLAP component schema does not match its parameter projection",
        ));
    }
    let process = map_process_config(audio_config, C::CLAP_MAX_PARAMETER_EVENTS)?;
    let runtime = InstanceRuntime::for_component(&main_thread.component)
        .map_err(|_| PluginError::Message("Invalid Chassis instance runtime"))?;

    let maximum_events = usize::try_from(C::CLAP_MAX_PARAMETER_EVENTS)
        .map_err(|_| PluginError::Message("CLAP parameter event bound is not representable"))?;
    let mut normalized_events = Vec::new();
    normalized_events
        .try_reserve_exact(maximum_events)
        .map_err(PluginError::from)?;
    let mut control_values = Vec::new();
    control_values
        .try_reserve_exact(shared.parameters.bindings().len())
        .map_err(PluginError::from)?;
    control_values.resize(shared.parameters.bindings().len(), 0.0);
    let mut process_slots = Vec::new();
    process_slots
        .try_reserve_exact(main_thread.audio.process_slots().len())
        .map_err(PluginError::from)?;
    process_slots.extend_from_slice(main_thread.audio.process_slots());

    let mut processor = ChassisAudioProcessor {
        runtime,
        shared,
        normalized_events,
        control_values,
        process_slots,
        _precision: PhantomData::<fn() -> M>,
    };
    processor.shared.parameters.request_sync();
    processor.sync_parameters()?;
    processor
        .runtime
        .activate(
            &main_thread.component,
            process,
            main_thread.audio.audio_io(),
        )
        .map_err(|_| PluginError::Message("Chassis component activation failed"))?;

    let latency = processor
        .runtime
        .active_latency()
        .ok_or(PluginError::Message(
            "Chassis runtime did not publish activation latency",
        ))?
        .get();
    let previous_latency = main_thread.latency.swap(latency, Ordering::AcqRel);
    if previous_latency != latency
        && let Some(host_latency) = main_thread.host.get_extension::<HostLatency>()
    {
        host_latency.changed(&main_thread.host);
    }

    Ok(processor)
}

impl<C> PluginAudioPortsImpl for ChassisMainThread<'_, C>
where
    C: ClapStereoEffect,
{
    fn count(&self, is_input: bool) -> u32 {
        self.audio.count(if is_input {
            PortDirection::Input
        } else {
            PortDirection::Output
        })
    }

    fn get(&self, index: u32, is_input: bool, writer: &mut AudioPortInfoWriter) {
        let direction = if is_input {
            PortDirection::Input
        } else {
            PortDirection::Output
        };
        if let Some(info) = self.audio.info(index, direction) {
            writer.set(&info);
        }
    }
}

impl<C> PluginLatencyImpl for ChassisMainThread<'_, C>
where
    C: ClapStereoEffect,
{
    fn get(&self) -> u32 {
        self.latency.load(Ordering::Acquire)
    }
}

impl<C> PluginMainThreadParams for ChassisMainThread<'_, C>
where
    C: ClapStereoEffect,
{
    fn count(&self) -> u32 {
        u32::try_from(self.shared.bindings().len()).unwrap_or(u32::MAX)
    }

    fn get_info(&self, param_index: u32, writer: &mut clack_extensions::params::ParamInfoWriter) {
        let Some(binding) = usize::try_from(param_index)
            .ok()
            .and_then(|index| self.shared.bindings().get(index))
        else {
            return;
        };
        let module = binding
            .descriptor()
            .key()
            .as_str()
            .rsplit_once('/')
            .map_or(&[][..], |(module, _)| module.as_bytes());
        let (minimum, maximum) = binding.plain_range();
        writer.set(&ParamInfo {
            id: binding.id(),
            flags: binding.flags(),
            cookie: Cookie::empty(),
            name: binding.descriptor().name().as_bytes(),
            module,
            min_value: minimum,
            max_value: maximum,
            default_value: binding.default_plain(),
        });
    }

    fn get_value(&self, param_id: ClapId) -> Option<f64> {
        let index = self
            .shared
            .bindings()
            .iter()
            .position(|binding| binding.id() == param_id)?;
        Some(self.shared.value(index))
    }

    fn value_to_text(
        &self,
        param_id: ClapId,
        value: f64,
        writer: &mut ParamDisplayWriter,
    ) -> core::fmt::Result {
        let Some(binding) = self
            .shared
            .bindings()
            .iter()
            .find(|binding| binding.id() == param_id)
        else {
            return Err(core::fmt::Error);
        };
        if let ParameterKind::Choice { options, .. } = binding.descriptor().kind() {
            // Stepped parameters must convert any in-range plain value; hosts
            // legitimately present fractional positions between options.
            let Some(option) = options.get(nearest_option_index(options, value)) else {
                return Err(core::fmt::Error);
            };
            return write!(writer, "{}", option.name());
        }
        if binding.parameter_value(value).is_none() {
            return Err(core::fmt::Error);
        }
        write!(writer, "{value}")
    }

    fn text_to_value(&self, param_id: ClapId, text: &std::ffi::CStr) -> Option<f64> {
        let index = self
            .shared
            .bindings()
            .iter()
            .position(|binding| binding.id() == param_id)?;
        let binding = &self.shared.bindings()[index];
        let text = text.to_str().ok()?.trim();
        // Choice parameters primarily accept their option display names.
        if let ParameterKind::Choice { options, .. } = binding.descriptor().kind() {
            let plain = options.iter().position(|option| option.name() == text)?;
            let plain = f64::from(u32::try_from(plain).ok()?);
            return binding.parameter_value(plain).map(|_| plain);
        }
        let value = text.parse().ok()?;
        binding.parameter_value(value).map(|_| value)
    }

    fn flush(&self, input: &InputEvents, _output: &mut OutputEvents) {
        self.shared.apply_input(input);
    }
}

impl<C> PluginRenderImpl for ChassisMainThread<'_, C>
where
    C: ClapStereoEffect,
{
    fn has_hard_realtime_requirement(&self) -> bool {
        false
    }

    fn set(&self, mode: RenderMode) -> Result<(), PluginError> {
        self.render.set(mode);
        Ok(())
    }
}

impl<C> PluginStateImpl for ChassisMainThread<'_, C>
where
    C: ClapStereoEffect,
{
    fn save(&self, output: &mut OutputStream) -> Result<(), PluginError> {
        let encoded = self
            .shared
            .encode_state(C::CLAP_ID, C::CLAP_STATE_SCHEMA, StateLimits::default())
            .map_err(|error| state_error(&error))?;
        write_bounded_state(output, &encoded)
    }

    fn load(&self, input: &mut InputStream) -> Result<(), PluginError> {
        let limits = StateLimits::default();
        let encoded = read_bounded_state(input, limits.max_total_bytes)?;
        let document =
            StateDocument::decode_with_limits(&encoded, limits).map_err(PluginError::from)?;
        self.shared
            .apply_state(&document, C::CLAP_ID, C::CLAP_STATE_SCHEMA)
            .map_err(|error| state_error(&error))?;
        // A loaded state replaces host-visible plain values; CLAP requires the
        // plugin to request a value rescan rather than assuming hosts poll.
        if !self.shared.bindings().is_empty()
            && let Some(params) = self.host.get_extension::<HostParams>()
        {
            params.rescan(&self.host, ParamRescanFlags::VALUES);
        }
        Ok(())
    }
}

impl<'a, C, P> PluginAudioProcessor<'a, ChassisShared, ChassisMainThread<'a, C>>
    for ChassisAudioProcessor<'a, P, F32Only>
where
    C: ClapStereoEffect<Processor = P>,
    P: ChassisProcess<f32> + Processor + Send + 'static,
{
    fn activate(
        _host: HostAudioProcessorHandle<'a>,
        main_thread: &ChassisMainThread<'_, C>,
        shared: &'a ChassisShared,
        audio_config: PluginAudioConfiguration,
    ) -> Result<Self, PluginError> {
        activate_processor::<C, P, F32Only>(main_thread, shared, audio_config)
    }

    fn process(
        &mut self,
        process: ClapProcess,
        audio: Audio,
        events: Events,
    ) -> Result<ProcessStatus, PluginError> {
        self.process_block::<f32>(process, audio, &events, C::CLAP_MAX_PARAMETER_EVENTS)
    }

    fn deactivate(mut self, _main_thread: &ChassisMainThread<'_, C>) {
        let _ = self.runtime.deactivate();
    }

    fn reset(&mut self) {
        let _ = self.runtime.reset();
    }
}

impl<'a, C, P> PluginAudioProcessor<'a, ChassisShared, ChassisMainThread<'a, C>>
    for ChassisAudioProcessor<'a, P, F32AndF64>
where
    C: ClapStereoEffect<Processor = P>,
    P: ChassisProcess<f32> + ChassisProcess<f64> + Processor + Send + 'static,
{
    fn activate(
        _host: HostAudioProcessorHandle<'a>,
        main_thread: &ChassisMainThread<'_, C>,
        shared: &'a ChassisShared,
        audio_config: PluginAudioConfiguration,
    ) -> Result<Self, PluginError> {
        activate_processor::<C, P, F32AndF64>(main_thread, shared, audio_config)
    }

    fn process(
        &mut self,
        process: ClapProcess,
        mut audio: Audio,
        events: Events,
    ) -> Result<ProcessStatus, PluginError> {
        let precision = sample_precision(&mut audio, &self.process_slots)?;
        match precision {
            ClapSamplePrecision::F32 => {
                self.process_block::<f32>(process, audio, &events, C::CLAP_MAX_PARAMETER_EVENTS)
            }
            ClapSamplePrecision::F64 => {
                self.process_block::<f64>(process, audio, &events, C::CLAP_MAX_PARAMETER_EVENTS)
            }
        }
    }

    fn deactivate(mut self, _main_thread: &ChassisMainThread<'_, C>) {
        let _ = self.runtime.deactivate();
    }

    fn reset(&mut self) {
        let _ = self.runtime.reset();
    }
}

impl<P> PluginAudioProcessorParams for ChassisAudioProcessor<'_, P, F32Only>
where
    P: ChassisProcess<f32> + Processor + Send + 'static,
{
    fn flush(&mut self, input: &InputEvents, _output: &mut OutputEvents) {
        self.shared.parameters.apply_input(input);
    }
}

impl<P> PluginAudioProcessorParams for ChassisAudioProcessor<'_, P, F32AndF64>
where
    P: ChassisProcess<f32> + ChassisProcess<f64> + Processor + Send + 'static,
{
    fn flush(&mut self, input: &InputEvents, _output: &mut OutputEvents) {
        self.shared.parameters.apply_input(input);
    }
}

impl<P, M> ChassisAudioProcessor<'_, P, M>
where
    P: Processor,
{
    fn process_block<S>(
        &mut self,
        process: ClapProcess,
        audio: Audio,
        events: &Events,
        maximum_parameter_events: u32,
    ) -> Result<ProcessStatus, PluginError>
    where
        P: ChassisProcess<S>,
        S: ClapSample,
    {
        let generation = self.shared.parameters.begin_block();
        self.sync_parameters()?;
        let transport = map_transport(process.transport)?;
        let mut buffers = ClapBufferSource::<S>::new(audio, &self.process_slots)?;
        let frame_count = buffers.frame_count();

        normalized_events(
            &self.shared.parameters,
            events.input,
            &mut self.normalized_events,
        )
        .map_err(PluginError::Message)?;
        let parameter_events = ParameterEvents::new(
            &self.normalized_events,
            frame_count,
            maximum_parameter_events,
        )
        .map_err(|_| PluginError::Message("Invalid CLAP parameter event stream"))?;
        let context = ProcessContext::new(
            self.shared.render.process_mode(),
            transport,
            parameter_events,
        );
        self.runtime
            .process_source(frame_count, context, &mut buffers)
            .map_err(|_| PluginError::Message("Invalid Chassis process block"))?;

        self.shared
            .parameters
            .publish_events(
                &self.normalized_events,
                generation,
                &mut self.control_values,
            )
            .map_err(sync_error)?;
        self.sync_parameters()?;

        Ok(ProcessStatus::Continue)
    }

    fn sync_parameters(&mut self) -> Result<(), PluginError> {
        self.shared
            .parameters
            .sync_into(self.runtime.parameters_mut(), &mut self.control_values)
            .map_err(sync_error)
    }
}

fn audio_mapping_error(_error: &AudioMappingError) -> PluginError {
    PluginError::Message("Invalid CLAP audio-port mapping")
}

/// Index of the option nearest a possibly fractional plain position.
fn nearest_option_index(options: &[ChoiceOption], value: f64) -> usize {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    let last = options.len().saturating_sub(1);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let rounded = (value + 0.5) as usize;
    rounded.min(last)
}

fn parameter_mapping_error(_error: &ParameterMappingError) -> PluginError {
    PluginError::Message("Invalid CLAP parameter mapping")
}

fn sync_error(_error: ParameterSyncError) -> PluginError {
    PluginError::Message("CLAP parameter projection could not reach the audio processor")
}

fn state_error(_error: &ParameterStateError) -> PluginError {
    PluginError::Message("Invalid Chassis parameter state")
}

fn read_bounded_state(input: &mut InputStream, maximum: usize) -> Result<Vec<u8>, PluginError> {
    let mut encoded = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        let count = input.read(&mut chunk).map_err(PluginError::from)?;
        if count == 0 {
            break;
        }
        if count > chunk.len() {
            return Err(PluginError::Message(
                "CLAP state stream returned an invalid read count",
            ));
        }
        let Some(new_length) = encoded.len().checked_add(count) else {
            return Err(PluginError::Message("CLAP state stream size overflow"));
        };
        if new_length > maximum {
            return Err(PluginError::Message(
                "CLAP state stream exceeds Chassis bounds",
            ));
        }
        encoded.extend_from_slice(&chunk[..count]);
    }
    Ok(encoded)
}

fn write_bounded_state(output: &mut OutputStream, encoded: &[u8]) -> Result<(), PluginError> {
    let mut offset = 0;
    while offset < encoded.len() {
        let count = output
            .write(&encoded[offset..])
            .map_err(PluginError::from)?;
        if count == 0 {
            return Err(PluginError::Message("CLAP state stream made no progress"));
        }
        if count > encoded.len() - offset {
            return Err(PluginError::Message(
                "CLAP state stream returned an invalid write count",
            ));
        }
        offset += count;
    }
    Ok(())
}

fn map_transport(transport: Option<&TransportEvent>) -> Result<TransportSnapshot, PluginError> {
    let Some(transport) = transport else {
        return Ok(TransportSnapshot::unknown());
    };
    let tempo = transport
        .flags
        .contains(TransportFlags::HAS_TEMPO)
        .then_some(transport.tempo)
        .filter(|tempo| tempo.is_finite() && *tempo > 0.0);
    TransportSnapshot::new(
        Some(transport.flags.contains(TransportFlags::IS_PLAYING)),
        Some(transport.flags.contains(TransportFlags::IS_RECORDING)),
        tempo,
        None,
    )
    .map_err(|_| PluginError::Message("Invalid CLAP transport metadata"))
}

fn map_process_config(
    config: PluginAudioConfiguration,
    max_parameter_events: u32,
) -> Result<ProcessConfig, PluginError> {
    const CLAP_MAX_FRAMES: u32 = 2_147_483_647;

    let minimum = NonZeroU32::new(config.min_frames_count).ok_or(PluginError::Message(
        "CLAP minimum frame count must be positive",
    ))?;
    let maximum = NonZeroU32::new(config.max_frames_count).ok_or(PluginError::Message(
        "CLAP maximum frame count must be positive",
    ))?;

    if config.min_frames_count > CLAP_MAX_FRAMES || config.max_frames_count > CLAP_MAX_FRAMES {
        return Err(PluginError::Message(
            "CLAP frame bounds exceed the specification limit",
        ));
    }

    ProcessConfig::new(
        config.sample_rate,
        Some(minimum),
        maximum,
        max_parameter_events,
    )
    .map_err(|_| PluginError::Message("Invalid CLAP process configuration"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clack_plugin::{
        events::{EventFlags, EventHeader},
        utils::FixedPoint,
    };

    fn transport(flags: TransportFlags, tempo: f64) -> TransportEvent {
        TransportEvent {
            header: EventHeader::new_core(0, EventFlags::empty()),
            flags,
            song_pos_beats: FixedPoint::default(),
            song_pos_seconds: FixedPoint::default(),
            tempo,
            tempo_inc: 0.0,
            loop_start_beats: FixedPoint::default(),
            loop_end_beats: FixedPoint::default(),
            loop_start_seconds: FixedPoint::default(),
            loop_end_seconds: FixedPoint::default(),
            bar_start: FixedPoint::default(),
            bar_number: 0,
            time_signature_numerator: 4,
            time_signature_denominator: 4,
        }
    }

    #[test]
    fn maps_clap_process_bounds_and_parameter_event_budget() {
        let config = PluginAudioConfiguration {
            sample_rate: 48_000.0,
            min_frames_count: 1,
            max_frames_count: 128,
        };
        let mapped = map_process_config(config, 7).expect("CLAP configuration is valid");
        assert!((mapped.sample_rate() - 48_000.0).abs() <= f64::EPSILON);
        assert_eq!(mapped.guaranteed_min_frames().map(NonZeroU32::get), Some(1));
        assert_eq!(mapped.max_frames().get(), 128);
        assert_eq!(mapped.max_parameter_events(), 7);

        assert!(
            map_process_config(
                PluginAudioConfiguration {
                    sample_rate: 48_000.0,
                    min_frames_count: 0,
                    max_frames_count: 128,
                },
                7,
            )
            .is_err()
        );
    }

    #[test]
    fn maps_optional_transport_and_ignores_invalid_tempo() {
        assert_eq!(
            map_transport(None).expect("missing transport is valid"),
            TransportSnapshot::unknown()
        );

        let event = transport(
            TransportFlags::IS_PLAYING | TransportFlags::IS_RECORDING | TransportFlags::HAS_TEMPO,
            120.0,
        );
        let mapped = map_transport(Some(&event)).expect("transport is valid");
        assert_eq!(mapped.playing(), Some(true));
        assert_eq!(mapped.recording(), Some(true));
        assert_eq!(mapped.tempo_bpm(), Some(120.0));
        assert_eq!(mapped.sample_position(), None);

        let invalid = transport(TransportFlags::HAS_TEMPO, 0.0);
        assert_eq!(
            map_transport(Some(&invalid))
                .expect("invalid optional tempo is ignored")
                .tempo_bpm(),
            None
        );
    }

    #[test]
    fn render_state_maps_clap_mode_to_process_mode() {
        let render = RenderState::default();
        assert_eq!(render.process_mode(), ProcessMode::Realtime);
        render.set(RenderMode::Offline);
        assert_eq!(render.process_mode(), ProcessMode::Offline);
        render.set(RenderMode::Realtime);
        assert_eq!(render.process_mode(), ProcessMode::Realtime);
    }
}
