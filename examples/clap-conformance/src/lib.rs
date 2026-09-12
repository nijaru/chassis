//! Deterministic exported CLAP component used to qualify Chassis adapter semantics.

use core::convert::Infallible;

use chassis_clap::{ClapStereoEffect, SingleComponentEntryWithF64, clack_export_entry};
use chassis_core::{
    buffer::BufferRelationship,
    parameters::{ChoiceOption, ParameterDescriptor, ParameterValue},
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

    fn schema(
        &self,
    ) -> Result<chassis_core::schema::ComponentSchema, chassis_core::schema::ComponentSchemaError>
    {
        chassis_core::schema::ComponentSchema::stereo_effect_with_state(
            chassis_core::schema::ComponentId::new(
                "org.nijaru.chassis.semantic.examples.clap-conformance.src.lib.conformanceeffect",
            )
            .expect("semantic component identity is valid"),
            chassis_core::schema::StateSchemaVersion::new(1),
            self.parameters.clone(),
        )
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
        let trim_index = block
            .parameters()
            .index("trim")
            .expect("conformance trim parameter exists");
        let base_trim = match block.parameters().get("trim") {
            Some(ParameterValue::Float(value)) => *value,
            _ => panic!("conformance trim parameter is a float"),
        };
        let events = block.parameter_events();

        for mut buffer in block.channels() {
            match buffer.relationship() {
                BufferRelationship::InPlace | BufferRelationship::Separate => {
                    let mut trim = events
                        .float_cursor(trim_index, base_trim)
                        .expect("conformance trim base is finite");
                    for (offset, sample) in buffer
                        .make_in_place()
                        .expect("paired main channel has input and output")
                        .iter_mut()
                        .enumerate()
                    {
                        let gain = trim
                            .value_at(u32::try_from(offset).expect("callback offset fits u32"))
                            .expect("callback offset is within the process block");
                        apply_trim_f32(sample, gain);
                    }
                }
                BufferRelationship::InputOnly => {}
                BufferRelationship::OutputOnly => {
                    panic!("conformance effect does not declare output-only channels");
                }
            }
        }
    }
}

impl Process<f64> for ConformanceProcessor {
    fn process<B>(&mut self, block: &mut ProcessBlock<'_, '_, '_, f64, B>)
    where
        B: ProcessBufferSource<f64> + ?Sized,
    {
        let trim_index = block
            .parameters()
            .index("trim")
            .expect("conformance trim parameter exists");
        let base_trim = match block.parameters().get("trim") {
            Some(ParameterValue::Float(value)) => *value,
            _ => panic!("conformance trim parameter is a float"),
        };
        let events = block.parameter_events();

        for mut buffer in block.channels() {
            match buffer.relationship() {
                BufferRelationship::InPlace | BufferRelationship::Separate => {
                    let mut trim = events
                        .float_cursor(trim_index, base_trim)
                        .expect("conformance trim base is finite");
                    for (offset, sample) in buffer
                        .make_in_place()
                        .expect("paired main channel has input and output")
                        .iter_mut()
                        .enumerate()
                    {
                        let gain = trim
                            .value_at(u32::try_from(offset).expect("callback offset fits u32"))
                            .expect("callback offset is within the process block");
                        apply_trim_f64(sample, gain);
                    }
                }
                BufferRelationship::InputOnly => {}
                BufferRelationship::OutputOnly => {
                    panic!("conformance effect does not declare output-only channels");
                }
            }
        }
    }
}

clack_export_entry!(SingleComponentEntryWithF64<ConformanceEffect>);

fn apply_trim_f32(sample: &mut f32, gain: f64) {
    #[allow(clippy::cast_possible_truncation)]
    let gained = *sample * gain as f32;
    *sample = if gained.is_subnormal() { 0.0 } else { gained };
}

fn apply_trim_f64(sample: &mut f64, gain: f64) {
    let gained = *sample * gain;
    *sample = if gained.is_subnormal() { 0.0 } else { gained };
}

#[cfg(test)]
mod tests {
    use super::{apply_trim_f32, apply_trim_f64};

    #[test]
    fn f32_trim_flushes_subnormal_output() {
        let mut sample = f32::from_bits(1);
        apply_trim_f32(&mut sample, 0.5);
        assert_eq!(sample.to_bits(), 0.0_f32.to_bits());
    }

    #[test]
    fn f32_trim_uses_supplied_gain() {
        let mut sample = 0.8_f32;
        apply_trim_f32(&mut sample, 0.25);
        assert_eq!(sample.to_bits(), 0.2_f32.to_bits());
    }

    #[test]
    fn f64_trim_uses_supplied_gain() {
        let mut sample = 0.8_f64;
        apply_trim_f64(&mut sample, 0.25);
        assert_eq!(sample.to_bits(), 0.2_f64.to_bits());
    }
}
