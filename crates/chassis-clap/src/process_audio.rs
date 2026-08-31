//! Allocation-free CLAP process-buffer projection.
//!
//! Callback audio is validated before product DSP is entered, then exposed as a
//! reiterable [`ProcessBufferSource`] whose channel items wrap Clack's safe
//! [`ChannelPair`] relationship directly. Setup-owned process slots distinguish
//! true semantic in-place pairs from unrelated ports that merely share the same
//! dense CLAP index. No callback-owned `ChannelBuffer` collection is materialized.

use core::{marker::PhantomData, slice};

use chassis_core::{
    buffer::{BufferAccessError, BufferRelationship, InputEndpoint, OutputEndpoint},
    process::{ProcessBlockError, ProcessBufferSource, ProcessChannel},
};
use clack_plugin::{
    prelude::{Audio, ChannelPair, PluginError},
    process::audio::{PairedChannels, PairedChannelsIter, PortPair, PortPairsIter},
};

use crate::audio::{ClapProcessPort, ClapProcessSlot};

/// The sample precision present in one CLAP process callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClapSamplePrecision {
    F32,
    F64,
}

/// One callback's validated, allocation-free CLAP buffer source.
pub(crate) struct ClapBufferSource<'audio, 'plan, S> {
    audio: Audio<'audio>,
    slots: &'plan [ClapProcessSlot],
    frame_count: usize,
    _sample: PhantomData<fn() -> S>,
}

impl<'audio, 'plan, S> ClapBufferSource<'audio, 'plan, S>
where
    S: ClapSample,
{
    pub(crate) fn new(
        mut audio: Audio<'audio>,
        slots: &'plan [ClapProcessSlot],
    ) -> Result<Self, PluginError> {
        validate_audio::<S>(&mut audio, slots)?;
        let frame_count = usize::try_from(audio.frames_count())
            .map_err(|_| PluginError::Message("CLAP frame count is not representable"))?;
        Ok(Self {
            audio,
            slots,
            frame_count,
            _sample: PhantomData,
        })
    }

    pub(crate) fn frame_count(&self) -> u32 {
        self.audio.frames_count()
    }
}

impl<S> ProcessBufferSource<S> for ClapBufferSource<'_, '_, S>
where
    S: ClapSample,
{
    type Channel<'a>
        = ClapChannel<'a, S>
    where
        Self: 'a;

    type Channels<'a>
        = ClapChannels<'a, S>
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
            _sample: PhantomData,
        }
    }
}

/// Classify and validate the one precision used by all mapped CLAP ports.
///
/// CLAP allows each port to advertise either precision, but Chassis dispatches
/// one sample type for the complete process block. A host-provided `Both` value
/// is rejected rather than silently selecting one of two mutable views.
pub(crate) fn sample_precision(
    audio: &mut Audio<'_>,
    slots: &[ClapProcessSlot],
) -> Result<ClapSamplePrecision, PluginError> {
    let expected_inputs = slots.iter().filter(|slot| slot.input.is_some()).count();
    let expected_outputs = slots.iter().filter(|slot| slot.output.is_some()).count();
    if audio.input_port_count() != expected_inputs || audio.output_port_count() != expected_outputs
    {
        return Err(PluginError::Message(
            "CLAP audio buffers do not match the activated Chassis port mapping",
        ));
    }

    let mut precision = None;
    let mut ports = audio.port_pairs();
    for _slot in slots {
        let mut port = ports
            .next()
            .ok_or(PluginError::Message("Missing mapped CLAP audio port"))?;
        let channels = port
            .channels()
            .map_err(|_| PluginError::Message("Invalid CLAP audio buffer representation"))?;
        let current = match (channels.as_f32().is_some(), channels.as_f64().is_some()) {
            (true, false) => ClapSamplePrecision::F32,
            (false, true) => ClapSamplePrecision::F64,
            (true, true) => {
                return Err(PluginError::Message(
                    "CLAP supplied both f32 and f64 audio for one port",
                ));
            }
            (false, false) => {
                return Err(PluginError::Message(
                    "CLAP audio port has no supported sample representation",
                ));
            }
        };
        if precision
            .replace(current)
            .is_some_and(|previous| previous != current)
        {
            return Err(PluginError::Message(
                "CLAP audio ports use mixed sample representations",
            ));
        }
    }

    if ports.next().is_some() {
        return Err(PluginError::Message(
            "CLAP supplied unexpected audio ports for the activated mapping",
        ));
    }
    precision.ok_or(PluginError::Message(
        "CLAP process mapping contains no audio ports",
    ))
}

struct CurrentPortChannels<'a, S> {
    channels: PairedChannelsIter<'a, S>,
    slot: ClapProcessSlot,
    next_channel: u32,
}

/// Allocation-free traversal of every semantic Chassis channel in one callback.
pub(crate) struct ClapChannels<'a, S> {
    ports: PortPairsIter<'a>,
    slots: slice::Iter<'a, ClapProcessSlot>,
    current: Option<CurrentPortChannels<'a, S>>,
    pending: Option<ClapChannel<'a, S>>,
    frame_count: usize,
    _sample: PhantomData<fn() -> S>,
}

impl<'a, S> Iterator for ClapChannels<'a, S>
where
    S: ClapSample,
{
    type Item = ClapChannel<'a, S>;

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
                channels: S::channels(&mut port)
                    .expect("validated CLAP port remains structurally valid and correctly typed")
                    .into_iter(),
                slot,
                next_channel: 0,
            });
        }
    }
}

/// Safe Chassis process channel backed directly by one Clack channel relationship.
pub(crate) struct ClapChannel<'a, S> {
    pair: ChannelPair<'a, S>,
    input: Option<InputEndpoint>,
    output: Option<OutputEndpoint>,
    frame_count: usize,
}

impl<S> ProcessChannel<S> for ClapChannel<'_, S>
where
    S: ClapSample,
{
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

    fn input(&self) -> Option<&[S]> {
        match &self.pair {
            ChannelPair::InputOnly(samples) | ChannelPair::InputOutput(samples, _) => {
                Some(*samples)
            }
            ChannelPair::InPlace(samples) => Some(&**samples),
            ChannelPair::OutputOnly(_) => None,
        }
    }

    fn output_mut(&mut self) -> Option<&mut [S]> {
        match &mut self.pair {
            ChannelPair::OutputOnly(samples)
            | ChannelPair::InputOutput(_, samples)
            | ChannelPair::InPlace(samples) => Some(&mut **samples),
            ChannelPair::InputOnly(_) => None,
        }
    }

    fn make_in_place(&mut self) -> Result<&mut [S], BufferAccessError> {
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

pub(crate) trait ClapSample: Sized + Copy {
    fn channels<'a>(port: &mut PortPair<'a>) -> Result<PairedChannels<'a, Self>, PluginError>;
}

impl ClapSample for f32 {
    fn channels<'a>(port: &mut PortPair<'a>) -> Result<PairedChannels<'a, Self>, PluginError> {
        let channels = port
            .channels()
            .map_err(|_| PluginError::Message("Invalid CLAP audio buffer representation"))?;
        channels.into_f32().ok_or(PluginError::Message(
            "CLAP audio port does not provide f32 samples",
        ))
    }
}

impl ClapSample for f64 {
    fn channels<'a>(port: &mut PortPair<'a>) -> Result<PairedChannels<'a, Self>, PluginError> {
        let channels = port
            .channels()
            .map_err(|_| PluginError::Message("Invalid CLAP audio buffer representation"))?;
        channels.into_f64().ok_or(PluginError::Message(
            "CLAP audio port does not provide f64 samples",
        ))
    }
}

fn map_channel<'a, S>(
    slot: ClapProcessSlot,
    pair: ChannelPair<'a, S>,
    channel: u32,
    frame_count: usize,
    pending: &mut Option<ClapChannel<'a, S>>,
) -> ClapChannel<'a, S> {
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

fn validate_audio<S>(audio: &mut Audio<'_>, slots: &[ClapProcessSlot]) -> Result<(), PluginError>
where
    S: ClapSample,
{
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
        let channels = S::channels(&mut port)?;
        if channels.input_channel_count() != mapped_channel_count(slot.input)?
            || channels.output_channel_count() != mapped_channel_count(slot.output)?
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

fn validate_channel_relationship<S>(
    slot: ClapProcessSlot,
    pair: &ChannelPair<'_, S>,
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
