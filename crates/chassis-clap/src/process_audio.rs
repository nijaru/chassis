//! Lazy safe process views of a completely validated raw CLAP sample topology.

pub(crate) use crate::raw_audio::{ClapSample, ClapSamplePrecision, sample_precision};
use crate::{
    audio::ClapProcessSlot,
    raw_audio::{AudioScratch, RawChannels, RawSource},
};
use chassis_runtime::{
    buffer::{BufferAccessError, BufferRelationship, InputEndpoint, OutputEndpoint},
    process::{ProcessBlockError, ProcessBufferSource, ProcessChannel},
};
use clack_plugin::prelude::{Audio, ChannelPair, PluginError};

pub(crate) struct ClapBufferSource<'audio, 'scratch, S> {
    raw: RawSource<'audio, 'scratch, S>,
}

impl<'audio, 'scratch, S: ClapSample> ClapBufferSource<'audio, 'scratch, S> {
    pub(crate) fn new(
        audio: Audio<'audio>,
        slots: &[ClapProcessSlot],
        scratch: &'scratch mut AudioScratch,
    ) -> Result<Self, PluginError> {
        Ok(Self {
            raw: RawSource::new(audio, slots, scratch)?,
        })
    }
    pub(crate) fn frame_count(&self) -> u32 {
        self.raw.frame_count()
    }
}
impl<S: ClapSample> ProcessBufferSource<S> for ClapBufferSource<'_, '_, S> {
    type Channel<'a>
        = ClapChannel<'a, S>
    where
        Self: 'a;
    type Channels<'a>
        = ClapChannels<'a, S>
    where
        Self: 'a;
    fn validate_frame_count(&mut self, expected: usize) -> Result<(), ProcessBlockError> {
        let actual = self.frame_count() as usize;
        if actual != expected {
            return Err(ProcessBlockError::BufferFrameCountMismatch { expected, actual });
        }
        Ok(())
    }
    fn channels(&mut self) -> Self::Channels<'_> {
        let frame_count = self.raw.frame_count() as usize;
        ClapChannels {
            raw: self.raw.channels(),
            frame_count,
        }
    }
}
pub(crate) struct ClapChannels<'a, S> {
    raw: RawChannels<'a, S>,
    frame_count: usize,
}
impl<'a, S: ClapSample> Iterator for ClapChannels<'a, S> {
    type Item = ClapChannel<'a, S>;
    fn next(&mut self) -> Option<Self::Item> {
        let (pair, input, output) = self.raw.next()?;
        Some(ClapChannel {
            pair,
            input,
            output,
            frame_count: self.frame_count,
        })
    }
}
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
