//! Initial CLAP deployment adapter for Chassis.
//!
//! This adapter currently proves the conventional stereo effect topology with
//! an optional stereo sidechain plus a bounded parameter projection for scalar
//! Chassis parameters. Stable CLAP IDs are supplied explicitly rather than
//! derived from declaration order. Choice parameters, arbitrary process-time
//! port topology, and richer note/event projections remain explicit follow-up
//! work rather than being silently approximated.

mod audio;
mod parameters;

use core::{fmt::Write as _, marker::PhantomData, num::NonZeroU32};
use std::{
    io::{Read as _, Write as _},
    sync::Arc,
    vec::Vec,
};

use audio::{AudioMappingError, ClapAudioConfiguration};
use chassis_core::{
    audio::{ChannelLayout, MAIN_INPUT, MAIN_OUTPUT, PortDirection, SIDECHAIN_INPUT},
    automation::ParameterEvents,
    buffer::{ChannelBuffer, InputEndpoint, OutputEndpoint},
    parameters::ParameterStore,
    process::{ProcessConfig, ProcessContext, ProcessMode, TransportSnapshot},
    runtime::{Component, InstanceRuntime, Process as ChassisProcess, Processor},
    state::{StateDocument, StateLimits},
};
use clack_extensions::{
    audio_ports::{AudioPortInfoWriter, PluginAudioPorts, PluginAudioPortsImpl},
    params::{
        ParamDisplayWriter, ParamInfo, PluginAudioProcessorParams, PluginMainThreadParams,
        PluginParams,
    },
    state::{PluginState, PluginStateImpl},
};
use clack_plugin::process::audio::PortPair;
use clack_plugin::{
    entry::{DefaultPluginFactory, SinglePluginEntry},
    events::{
        event_types::{TransportEvent, TransportFlags},
        io::{InputEvents, OutputEvents},
    },
    plugin::features::{AUDIO_EFFECT, STEREO},
    prelude::{
        Audio, ChannelPair, ClapId, Events, HostAudioProcessorHandle, HostMainThreadHandle,
        HostSharedHandle, Plugin, PluginAudioConfiguration, PluginAudioProcessor, PluginDescriptor,
        PluginError, PluginExtensions, PluginMainThread, PluginShared, Process as ClapProcess,
        ProcessStatus,
    },
    stream::{InputStream, OutputStream},
    utils::Cookie,
};
use parameters::{
    ClapParameterState, ParameterMappingError, ParameterStateError, ParameterSyncError,
    normalized_events,
};

/// Explicit CLAP audio-port projection metadata.
pub use audio::{ClapAudioPort, DEFAULT_CLAP_AUDIO_PORTS};
/// Re-export of Clack's CLAP entry macro for Chassis export crates.
pub use clack_plugin::clack_export_entry;

/// Temporary CLAP metadata contract for the conventional stereo-effect adapter.
///
/// Parameter and audio-port IDs are explicit because stable Chassis keys cannot
/// be silently hashed or assigned from declaration order without creating a
/// persistence compatibility hazard. The setup mapper already validates general
/// declared layouts, while the current process bridge intentionally qualifies
/// only stereo main input/output with zero or one stereo sidechain.
pub trait ClapStereoEffect: Component + Default + 'static {
    /// Stable reverse-domain CLAP plugin identifier.
    const CLAP_ID: &'static str;

    /// Human-readable CLAP plugin name.
    const CLAP_NAME: &'static str;

    /// Stable CLAP audio-port IDs and layouts keyed by canonical Chassis port key.
    ///
    /// The default projects the conventional stereo main input/output and stereo
    /// sidechain. Products may omit the optional sidechain by overriding this
    /// slice with only the main mappings. Other mappings are validated at setup
    /// but are rejected by the current process bridge until arbitrary callback
    /// topology has a no-allocation borrow representation.
    const CLAP_AUDIO_PORTS: &'static [ClapAudioPort] = &DEFAULT_CLAP_AUDIO_PORTS;

    /// Stable CLAP parameter IDs keyed by canonical Chassis parameter key.
    ///
    /// The mapping must contain exactly one entry for every descriptor returned
    /// by [`Component::parameter_descriptors`]. The initial adapter supports
    /// scalar float, integer, and boolean descriptors; choices are rejected
    /// during plugin construction until their index/identity projection is
    /// specified.
    const CLAP_PARAMETER_IDS: &'static [(&'static str, u32)] = &[];

    /// Maximum number of known CLAP parameter value events accepted per block.
    ///
    /// This is an explicit product/deployment bound. Zero is correct for a
    /// component that has no parameter projection.
    const CLAP_MAX_PARAMETER_EVENTS: u32 = 0;

    /// Chassis parameter-state schema version projected through CLAP state.
    const CLAP_STATE_SCHEMA: u32 = 1;
}

/// Clack plugin marker that exports one [`ClapStereoEffect`] through Chassis.
pub struct ChassisPlugin<C>(PhantomData<fn() -> C>);

/// Single-plugin CLAP entry type for a Chassis stereo effect.
pub type SingleComponentEntry<C> = SinglePluginEntry<ChassisPlugin<C>>;

/// Thread-safe scalar parameter publication shared by CLAP domains.
///
/// This remains an adapter-local cross-domain bridge while the equivalent
/// general framework publication primitive is model-tested. The audio-domain
/// [`InstanceRuntime`] is rebuilt from this durable publication before every
/// activation and remains the semantic runtime projection while active.
pub struct ChassisShared {
    parameters: Arc<ClapParameterState>,
}

impl PluginShared<'_> for ChassisShared {}

/// Main-thread owner of the format-independent Chassis component definition.
pub struct ChassisMainThread<C> {
    component: C,
    audio: ClapAudioConfiguration,
    shared: Arc<ClapParameterState>,
}

/// CLAP audio-thread wrapper around an activated Chassis instance runtime.
pub struct ChassisAudioProcessor<'a, P>
where
    P: Processor,
{
    runtime: InstanceRuntime<P>,
    shared: &'a ChassisShared,
    normalized_events: Vec<chassis_core::automation::ParameterEvent<'static>>,
    control_values: Vec<f64>,
    sidechain_enabled: bool,
}

impl<C> Plugin for ChassisPlugin<C>
where
    C: ClapStereoEffect,
    C::Processor: ChassisProcess<f32> + Send + 'static,
{
    type AudioProcessor<'a> = ChassisAudioProcessor<'a, C::Processor>;
    type Shared<'a> = ChassisShared;
    type MainThread<'a> = ChassisMainThread<C>;

    fn declare_extensions(
        builder: &mut PluginExtensions<Self>,
        _shared: Option<&Self::Shared<'_>>,
    ) {
        builder.register::<PluginAudioPorts>();
        builder.register::<PluginParams>();
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
        let component = C::default();
        ParameterStore::new(component.parameter_descriptors())
            .map_err(|_| PluginError::Message("Invalid Chassis parameter schema"))?;
        if !component.parameter_descriptors().is_empty() && C::CLAP_MAX_PARAMETER_EVENTS == 0 {
            return Err(PluginError::Message(
                "CLAP parameter projection requires a positive event bound",
            ));
        }
        let parameters =
            ClapParameterState::new(component.parameter_descriptors(), C::CLAP_PARAMETER_IDS)
                .map_err(|error| parameter_mapping_error(&error))?;
        Ok(ChassisShared {
            parameters: Arc::new(parameters),
        })
    }

    fn new_main_thread<'a>(
        _host: HostMainThreadHandle<'a>,
        shared: &'a ChassisShared,
    ) -> Result<ChassisMainThread<C>, PluginError> {
        let component = C::default();
        if !shared
            .parameters
            .matches_descriptors(component.parameter_descriptors())
        {
            return Err(PluginError::Message(
                "CLAP component schema changed between shared and main-thread construction",
            ));
        }
        let audio = ClapAudioConfiguration::new(component.audio_ports(), C::CLAP_AUDIO_PORTS)
            .map_err(|error| audio_mapping_error(&error))?;
        Ok(ChassisMainThread {
            component,
            audio,
            shared: Arc::clone(&shared.parameters),
        })
    }
}

impl<C> PluginMainThread<'_, ChassisShared> for ChassisMainThread<C> where C: ClapStereoEffect {}

impl<C> PluginAudioPortsImpl for ChassisMainThread<C>
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

impl<C> PluginMainThreadParams for ChassisMainThread<C>
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
        let value = text.to_str().ok()?.trim().parse().ok()?;
        self.shared.bindings()[index]
            .parameter_value(value)
            .map(|_| value)
    }

    fn flush(&self, input: &InputEvents, _output: &mut OutputEvents) {
        self.shared.apply_input(input);
    }
}

impl<C> PluginStateImpl for ChassisMainThread<C>
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
            .map_err(|error| state_error(&error))
    }
}

impl<'a, C, P> PluginAudioProcessor<'a, ChassisShared, ChassisMainThread<C>>
    for ChassisAudioProcessor<'a, P>
where
    C: ClapStereoEffect<Processor = P>,
    P: ChassisProcess<f32> + Processor + Send + 'static,
{
    fn activate(
        _host: HostAudioProcessorHandle<'a>,
        main_thread: &ChassisMainThread<C>,
        shared: &'a ChassisShared,
        audio_config: PluginAudioConfiguration,
    ) -> Result<Self, PluginError> {
        if !shared
            .parameters
            .matches_descriptors(main_thread.component.parameter_descriptors())
        {
            return Err(PluginError::Message(
                "CLAP component schema does not match its parameter projection",
            ));
        }
        let sidechain_enabled = validate_stereo_process_mapping(&main_thread.audio)?;
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

        let mut processor = Self {
            runtime,
            shared,
            normalized_events,
            control_values,
            sidechain_enabled,
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
        Ok(processor)
    }

    fn process(
        &mut self,
        process: ClapProcess,
        mut audio: Audio,
        events: Events,
    ) -> Result<ProcessStatus, PluginError> {
        self.sync_parameters()?;
        let transport = map_transport(process.transport)?;
        let expected_inputs = if self.sidechain_enabled { 2 } else { 1 };
        if audio.input_port_count() != expected_inputs || audio.output_port_count() != 1 {
            return Err(PluginError::Message(
                "CLAP audio buffers do not match the activated Chassis stereo topology",
            ));
        }

        let mut ports = audio.port_pairs();
        let main = ports
            .next()
            .ok_or(PluginError::Message("Missing CLAP main audio port pair"))?;
        let frame_count = main.frames_count();
        let sidechain = self
            .sidechain_enabled
            .then(|| {
                ports
                    .next()
                    .ok_or(PluginError::Message("Missing CLAP sidechain input port"))
            })
            .transpose()?;
        if ports.next().is_some() {
            return Err(PluginError::Message(
                "CLAP supplied unexpected audio ports for the activated topology",
            ));
        }

        normalized_events(
            &self.shared.parameters,
            events.input,
            &mut self.normalized_events,
        )
        .map_err(PluginError::Message)?;
        let parameter_events = ParameterEvents::new(
            &self.normalized_events,
            frame_count,
            C::CLAP_MAX_PARAMETER_EVENTS,
        )
        .map_err(|_| PluginError::Message("Invalid CLAP parameter event stream"))?;
        let context = ProcessContext::new(ProcessMode::Realtime, transport, parameter_events);
        process_audio_ports(&mut self.runtime, main, sidechain, context)?;

        self.shared
            .parameters
            .publish_events(&self.normalized_events, &mut self.control_values)
            .map_err(sync_error)?;
        self.sync_parameters()?;

        Ok(ProcessStatus::Continue)
    }

    fn deactivate(mut self, _main_thread: &ChassisMainThread<C>) {
        let _ = self.runtime.deactivate();
    }

    fn reset(&mut self) {
        let _ = self.runtime.reset();
    }
}

fn process_audio_ports<P>(
    runtime: &mut InstanceRuntime<P>,
    mut main: PortPair<'_>,
    mut sidechain: Option<PortPair<'_>>,
    context: ProcessContext<'_>,
) -> Result<(), PluginError>
where
    P: ChassisProcess<f32> + Processor,
{
    let frame_count = main.frames_count();
    let channels = main.channels()?.into_f32().ok_or(PluginError::Message(
        "Chassis stereo topology requires f32 main audio",
    ))?;
    if channels.channel_pair_count() != 2 {
        return Err(PluginError::Message(
            "Chassis stereo topology requires exactly two main channels",
        ));
    }

    let mut channels = channels.into_iter();
    let left = channels
        .next()
        .ok_or(PluginError::Message("Missing left main channel"))?;
    let right = channels
        .next()
        .ok_or(PluginError::Message("Missing right main channel"))?;
    let Some(sidechain) = sidechain.as_mut() else {
        let mut buffers = [
            map_main_channel(left, 0, frame_count)?,
            map_main_channel(right, 1, frame_count)?,
        ];
        return runtime
            .process(frame_count, context, &mut buffers)
            .map_err(|_| PluginError::Message("Invalid Chassis process block"));
    };

    if sidechain.frames_count() != frame_count {
        return Err(PluginError::Message(
            "CLAP sidechain frame count differs from main audio",
        ));
    }
    let sidechain_channels = sidechain
        .channels()?
        .into_f32()
        .ok_or(PluginError::Message(
            "Chassis stereo sidechain requires f32 audio",
        ))?;
    if sidechain_channels.channel_pair_count() != 2 {
        return Err(PluginError::Message(
            "Chassis stereo sidechain requires exactly two channels",
        ));
    }
    let mut sidechain_channels = sidechain_channels.into_iter();
    let sidechain_left = sidechain_channels
        .next()
        .ok_or(PluginError::Message("Missing left sidechain channel"))?;
    let sidechain_right = sidechain_channels
        .next()
        .ok_or(PluginError::Message("Missing right sidechain channel"))?;
    let mut buffers = [
        map_main_channel(left, 0, frame_count)?,
        map_main_channel(right, 1, frame_count)?,
        map_sidechain_channel(&sidechain_left, 0, frame_count)?,
        map_sidechain_channel(&sidechain_right, 1, frame_count)?,
    ];
    runtime
        .process(frame_count, context, &mut buffers)
        .map_err(|_| PluginError::Message("Invalid Chassis process block"))
}

impl<P> PluginAudioProcessorParams for ChassisAudioProcessor<'_, P>
where
    P: ChassisProcess<f32> + Processor + Send + 'static,
{
    fn flush(&mut self, input: &InputEvents, _output: &mut OutputEvents) {
        self.shared.parameters.apply_input(input);
    }
}

impl<P> ChassisAudioProcessor<'_, P>
where
    P: Processor,
{
    fn sync_parameters(&mut self) -> Result<(), PluginError> {
        self.shared
            .parameters
            .sync_into(self.runtime.parameters_mut(), &mut self.control_values)
            .map_err(sync_error)
    }
}

fn validate_stereo_process_mapping(audio: &ClapAudioConfiguration) -> Result<bool, PluginError> {
    let input_count = audio.count(PortDirection::Input);
    let output_count = audio.count(PortDirection::Output);
    if !(input_count == 1 || input_count == 2) || output_count != 1 {
        return Err(PluginError::Message(
            "Current CLAP process bridge supports stereo main plus at most one sidechain",
        ));
    }

    let main_input = audio
        .input(0)
        .ok_or(PluginError::Message("Missing mapped Chassis main input"))?;
    let main_output = audio
        .output(0)
        .ok_or(PluginError::Message("Missing mapped Chassis main output"))?;
    if main_input.key != MAIN_INPUT
        || main_output.key != MAIN_OUTPUT
        || main_input.layout != ChannelLayout::Stereo
        || main_output.layout != ChannelLayout::Stereo
    {
        return Err(PluginError::Message(
            "Current CLAP process bridge requires stereo Chassis main ports",
        ));
    }

    if input_count == 2 {
        let sidechain = audio
            .input(1)
            .ok_or(PluginError::Message("Missing mapped Chassis sidechain"))?;
        if sidechain.key != SIDECHAIN_INPUT || sidechain.layout != ChannelLayout::Stereo {
            return Err(PluginError::Message(
                "Current CLAP process bridge requires a stereo Chassis sidechain",
            ));
        }
        Ok(true)
    } else {
        Ok(false)
    }
}

fn audio_mapping_error(_error: &AudioMappingError) -> PluginError {
    PluginError::Message("Invalid CLAP audio-port mapping")
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
    TransportSnapshot::new(
        Some(transport.flags.contains(TransportFlags::IS_PLAYING)),
        Some(transport.flags.contains(TransportFlags::IS_RECORDING)),
        transport
            .flags
            .contains(TransportFlags::HAS_TEMPO)
            .then_some(transport.tempo),
        None,
    )
    .map_err(|_| PluginError::Message("Invalid CLAP transport tempo"))
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

fn map_main_channel(
    pair: ChannelPair<'_, f32>,
    channel: u32,
    frame_count: u32,
) -> Result<ChannelBuffer<'_, f32>, PluginError> {
    let input = InputEndpoint::new(MAIN_INPUT, channel);
    let output = OutputEndpoint::new(MAIN_OUTPUT, channel);

    match pair {
        ChannelPair::InputOutput(input_samples, output_samples) => {
            ChannelBuffer::separate(input, input_samples, output, output_samples, frame_count)
                .map_err(|_| PluginError::Message("Invalid disjoint CLAP channel buffers"))
        }
        ChannelPair::InPlace(samples) => {
            ChannelBuffer::in_place(input, output, samples, frame_count)
                .map_err(|_| PluginError::Message("Invalid in-place CLAP channel buffer"))
        }
        ChannelPair::InputOnly(_) | ChannelPair::OutputOnly(_) => Err(PluginError::Message(
            "Required CLAP main input/output channel is missing",
        )),
    }
}

fn map_sidechain_channel<'a>(
    pair: &ChannelPair<'a, f32>,
    channel: u32,
    frame_count: u32,
) -> Result<ChannelBuffer<'a, f32>, PluginError> {
    let input = InputEndpoint::new(SIDECHAIN_INPUT, channel);
    match pair {
        ChannelPair::InputOnly(samples) => ChannelBuffer::input_only(input, samples, frame_count)
            .map_err(|_| PluginError::Message("Invalid CLAP sidechain channel buffer")),
        ChannelPair::InputOutput(_, _) | ChannelPair::InPlace(_) | ChannelPair::OutputOnly(_) => {
            Err(PluginError::Message(
                "CLAP sidechain must be an input-only port",
            ))
        }
    }
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
    fn maps_optional_transport_and_rejects_invalid_tempo() {
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
        assert!(map_transport(Some(&invalid)).is_err());
    }

    #[test]
    fn current_process_mapping_accepts_main_only_or_default_sidechain() {
        let main_only = [DEFAULT_CLAP_AUDIO_PORTS[0], DEFAULT_CLAP_AUDIO_PORTS[1]];
        let main_only =
            ClapAudioConfiguration::new(&chassis_core::audio::DEFAULT_EFFECT_PORTS, &main_only)
                .expect("main-only mapping is valid");
        assert!(
            !validate_stereo_process_mapping(&main_only).expect("main-only process is supported")
        );

        let sidechain = ClapAudioConfiguration::new(
            &chassis_core::audio::DEFAULT_EFFECT_PORTS,
            &DEFAULT_CLAP_AUDIO_PORTS,
        )
        .expect("default mapping is valid");
        assert!(
            validate_stereo_process_mapping(&sidechain)
                .expect("default sidechain process is supported")
        );
    }
}
