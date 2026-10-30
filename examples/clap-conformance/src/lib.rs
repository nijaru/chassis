//! Deterministic exported CLAP component used to qualify Chassis adapter semantics.

mod trim_control;

use core::convert::Infallible;

use chassis_clap::{ClapComponent, F32AndF64, SingleComponentEntry, clack_export_entry};
use chassis_runtime::{
    buffer::ChannelBuffer,
    parameters::{BooleanParameter, ChoiceOption, FloatParameter, ParameterDescriptor},
    process::{ActivationConfig, ProcessBlock},
    runtime::{Component, Process, ProcessError, Processor},
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
            .expect("float parameter is valid")
            .with_control_policy(trim_control::TrimControl { precision: 1 })
            .expect("trim control policy is valid")
            .with_global_modulation()
            .expect("trim is a float");
        Self {
            parameters: vec![
                mode,
                trim,
                ParameterDescriptor::boolean("bypass", "Bypass", false)
                    .expect("bypass parameter is valid")
                    .with_bypass()
                    .expect("bypass is Boolean"),
            ],
        }
    }
}

impl Component for ConformanceEffect {
    type Processor = ConformanceProcessor;
    type ActivationError = Infallible;

    fn schema(
        &self,
    ) -> Result<
        chassis_runtime::schema::ComponentSchema,
        chassis_runtime::schema::ComponentSchemaError,
    > {
        chassis_runtime::schema::ComponentSchema::stereo_effect_with_state(
            chassis_runtime::schema::ComponentId::new("org.nijaru.chassis.conformance")
                .expect("semantic component identity is valid"),
            chassis_runtime::schema::StateSchemaVersion::new(1),
            self.parameters.clone(),
        )
    }

    fn activate(
        &self,
        _config: &ActivationConfig<'_>,
        parameters: &chassis_runtime::parameters::ParameterStore,
        _custom_state: &[chassis_runtime::state::StateEntry],
    ) -> Result<Self::Processor, Self::ActivationError> {
        Ok(ConformanceProcessor {
            bypass: parameters
                .boolean_parameter("bypass")
                .expect("bypass is Boolean"),
            trim: parameters.float_parameter("trim").expect("trim is a float"),
        })
    }
}

impl ClapComponent for ConformanceEffect {
    const CLAP_DESCRIPTOR: chassis_clap::ClapDescriptor = chassis_clap::ClapDescriptor {
        features: &[c"audio-effect", c"stereo"],
        vendor: "Nijaru",
        version: env!("CARGO_PKG_VERSION"),
        description: "Deterministic stereo effect for Chassis CLAP conformance qualification",
        ..chassis_clap::ClapDescriptor::new("com.nijaru.chassis.conformance", "Chassis Conformance")
    };
    const CLAP_PARAMETER_IDS: &'static [(&'static str, u32)] =
        &[("mode", 7001), ("trim", 7002), ("bypass", 7003)];
    const CLAP_MAX_PARAMETER_EVENTS: u32 = 512;
}

struct ConformanceProcessor {
    bypass: BooleanParameter,
    trim: FloatParameter,
}

impl Processor for ConformanceProcessor {}

impl ConformanceProcessor {
    fn process_trim<S: Copy>(
        &self,
        block: &mut ProcessBlock<'_, S>,
        apply: impl Fn(&mut S, f64),
    ) -> Result<(), ProcessError> {
        let bypass = self.bypass.cursor(block)?;
        let trim = self.trim.cursor(block)?;
        block.try_for_each_channel(|buffer| {
            let mut bypass = bypass.clone();
            let mut trim = trim.clone();
            let mut process_sample =
                |offset: usize, source: S, destination: &mut S| -> Result<(), ProcessError> {
                    let offset = u32::try_from(offset).expect("callback frame count fits u32");
                    let gain = trim.value_at(offset)?;
                    // Product policy: immediate unity-gain dry path, with the same
                    // subnormal cleanup. This effect has no latency or DSP history.
                    let gain = if bypass.value_at(offset)? { 1.0 } else { gain };
                    *destination = source;
                    apply(destination, gain);
                    Ok(())
                };
            match buffer {
                ChannelBuffer::InPlace { samples, .. } => {
                    for (offset, sample) in samples.iter_mut().enumerate() {
                        process_sample(offset, *sample, sample)?;
                    }
                }
                ChannelBuffer::Separate {
                    input_samples,
                    output_samples,
                    ..
                } => {
                    for (offset, (input, output)) in
                        input_samples.iter().zip(output_samples).enumerate()
                    {
                        process_sample(offset, *input, output)?;
                    }
                }
                ChannelBuffer::InputOnly { .. } => {}
                ChannelBuffer::OutputOnly { .. } => {
                    return Err(ProcessError::Message("effect output requires an input"));
                }
            }
            Ok(())
        })
    }
}

impl Process<f32> for ConformanceProcessor {
    fn process(
        &mut self,
        block: &mut ProcessBlock<'_, f32>,
    ) -> Result<(), chassis_runtime::runtime::ProcessError> {
        self.process_trim(block, apply_trim_f32)
    }
}

impl Process<f64> for ConformanceProcessor {
    fn process(
        &mut self,
        block: &mut ProcessBlock<'_, f64>,
    ) -> Result<(), chassis_runtime::runtime::ProcessError> {
        self.process_trim(block, apply_trim_f64)
    }
}

clack_export_entry!(SingleComponentEntry<ConformanceEffect, F32AndF64>);

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
        // The product must flush a nonzero subnormal result, not merely let
        // the multiplication underflow to zero on its own.
        let mut sample = f32::from_bits(4);
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
