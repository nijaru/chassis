//! Minimal exported CLAP component used to validate the first Chassis adapter slice.

use core::convert::Infallible;

use chassis_clap::{ClapStereoEffect, SingleComponentEntryWithF64, clack_export_entry};
use chassis_core::{
    buffer::BufferRelationship,
    parameters::{ChoiceOption, ParameterDescriptor},
    process::{ActivationConfig, ProcessBlock, ProcessBufferSource, ProcessChannel},
    runtime::{Component, Process, Processor},
};

struct ConformanceEffect {
    parameters: Vec<ParameterDescriptor>,
}

impl Default for ConformanceEffect {
    fn default() -> Self {
        Self::new()
    }
}

impl ConformanceEffect {
    fn new() -> Self {
        let mode = ParameterDescriptor::choice(
            "mode",
            "Mode",
            vec![
                ChoiceOption::new("clean", "Clean").expect("choice option is valid"),
                ChoiceOption::new("warm", "Warm").expect("choice option is valid"),
            ],
            "clean",
        )
        .expect("choice parameter is valid");
        let trim = ParameterDescriptor::float("trim", "Trim", 0.0, 1.0, 0.5)
            .expect("float parameter is valid");
        Self {
            parameters: vec![mode, trim],
        }
    }
}

impl Component for ConformanceEffect {
    type Processor = ConformanceProcessor;
    type ActivationError = Infallible;

    fn parameter_descriptors(&self) -> &[ParameterDescriptor] {
        &self.parameters
    }

    fn activate(
        &self,
        _config: &ActivationConfig<'_>,
    ) -> Result<Self::Processor, Self::ActivationError> {
        Ok(ConformanceProcessor)
    }
}

impl ClapStereoEffect for ConformanceEffect {
    const CLAP_ID: &'static str = "com.nijaru.chassis.conformance";
    const CLAP_NAME: &'static str = "Chassis Conformance";
    const CLAP_PARAMETER_IDS: &'static [(&'static str, u32)] = &[("mode", 7001), ("trim", 7002)];
    const CLAP_MAX_PARAMETER_EVENTS: u32 = 512;
}

struct ConformanceProcessor;

impl Processor for ConformanceProcessor {}

impl Process<f32> for ConformanceProcessor {
    fn process<B>(&mut self, block: &mut ProcessBlock<'_, '_, '_, f32, B>)
    where
        B: ProcessBufferSource<f32> + ?Sized,
    {
        process_gain(block, apply_gain);
    }
}

impl Process<f64> for ConformanceProcessor {
    fn process<B>(&mut self, block: &mut ProcessBlock<'_, '_, '_, f64, B>)
    where
        B: ProcessBufferSource<f64> + ?Sized,
    {
        process_gain(block, apply_gain_f64);
    }
}

fn process_gain<S, B>(block: &mut ProcessBlock<'_, '_, '_, S, B>, apply_gain: fn(&mut S))
where
    S: Copy,
    B: ProcessBufferSource<S> + ?Sized,
{
    for mut buffer in block.channels() {
        match buffer.relationship() {
            BufferRelationship::InPlace | BufferRelationship::Separate => {
                for sample in buffer
                    .make_in_place()
                    .expect("paired main channel has input and output")
                {
                    apply_gain(sample);
                }
            }
            BufferRelationship::InputOnly => {}
            BufferRelationship::OutputOnly => {
                panic!("conformance effect does not declare output-only channels");
            }
        }
    }
}

clack_export_entry!(SingleComponentEntryWithF64<ConformanceEffect>);

fn apply_gain(sample: &mut f32) {
    let gained = *sample * 0.5;
    *sample = if gained.is_subnormal() { 0.0 } else { gained };
}

fn apply_gain_f64(sample: &mut f64) {
    let gained = *sample * 0.5;
    *sample = if gained.is_subnormal() { 0.0 } else { gained };
}

#[cfg(test)]
mod tests {
    use super::{apply_gain, apply_gain_f64};

    #[test]
    fn gain_flushes_subnormal_output() {
        let mut sample = f32::from_bits(1);
        apply_gain(&mut sample);
        assert_eq!(sample.to_bits(), 0.0_f32.to_bits());
    }

    #[test]
    fn gain_preserves_normal_output() {
        let mut sample = 0.8;
        apply_gain(&mut sample);
        assert_eq!(sample.to_bits(), 0.4_f32.to_bits());
    }

    #[test]
    fn f64_gain_preserves_normal_output() {
        let mut sample = 0.8_f64;
        apply_gain_f64(&mut sample);
        assert_eq!(sample.to_bits(), 0.4_f64.to_bits());
    }
}
