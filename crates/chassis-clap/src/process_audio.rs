//! Allocation-free CLAP process-buffer projection.
//!
//! Callback audio is validated before product DSP is entered, then exposed as a
//! reiterable [`ProcessBufferSource`] whose channel items wrap Clack's safe
//! [`ChannelPair`] relationship directly. No callback-owned `ChannelBuffer`
//! collection is materialized.

use chassis_core::{
    audio::{MAIN_INPUT, MAIN_OUTPUT, SIDECHAIN_INPUT},
    buffer::{BufferAccessError, BufferRelationship, InputEndpoint, OutputEndpoint},
    process::{ProcessBlockError, ProcessBufferSource, ProcessChannel},
};
use clack_plugin::{
    prelude::{Audio, ChannelPair, PluginError},
    process::audio::{PairedChannelsIter, PortPair, PortPairsIter},
};

pub(crate) struct ClapStereoBufferSource<'a> {
    audio: Audio<'a>,
    sidechain_enabled: bool,
}

impl<'a> ClapStereoBufferSource<'a> {
    pub(crate) fn new(
        mut audio: Audio<'a>,
        sidechain_enabled: bool,
    ) -> Result<Self, PluginError> {
        validate_audio(&mut audio, sidechain_enabled)?;
        Ok(Self {
            audio,
            sidechain_enabled,
        })
    }

    pub(crate) fn frame_count(&self) -> u32 {
        self.audio.frames_count()
    }
}

impl ProcessBufferSource<f32> for ClapStereoBufferSource<'_> {
    type Channel<'a>
        = ClapChannel<'a>
    where
        Self: 'a;

    type Channels<'a>
        = ClapStereoChannels<'a>
    where
        Self: 'a;

    fn validate_frame_count(&mut self, expected: usize) -> Result<(), ProcessBlockError> {
        let actual = usize::try_from(self.audio.frames_count())
            .map_err(|_| ProcessBlockError::FrameCountNotRepresentable)?;
        if actual != expected {
            return Err(ProcessBlockError::BufferFrameCountMismatch { expected, actual });
        }
        Ok(())
    }

    fn channels(&mut self) -> Self::Channels<'_> {
        ClapStereoChannels {
            ports: self.audio.port_pairs(),
            current: None,
            next_port: 0,
            sidechain_enabled: self.sidechain_enabled,
        }
    }
}

#[derive(Clone, Copy)]
enum PortKind {
    Main,
    Sidechain,
}

struct CurrentPortChannels<'a> {
    channels: PairedChannelsIter<'a, f32>,
    kind: PortKind,
    next_channel: u32,
}

pub(crate) struct ClapStereoChannels<'a> {
    ports: PortPairsIter<'a>,
    current: Option<CurrentPortChannels<'a>>,
    next_port: u8,
    sidechain_enabled: bool,
}

impl<'a> Iterator for ClapStereoChannels<'a> {
    type Item = ClapChannel<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(current) = self.current.as_mut() {
                if let Some(pair) = current.channels.next() {
                    let channel = current.next_channel;
                    current.next_channel = current.next_channel.saturating_add(1);
                    return Some(match current.kind {
                        PortKind::Main => ClapChannel {
                            pair,
                            input: Some(InputEndpoint::new(MAIN_INPUT, channel)),
                            output: Some(OutputEndpoint::new(MAIN_OUTPUT, channel)),
                        },
                        PortKind::Sidechain => ClapChannel {
                            pair,
                            input: Some(InputEndpoint::new(SIDECHAIN_INPUT, channel)),
                            output: None,
                        },
                    });
                }
                self.current = None;
            }

            let kind = match self.next_port {
                0 => PortKind::Main,
                1 if self.sidechain_enabled => PortKind::Sidechain,
                _ => return None,
            };
            self.next_port = self.next_port.saturating_add(1);
            let mut port = self.ports.next()?;
            self.current = Some(CurrentPortChannels {
                channels: validated_f32_channels(&mut port),
                kind,
                next_channel: 0,
            });
        }
    }
}

/// Safe Chassis process channel backed directly by one Clack channel pair.
pub(crate) struct ClapChannel<'a> {
    pair: ChannelPair<'a, f32>,
    input: Option<InputEndpoint>,
    output: Option<OutputEndpoint>,
}

impl ProcessChannel<f32> for ClapChannel<'_> {
    fn relationship(&self) -> BufferRelationship {
        match &self.pair {
            ChannelPair::InputOnly(_) => BufferRelationship::InputOnly,
            ChannelPair::OutputOnly(_) => BufferRelationship::OutputOnly,
            ChannelPair::InputOutput(_, _) => BufferRelationship::Separate,
            ChannelPair::InPlace(_) => BufferRelationship::InPlace,
        }
    }

    fn input_endpoint(&self) -> Option<InputEndpoint> {
        match &self.pair {
            ChannelPair::InputOnly(_) | ChannelPair::InputOutput(_, _) | ChannelPair::InPlace(_) => {
                self.input
            }
            ChannelPair::OutputOnly(_) => None,
        }
    }

    fn output_endpoint(&self) -> Option<OutputEndpoint> {
        match &self.pair {
            ChannelPair::OutputOnly(_) | ChannelPair::InputOutput(_, _) | ChannelPair::InPlace(_) => {
                self.output
            }
            ChannelPair::InputOnly(_) => None,
        }
    }

    fn frame_count(&self) -> usize {
        match &self.pair {
            ChannelPair::InputOnly(samples) => samples.len(),
            ChannelPair::OutputOnly(samples) => samples.len(),
            ChannelPair::InputOutput(input, _) => input.len(),
            ChannelPair::InPlace(samples) => samples.len(),
        }
    }

    fn input(&self) -> Option<&[f32]> {
        match &self.pair {
            ChannelPair::InputOnly(samples) => Some(samples),
            ChannelPair::InputOutput(samples, _) => Some(samples),
            ChannelPair::InPlace(samples) => Some(samples),
            ChannelPair::OutputOnly(_) => None,
        }
    }

    fn output_mut(&mut self) -> Option<&mut [f32]> {
        match &mut self.pair {
            ChannelPair::OutputOnly(samples) => Some(&mut **samples),
            ChannelPair::InputOutput(_, samples) => Some(&mut **samples),
            ChannelPair::InPlace(samples) => Some(&mut **samples),
            ChannelPair::InputOnly(_) => None,
        }
    }

    fn make_in_place(&mut self) -> Result<&mut [f32], BufferAccessError> {
        match &mut self.pair {
            ChannelPair::InPlace(samples) => Ok(&mut **samples),
            ChannelPair::InputOutput(input, output) => {
                output.copy_from_slice(input);
                Ok(&mut **output)
            }
            ChannelPair::InputOnly(_) => Err(BufferAccessError::MissingOutput),
            ChannelPair::OutputOnly(_) => Err(BufferAccessError::MissingInput),
        }
    }
}

fn validate_audio(audio: &mut Audio<'_>, sidechain_enabled: bool) -> Result<(), PluginError> {
    let expected_inputs = if sidechain_enabled { 2 } else { 1 };
    if audio.input_port_count() != expected_inputs || audio.output_port_count() != 1 {
        return Err(PluginError::Message(
            "CLAP audio buffers do not match the activated Chassis stereo topology",
        ));
    }

    let mut ports = audio.port_pairs();
    let mut main = ports
        .next()
        .ok_or(PluginError::Message("Missing CLAP main audio port pair"))?;
    let main_channels = main.channels()?.into_f32().ok_or(PluginError::Message(
        "Chassis stereo topology requires f32 main audio",
    ))?;
    if main_channels.input_channel_count() != 2 || main_channels.output_channel_count() != 2 {
        return Err(PluginError::Message(
            "Chassis stereo topology requires exactly two main input and output channels",
        ));
    }
    for pair in main_channels {
        if !matches!(pair, ChannelPair::InputOutput(_, _) | ChannelPair::InPlace(_)) {
            return Err(PluginError::Message(
                "Required CLAP main input/output channel is missing",
            ));
        }
    }

    if sidechain_enabled {
        let mut sidechain = ports
            .next()
            .ok_or(PluginError::Message("Missing CLAP sidechain input port"))?;
        let sidechain_channels =
            sidechain
                .channels()?
                .into_f32()
                .ok_or(PluginError::Message(
                    "Chassis stereo sidechain requires f32 audio",
                ))?;
        if sidechain_channels.input_channel_count() != 2
            || sidechain_channels.output_channel_count() != 0
        {
            return Err(PluginError::Message(
                "Chassis stereo sidechain requires exactly two input-only channels",
            ));
        }
        for pair in sidechain_channels {
            if !matches!(pair, ChannelPair::InputOnly(_)) {
                return Err(PluginError::Message(
                    "CLAP sidechain must be an input-only port",
                ));
            }
        }
    }

    if ports.next().is_some() {
        return Err(PluginError::Message(
            "CLAP supplied unexpected audio ports for the activated topology",
        ));
    }
    Ok(())
}

fn validated_f32_channels<'a>(port: &mut PortPair<'a>) -> PairedChannelsIter<'a, f32> {
    // `ClapStereoBufferSource::new` validated this same immutable callback
    // descriptor set before product processing. Chassis never mutates CLAP data
    // pointers or sample-type fields between validation and traversal.
    port.channels()
        .expect("validated CLAP port remains structurally valid")
        .into_f32()
        .expect("validated CLAP port remains f32")
        .into_iter()
}
