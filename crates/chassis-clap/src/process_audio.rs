//! Lazy safe process views of a completely validated raw CLAP sample topology.

use core::ops::ControlFlow;

pub(crate) use crate::raw_audio::{ClapSample, ClapSamplePrecision, sample_precision};
use crate::{
    audio::ClapProcessSlot,
    raw_audio::{AudioScratch, RawSource},
};
use chassis_runtime::{buffer::ChannelBuffer, process::ProcessBufferSource};
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
    fn visit_channels(
        &mut self,
        visitor: &mut dyn FnMut(ChannelBuffer<'_, S>) -> ControlFlow<()>,
    ) -> ControlFlow<()> {
        for (pair, input, output) in self.raw.channels() {
            // RawSource has already proved the whole alias topology and mapped
            // endpoints. Construct only one short-lived safe view at a time.
            let channel = match pair {
                ChannelPair::InPlace(samples) => ChannelBuffer::InPlace {
                    input: input.expect("in-place slot has an input"),
                    output: output.expect("in-place slot has an output"),
                    samples,
                },
                ChannelPair::InputOutput(input_samples, output_samples) => {
                    ChannelBuffer::Separate {
                        input: input.expect("paired slot has an input"),
                        input_samples,
                        output: output.expect("paired slot has an output"),
                        output_samples,
                    }
                }
                ChannelPair::InputOnly(samples) => ChannelBuffer::InputOnly {
                    input: input.expect("input slot has an input"),
                    samples,
                },
                ChannelPair::OutputOnly(samples) => ChannelBuffer::OutputOnly {
                    output: output.expect("output slot has an output"),
                    samples,
                },
            };
            visitor(channel)?;
        }
        ControlFlow::Continue(())
    }
}
