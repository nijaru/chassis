//! Allocation-free CLAP process-buffer projection.
//!
//! Callback audio is validated before product DSP is entered, then exposed as a
//! reiterable [`ProcessBufferSource`] whose channel items wrap Clack's safe
//! [`ChannelPair`] relationship directly. Setup-owned process slots distinguish
//! true semantic in-place pairs from unrelated ports that merely share the same
//! dense CLAP index. No callback-owned `ChannelBuffer` collection is materialized.

use core::slice;

use chassis_core::{
    buffer::{BufferAccessError, BufferRelationship, InputEndpoint, OutputEndpoint},
    process::{ProcessBlockError, ProcessBufferSource, ProcessChannel},
};
use clack_plugin::{
    prelude::{Audio, ChannelPair, PluginError},
    process::audio::{PairedChannelsIter, PortPair, PortPairsIter},
};

use crate::audio::{ClapProcessPort, ClapProcessSlot};

/// One callback's validated, allocation-free CLAP buffer source.
pub(crate) struct ClapBufferSource<'audio, 'plan> {
    audio: Audio<'audio>,
    slots: &'plan [ClapProcessSlot],
    frame_count: usize,
}

impl<'audio, 'plan> ClapBufferSource<'audio, 'plan> {
    pub(crate) fn new(
        mut audio: Audio<'audio>,
        slots: &'plan [ClapProcessSlot],
    ) -> Result<Self, PluginError> {
        validate_audio(&mut audio, slots)?;
        let frame_count = usize::try_from(audio.frames_count())
            .map_err(|_| PluginError::Message("CLAP frame count is not representable"))?;
        Ok(Self {
            audio,
            slots,
            frame_count,
        })
    }

    pub(crate) fn frame_count(&self) -> u32 {
        self.audio.frames_count()
    }
}

impl ProcessBufferSource<f32> for ClapBufferSource<'_, '_> {
    type Channel<'a>
        = ClapChannel<'a>
    where
        Self: 'a;

    type Channels<'a>
        = ClapChannels<'a>
    where
        Self: 'a;

    fn validate_frame_count(&mut self, expected: usize) -> Result<(), ProcessBlockError> {
        if self.frame_count != expected {
            return Err(ProcessBlockError::BufferFrameCountMismatch {
                expected,
                actual: self.frame_count,
            });
        }
        Ok(())
    }

    fn channels(&mut self) -> Self::Channels<'_> {
        ClapChannels {
            ports: self.audio.port_pairs(),
            slots: self.slots.iter(),
            current: None,
            pending: None,
            frame_count: self.frame_count,
        }
    }
}

struct CurrentPortChannels<'a> {
    channels: PairedChannelsIter<'a, f32>,
    slot: ClapProcessSlot,
    next_channel: u32,
}

/// Allocation-free traversal of every semantic Chassis channel in one callback.
pub(crate) struct ClapChannels<'a> {
    ports: PortPairsIter<'a>,
    slots: slice::Iter<'a, ClapProcessSlot>,
    current: Option<CurrentPortChannels<'a>>,
    pending: Option<ClapChannel<'a>>,
    frame_count: usize,
}

impl<'a> Iterator for ClapChannels<'a> {
    type Item = ClapChannel<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(pending) = self.pending.take() {
            return Some(pending);
        }

        loop {
            if let Some(current) = self.current.as_mut() {
                if let Some(pair) = current.channels.next() {
                    let channel = current.next_channel;
                    current.next_channel = current.next_channel.saturating_add(1);
                    return Some(map_channel(
                        current.slot,
                        pair,
                        channel,
                        self.frame_count,
                        &mut self.pending,
                    ));
                }
                self.current = None;
            }

            let slot = *self.slots.next()?;
            let mut port = self
                .ports
                .next()
                .expect("validated CLAP process slot has a matching dense port");
            self.current = Some(CurrentPortChannels {
                channels: validated_f32_channels(&mut port),
                slot,
                next_channel: 0,
            });
        }
    }
}

/// Safe Chassis process channel backed directly by one Clack channel relationship.
pub(crate) struct ClapChannel<'a> {
    pair: ChannelPair<'a, f32>,
    input: Option<InputEndpoint>,
    output: Option<OutputEndpoint>,
    frame_count: usize,
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
        self.input
    }

    fn output_endpoint(&self) -> Option<OutputEndpoint> {
        self.output
    }

    fn frame_count(&self) -> usize {
        self.frame_count
    }

    fn input(&self) -> Option<&[f32]> {
        match &self.pair {
            ChannelPair::InputOnly(samples) | ChannelPair::InputOutput(samples, _) => {
                Some(*samples)
            }
            ChannelPair::InPlace(samples) => Some(&**samples),
            ChannelPair::OutputOnly(_) => None,
        }
    }

    fn output_mut(&mut self) -> Option<&mut [f32]> {
        match &mut self.pair {
            ChannelPair::OutputOnly(samples)
            | ChannelPair::InputOutput(_, samples)
            | ChannelPair::InPlace(samples) => Some(&mut **samples),
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

fn map_channel<'a>(
    slot: ClapProcessSlot,
    pair: ChannelPair<'a, f32>,
    channel: u32,
    frame_count: usize,
    pending: &mut Option<ClapChannel<'a>>,
) -> ClapChannel<'a> {
    if slot.paired {
        return ClapChannel {
            pair,
            input: slot.input.map(|port| input_endpoint(port, channel)),
            output: slot.output.map(|port| output_endpoint(port, channel)),
            frame_count,
        };
    }

    match pair {
        ChannelPair::InputOutput(input, output) => {
            *pending = Some(ClapChannel {
                pair: ChannelPair::OutputOnly(output),
                input: None,
                output: slot.output.map(|port| output_endpoint(port, channel)),
                frame_count,
            });
            ClapChannel {
                pair: ChannelPair::InputOnly(input),
                input: slot.input.map(|port| input_endpoint(port, channel)),
                output: None,
                frame_count,
            }
        }
        ChannelPair::InputOnly(input) => ClapChannel {
            pair: ChannelPair::InputOnly(input),
            input: slot.input.map(|port| input_endpoint(port, channel)),
            output: None,
            frame_count,
        },
        ChannelPair::OutputOnly(output) => ClapChannel {
            pair: ChannelPair::OutputOnly(output),
            input: None,
            output: slot.output.map(|port| output_endpoint(port, channel)),
            frame_count,
        },
        ChannelPair::InPlace(_) => {
            unreachable!("validation rejects in-place aliasing between unrelated CLAP ports")
        }
    }
}

fn input_endpoint(port: ClapProcessPort, channel: u32) -> InputEndpoint {
    InputEndpoint::new(port.key, channel)
}

fn output_endpoint(port: ClapProcessPort, channel: u32) -> OutputEndpoint {
    OutputEndpoint::new(port.key, channel)
}

fn validate_audio(audio: &mut Audio<'_>, slots: &[ClapProcessSlot]) -> Result<(), PluginError> {
    let expected_inputs = slots.iter().filter(|slot| slot.input.is_some()).count();
    let expected_outputs = slots.iter().filter(|slot| slot.output.is_some()).count();
    if audio.input_port_count() != expected_inputs || audio.output_port_count() != expected_outputs
    {
        return Err(PluginError::Message(
            "CLAP audio buffers do not match the activated Chassis port mapping",
        ));
    }

    let mut ports = audio.port_pairs();
    for slot in slots {
        let mut port = ports
            .next()
            .ok_or(PluginError::Message("Missing mapped CLAP audio port"))?;
        let channels = port.channels()?.into_f32().ok_or(PluginError::Message(
            "Chassis CLAP processing currently requires f32 audio on every mapped port",
        ))?;
        let expected_input_channels = mapped_channel_count(slot.input)?;
        let expected_output_channels = mapped_channel_count(slot.output)?;
        if channels.input_channel_count() != expected_input_channels
            || channels.output_channel_count() != expected_output_channels
        {
            return Err(PluginError::Message(
                "CLAP channel layout does not match the activated Chassis port mapping",
            ));
        }
        for pair in channels {
            validate_channel_relationship(*slot, &pair)?;
        }
    }

    if ports.next().is_some() {
        return Err(PluginError::Message(
            "CLAP supplied unexpected audio ports for the activated mapping",
        ));
    }
    Ok(())
}

fn mapped_channel_count(port: Option<ClapProcessPort>) -> Result<usize, PluginError> {
    port.map_or(Ok(0), |port| {
        usize::try_from(port.layout.channel_count())
            .map_err(|_| PluginError::Message("Mapped CLAP channel count is not representable"))
    })
}

fn validate_channel_relationship(
    slot: ClapProcessSlot,
    pair: &ChannelPair<'_, f32>,
) -> Result<(), PluginError> {
    if slot.paired {
        if matches!(
            pair,
            ChannelPair::InputOutput(_, _) | ChannelPair::InPlace(_)
        ) {
            return Ok(());
        }
        return Err(PluginError::Message(
            "Mapped CLAP in-place pair is missing an input or output channel",
        ));
    }

    match pair {
        ChannelPair::InPlace(_) => Err(PluginError::Message(
            "CLAP host aliased audio ports that are not declared in-place partners",
        )),
        ChannelPair::InputOnly(_) if slot.input.is_none() => Err(PluginError::Message(
            "CLAP supplied an unexpected input channel for this process slot",
        )),
        ChannelPair::OutputOnly(_) if slot.output.is_none() => Err(PluginError::Message(
            "CLAP supplied an unexpected output channel for this process slot",
        )),
        ChannelPair::InputOutput(_, _) if slot.input.is_none() || slot.output.is_none() => {
            Err(PluginError::Message(
                "CLAP supplied an unexpected paired channel for this process slot",
            ))
        }
        ChannelPair::InputOnly(_) | ChannelPair::OutputOnly(_) | ChannelPair::InputOutput(_, _) => {
            Ok(())
        }
    }
}

fn validated_f32_channels<'a>(port: &mut PortPair<'a>) -> PairedChannelsIter<'a, f32> {
    // `ClapBufferSource::new` validated this same callback descriptor set before
    // product processing. Chassis never mutates CLAP data pointers or sample-type
    // fields between validation and traversal.
    port.channels()
        .expect("validated CLAP port remains structurally valid")
        .into_f32()
        .expect("validated CLAP port remains f32")
        .into_iter()
}
