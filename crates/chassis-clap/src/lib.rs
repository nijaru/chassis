//! Initial CLAP deployment adapter for Chassis.
//!
//! This pre-alpha slice intentionally proves only the default stereo effect path:
//! one stereo main input, one stereo main output, `f32`, realtime processing, and
//! no parameters/state/events yet. The limitation is explicit so CLAP semantics
//! do not leak into `chassis-core` merely to make the first export compile.

use core::{marker::PhantomData, num::NonZeroU32};

use chassis_core::{
    audio::{DEFAULT_EFFECT_CONFIGURATION, MAIN_INPUT, MAIN_OUTPUT},
    buffer::{ChannelBuffer, InputEndpoint, OutputEndpoint},
    process::{ProcessConfig, ProcessMode},
    runtime::{Activated, Component, Process as ChassisProcess, activate},
};
use clack_extensions::audio_ports::{
    AudioPortFlags, AudioPortInfo, AudioPortInfoWriter, AudioPortType, PluginAudioPorts,
    PluginAudioPortsImpl,
};
use clack_plugin::{
    entry::{DefaultPluginFactory, SinglePluginEntry},
    plugin::features::{AUDIO_EFFECT, STEREO},
    prelude::{
        Audio, ChannelPair, ClapId, Events, HostAudioProcessorHandle, HostMainThreadHandle,
        HostSharedHandle, Plugin, PluginAudioConfiguration, PluginAudioProcessor, PluginDescriptor,
        PluginError, PluginExtensions, PluginMainThread, Process as ClapProcess, ProcessStatus,
    },
};

/// Re-export of Clack's CLAP entry macro for Chassis export crates.
pub use clack_plugin::clack_export_entry;

/// Temporary CLAP metadata contract for the first default-stereo adapter proof.
///
/// This is intentionally narrower than the eventual Chassis identity/capability
/// model. It exists only until format-independent product metadata and generated
/// export identity are implemented.
pub trait ClapStereoEffect: Component + Default + 'static {
    /// Stable reverse-domain CLAP plugin identifier.
    const CLAP_ID: &'static str;

    /// Human-readable CLAP plugin name.
    const CLAP_NAME: &'static str;
}

/// Clack plugin marker that exports one [`ClapStereoEffect`] through Chassis.
pub struct ChassisPlugin<C>(PhantomData<fn() -> C>);

/// Single-plugin CLAP entry type for a Chassis stereo effect.
pub type SingleComponentEntry<C> = SinglePluginEntry<ChassisPlugin<C>>;

/// Main-thread owner of the format-independent Chassis component definition.
pub struct ChassisMainThread<C> {
    component: C,
}

/// CLAP audio-thread wrapper around an activated Chassis processor.
pub struct ChassisAudioProcessor<P> {
    active: Activated<'static, P>,
}

impl<C> Plugin for ChassisPlugin<C>
where
    C: ClapStereoEffect,
    C::Processor: ChassisProcess<f32> + Send + 'static,
{
    type AudioProcessor<'a> = ChassisAudioProcessor<C::Processor>;
    type Shared<'a> = ();
    type MainThread<'a> = ChassisMainThread<C>;

    fn declare_extensions(builder: &mut PluginExtensions<Self>, _shared: Option<&()>) {
        builder.register::<PluginAudioPorts>();
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

    fn new_shared(_host: HostSharedHandle<'_>) -> Result<(), PluginError> {
        Ok(())
    }

    fn new_main_thread<'a>(
        _host: HostMainThreadHandle<'a>,
        _shared: &'a (),
    ) -> Result<ChassisMainThread<C>, PluginError> {
        Ok(ChassisMainThread {
            component: C::default(),
        })
    }
}

impl<'a, C> PluginMainThread<'a, ()> for ChassisMainThread<C> where C: ClapStereoEffect {}

impl<C> PluginAudioPortsImpl for ChassisMainThread<C>
where
    C: ClapStereoEffect,
{
    fn count(&self, _is_input: bool) -> u32 {
        1
    }

    fn get(&self, index: u32, is_input: bool, writer: &mut AudioPortInfoWriter) {
        if index != 0 {
            return;
        }

        writer.set(&AudioPortInfo {
            id: ClapId::new(0),
            name: if is_input {
                b"Main Input"
            } else {
                b"Main Output"
            },
            channel_count: 2,
            flags: AudioPortFlags::IS_MAIN,
            port_type: Some(AudioPortType::STEREO),
            in_place_pair: Some(ClapId::new(0)),
        });
    }
}

impl<'a, C, P> PluginAudioProcessor<'a, (), ChassisMainThread<C>> for ChassisAudioProcessor<P>
where
    C: ClapStereoEffect,
    P: ChassisProcess<f32> + Send + 'static,
{
    fn activate(
        _host: HostAudioProcessorHandle<'a>,
        main_thread: &ChassisMainThread<C>,
        _shared: &'a (),
        audio_config: PluginAudioConfiguration,
    ) -> Result<Self, PluginError> {
        let process = map_process_config(audio_config)?;
        let active = activate(
            &main_thread.component,
            process,
            DEFAULT_EFFECT_CONFIGURATION,
        )
        .map_err(|_| PluginError::Message("Chassis component activation failed"))?;

        Ok(Self { active })
    }

    fn process(
        &mut self,
        _process: ClapProcess,
        mut audio: Audio,
        _events: Events,
    ) -> Result<ProcessStatus, PluginError> {
        if audio.input_port_count() != 1 || audio.output_port_count() != 1 {
            return Err(PluginError::Message(
                "Chassis stereo proof requires exactly one input and one output port",
            ));
        }

        let mut main = audio
            .port_pair(0)
            .ok_or(PluginError::Message("Missing CLAP main audio port pair"))?;
        let frame_count = main.frames_count();
        let channels = main
            .channels()?
            .into_f32()
            .ok_or(PluginError::Message("Chassis stereo proof requires f32 audio"))?;

        if channels.channel_pair_count() != 2 {
            return Err(PluginError::Message(
                "Chassis stereo proof requires exactly two main channels",
            ));
        }

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

        self.active
            .process(frame_count, ProcessMode::Realtime, &mut buffers)
            .map_err(|_| PluginError::Message("Invalid Chassis process block"))?;

        Ok(ProcessStatus::Continue)
    }

    fn deactivate(self, _main_thread: &ChassisMainThread<C>) {
        self.active.deactivate();
    }

    fn reset(&mut self) {
        self.active.reset();
    }
}

fn map_process_config(config: PluginAudioConfiguration) -> Result<ProcessConfig, PluginError> {
    const CLAP_MAX_FRAMES: u32 = 2_147_483_647;

    let minimum = NonZeroU32::new(config.min_frames_count)
        .ok_or(PluginError::Message("CLAP minimum frame count must be positive"))?;
    let maximum = NonZeroU32::new(config.max_frames_count)
        .ok_or(PluginError::Message("CLAP maximum frame count must be positive"))?;

    if config.min_frames_count > CLAP_MAX_FRAMES || config.max_frames_count > CLAP_MAX_FRAMES {
        return Err(PluginError::Message(
            "CLAP frame bounds exceed the specification limit",
        ));
    }

    ProcessConfig::new(config.sample_rate, Some(minimum), maximum)
        .map_err(|_| PluginError::Message("Invalid CLAP process configuration"))
}

fn map_main_channel<'a>(
    pair: ChannelPair<'a, f32>,
    channel: u32,
    frame_count: u32,
) -> Result<ChannelBuffer<'a, f32>, PluginError> {
    let input = InputEndpoint::new(MAIN_INPUT, channel);
    let output = OutputEndpoint::new(MAIN_OUTPUT, channel);

    match pair {
        ChannelPair::InputOutput(input_samples, output_samples) => ChannelBuffer::separate(
            input,
            input_samples,
            output,
            output_samples,
            frame_count,
        )
        .map_err(|_| PluginError::Message("Invalid disjoint CLAP channel buffers")),
        ChannelPair::InPlace(samples) => ChannelBuffer::in_place(input, output, samples, frame_count)
            .map_err(|_| PluginError::Message("Invalid in-place CLAP channel buffer")),
        ChannelPair::InputOnly(_) | ChannelPair::OutputOnly(_) => Err(PluginError::Message(
            "Required CLAP main input/output channel is missing",
        )),
    }
}
