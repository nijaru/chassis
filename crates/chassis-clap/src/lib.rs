//! Initial CLAP deployment adapter for Chassis.
//!
//! This adapter currently proves the default stereo effect path plus a bounded
//! parameter projection for scalar Chassis parameters. CLAP IDs are supplied by
//! each product and are checked against the format-independent schema during
//! plugin construction. Choice parameters and richer note/event projections
//! remain explicit follow-up work rather than being silently approximated.

mod parameters;
mod publication;

use core::{fmt::Write as _, marker::PhantomData, num::NonZeroU32};
use std::{
    io::{Read as _, Write as _},
    sync::Arc,
    vec::Vec,
};

use chassis_core::{
    audio::{DEFAULT_EFFECT_CONFIGURATION, MAIN_INPUT, MAIN_OUTPUT},
    automation::ParameterEvents,
    buffer::{ChannelBuffer, InputEndpoint, OutputEndpoint},
    parameters::ParameterStore,
    process::{ProcessConfig, ProcessContext, ProcessMode, TransportSnapshot},
    runtime::{Component, InstanceRuntime, Process as ChassisProcess, Processor},
    state::{StateDocument, StateLimits},
};
use clack_extensions::{
    audio_ports::{
        AudioPortFlags, AudioPortInfo, AudioPortInfoWriter, AudioPortType, PluginAudioPorts,
        PluginAudioPortsImpl,
    },
    params::{
        ParamDisplayWriter, ParamInfo, PluginAudioProcessorParams, PluginMainThreadParams,
        PluginParams,
    },
    state::{PluginState, PluginStateImpl},
};
use clack_plugin::{
    entry::{DefaultPluginFactory, SinglePluginEntry},
    events::{
        Event, Events, event_types::TransportEvent, io::InputEvents, io::OutputEvents,
        spaces::CoreEventSpace,
    },
    host::{HostAudioProcessorHandle, HostMainThreadHandle, HostSharedHandle},
    plugin::{
        Plugin, PluginAudioConfiguration, PluginAudioProcessor, PluginError, PluginMainThread,
        PluginShared,
    },
    process::{Audio, ChannelPair, Process as ClapProcess, ProcessStatus},
    stream::{InputStream, OutputStream},
    utils::ClapId,
};

use crate::parameters::{
    ClapParameterState, ParameterMappingError, ParameterStateError, ParameterSyncError,
    normalized_events,
};

/// Product contract for the initial Chassis CLAP stereo-effect adapter.
///
/// Products supply stable CLAP identity plus an explicit mapping from Chassis
/// parameter keys to stable CLAP numeric IDs. The adapter currently supports
/// the default stereo main input/output configuration and scalar float,
/// integer, and boolean parameters.
pub trait ClapStereoEffect: Component {
    /// Stable CLAP plugin identifier.
    const CLAP_ID: &'static str;
    /// Product schema version used by Chassis parameter state.
    const CLAP_STATE_SCHEMA: u32;
    /// Product display name.
    const CLAP_NAME: &'static str;
    /// Product vendor name.
    const CLAP_VENDOR: &'static str;
    /// Product version string.
    const CLAP_VERSION: &'static str;
    /// Maximum normalized parameter events accepted per process block.
    const CLAP_MAX_PARAMETER_EVENTS: u32;

    /// Stable Chassis parameter-key to CLAP-ID mapping.
    fn clap_parameter_ids() -> &'static [(&'static str, u32)];
}

/// Shared CLAP instance state retained across processor activation cycles.
pub struct ChassisShared {
    parameters: ClapParameterState,
}

impl PluginShared for ChassisShared {}

/// Main-thread CLAP object retaining the product definition.
pub struct ChassisMainThread<C> {
    component: C,
    _host: PhantomData<HostMainThreadHandle<'static>>,
}

impl<C> PluginMainThread<'_> for ChassisMainThread<C> where C: ClapStereoEffect {}

/// Active CLAP audio processor backed by a Chassis [`InstanceRuntime`].
pub struct ChassisAudioProcessor<'a, P>
where
    P: Processor,
{
    runtime: InstanceRuntime<P>,
    shared: &'a ChassisShared,
    normalized_events: Vec<chassis_core::automation::ParameterEvent<'static>>,
    control_values: Vec<f64>,
}

impl<C> Plugin for C
where
    C: ClapStereoEffect + Default + 'static,
    C::Processor: ChassisProcess<f32> + Processor + Send + 'static,
    C::ActivationError: core::fmt::Debug,
{
    type AudioProcessor<'a> = ChassisAudioProcessor<'a, C::Processor>;
    type Shared<'a> = ChassisShared;
    type MainThread<'a> = ChassisMainThread<C>;

    fn declare_extensions(builder: &mut clack_plugin::plugin::PluginExtensions<Self>) {
        builder
            .register::<PluginAudioPorts>()
            .register::<PluginParams>()
            .register::<PluginState>();
    }

    fn create_shared<'a>(
        _host: HostSharedHandle<'a>,
    ) -> Result<Self::Shared<'a>, PluginError> {
        let component = C::default();
        let parameters = ClapParameterState::new(
            component.parameter_descriptors(),
            C::clap_parameter_ids(),
        )
        .map_err(|error| parameter_mapping_error(&error))?;
        Ok(ChassisShared { parameters })
    }

    fn create_main_thread<'a>(
        _host: HostMainThreadHandle<'a>,
        _shared: &Self::Shared<'a>,
    ) -> Result<Self::MainThread<'a>, PluginError> {
        Ok(ChassisMainThread {
            component: C::default(),
            _host: PhantomData,
        })
    }
}

impl<C> PluginAudioPortsImpl for ChassisMainThread<C>
where
    C: ClapStereoEffect,
{
    fn count(&mut self, is_input: bool) -> u32 {
        u32::from(if is_input {
            C::default()
                .audio_ports()
                .iter()
                .any(|port| port.key() == MAIN_INPUT)
        } else {
            C::default()
                .audio_ports()
                .iter()
                .any(|port| port.key() == MAIN_OUTPUT)
        })
    }

    fn get(&mut self, index: u32, is_input: bool, writer: &mut AudioPortInfoWriter) {
        if index != 0 {
            return;
        }
        let id = ClapId::new(0);
        let name = if is_input { "Main Input" } else { "Main Output" };
        let flags = if is_input {
            AudioPortFlags::IS_MAIN | AudioPortFlags::SUPPORTS_64BITS
        } else {
            AudioPortFlags::IS_MAIN | AudioPortFlags::SUPPORTS_64BITS
        };
        writer.set(&AudioPortInfo {
            id,
            name,
            channel_count: 2,
            flags,
            port_type: Some(AudioPortType::STEREO),
            in_place_pair: Some(id),
        });
    }
}

impl<C> PluginMainThreadParams for ChassisMainThread<C>
where
    C: ClapStereoEffect,
{
    fn count(&mut self) -> u32 {
        u32::try_from(C::default().parameter_descriptors().len()).unwrap_or(u32::MAX)
    }

    fn get_info(&mut self, param_index: u32, writer: &mut ParamInfo) {
        let Ok(index) = usize::try_from(param_index) else {
            return;
        };
        let Some(binding) = C::default()
            .parameter_descriptors()
            .get(index)
            .and_then(|descriptor| {
                C::clap_parameter_ids()
                    .iter()
                    .find(|(key, _)| *key == descriptor.key().as_str())
                    .and_then(|(_, id)| ClapId::from_raw(*id))
                    .map(|id| (descriptor, id))
            })
        else {
            return;
        };
        let descriptor = binding.0;
        let id = binding.1;
        let state = match ClapParameterState::new(
            C::default().parameter_descriptors(),
            C::clap_parameter_ids(),
        ) {
            Ok(state) => state,
            Err(_) => return,
        };
        let Some(binding) = state.bindings().get(index) else {
            return;
        };
        let (min_value, max_value) = binding.plain_range();
        writer.set(&clack_extensions::params::ParamInfo {
            id,
            flags: binding.flags(),
            name: descriptor.name(),
            module: "",
            min_value,
            max_value,
            default_value: binding.default_plain(),
            cookie: clack_extensions::params::ParamCookie::empty(),
        });
    }

    fn get_value(&mut self, param_id: ClapId) -> Option<f64> {
        let state = ClapParameterState::new(
            C::default().parameter_descriptors(),
            C::clap_parameter_ids(),
        )
        .ok()?;
        state
            .bindings()
            .iter()
            .position(|binding| binding.id() == param_id)
            .map(|index| self.shared_value(&state, index))
    }

    fn value_to_text(
        &mut self,
        param_id: ClapId,
        value: f64,
        writer: &mut ParamDisplayWriter,
    ) -> core::fmt::Result {
        let state = ClapParameterState::new(
            C::default().parameter_descriptors(),
            C::clap_parameter_ids(),
        )
        .map_err(|_| core::fmt::Error)?;
        let Some(binding) = state.bindings().iter().find(|binding| binding.id() == param_id) else {
            return Err(core::fmt::Error);
        };
        match binding.descriptor().kind() {
            chassis_core::parameters::ParameterKind::Float { .. } => write!(writer, "{value}"),
            chassis_core::parameters::ParameterKind::Integer { .. } => {
                let Some(chassis_core::parameters::ParameterValue::Integer(value)) =
                    binding.parameter_value(value)
                else {
                    return Err(core::fmt::Error);
                };
                write!(writer, "{value}")
            }
            chassis_core::parameters::ParameterKind::Boolean { .. } => {
                let Some(chassis_core::parameters::ParameterValue::Boolean(value)) =
                    binding.parameter_value(value)
                else {
                    return Err(core::fmt::Error);
                };
                writer.write_str(if value { "On" } else { "Off" })
            }
            chassis_core::parameters::ParameterKind::Choice { .. } => Err(core::fmt::Error),
        }
    }

    fn text_to_value(&mut self, param_id: ClapId, text: &str) -> Option<f64> {
        let state = ClapParameterState::new(
            C::default().parameter_descriptors(),
            C::clap_parameter_ids(),
        )
        .ok()?;
        let binding = state.bindings().iter().find(|binding| binding.id() == param_id)?;
        match binding.descriptor().kind() {
            chassis_core::parameters::ParameterKind::Float { .. } => text.parse().ok(),
            chassis_core::parameters::ParameterKind::Integer { .. } => {
                let value: i64 = text.parse().ok()?;
                binding.parameter_value(value as f64)?;
                Some(value as f64)
            }
            chassis_core::parameters::ParameterKind::Boolean { .. } => match text {
                "On" | "on" | "1" => Some(1.0),
                "Off" | "off" | "0" => Some(0.0),
                _ => None,
            },
            chassis_core::parameters::ParameterKind::Choice { .. } => None,
        }
    }

    fn flush(&mut self, input: &InputEvents, _output: &mut OutputEvents) {
        self.shared.parameters.apply_input(input);
    }
}

impl<C> ChassisMainThread<C>
where
    C: ClapStereoEffect,
{
    fn shared_value(&self, state: &ClapParameterState, index: usize) -> f64 {
        let _ = state;
        self.shared.parameters.value(index)
    }
}

impl<C> PluginStateImpl for ChassisMainThread<C>
where
    C: ClapStereoEffect,
{
    fn save(&self, output: &mut OutputStream) -> Result<(), PluginError> {
        let encoded = self
            .shared
            .parameters
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
            .parameters
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
        };
        processor.shared.parameters.request_sync();
        processor.sync_parameters()?;
        processor
            .runtime
            .activate(
                &main_thread.component,
                process,
                DEFAULT_EFFECT_CONFIGURATION,
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
        if audio.input_port_count() != 1 || audio.output_port_count() != 1 {
            return Err(PluginError::Message(
                "Chassis stereo proof requires exactly one input and one output port",
            ));
        }

        let mut main = audio
            .port_pair(0)
            .ok_or(PluginError::Message("Missing CLAP main audio port pair"))?;
        let frame_count = main.frames_count();
        let channels = main.channels()?.into_f32().ok_or(PluginError::Message(
            "Chassis stereo proof requires f32 audio",
        ))?;

        if channels.channel_pair_count() != 2 {
            return Err(PluginError::Message(
                "Chassis stereo proof requires exactly two main channels",
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

        let mut channels = channels.into_iter();
        let left = channels
            .next()
            .ok_or(PluginError::Message("Missing left main channel"))?;
        let right = channels
            .next()
            .ok_or(PluginError::Message("Missing right main channel"))?;

        let mut buffers = [
            map_main_channel(left, 0, frame_count)?,
            map_main_channel(right, 1, frame_count)?,
        ];

        let context = ProcessContext::new(ProcessMode::Realtime, transport, parameter_events);
        self.runtime
            .process(frame_count, context, &mut buffers)
            .map_err(|_| PluginError::Message("Invalid Chassis process block"))?;
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
    fn maps_transport_flags_and_rejects_non_finite_tempo() {
        let mapped = map_transport(Some(&transport(
            TransportFlags::IS_PLAYING | TransportFlags::HAS_TEMPO,
            124.0,
        )))
        .expect("transport maps");
        assert_eq!(mapped.is_playing(), Some(true));
        assert_eq!(mapped.is_recording(), Some(false));
        assert_eq!(mapped.tempo_bpm(), Some(124.0));

        assert!(
            map_transport(Some(&transport(TransportFlags::HAS_TEMPO, f64::NAN))).is_err()
        );
    }
}
