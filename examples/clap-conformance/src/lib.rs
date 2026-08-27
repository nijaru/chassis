//! Minimal exported CLAP component used to validate the first Chassis adapter slice.

use core::convert::Infallible;

use chassis_clap::{ClapStereoEffect, SingleComponentEntry, clack_export_entry};
use chassis_core::{
    process::{ActivationConfig, ProcessBlock},
    runtime::{Component, Process, Processor},
};

#[derive(Default)]
struct ConformanceEffect;

impl Component for ConformanceEffect {
    type Processor = ConformanceProcessor;
    type ActivationError = Infallible;

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
}

struct ConformanceProcessor;

impl Processor for ConformanceProcessor {}

impl Process<f32> for ConformanceProcessor {
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, f32>) {
        for buffer in block.buffers_mut() {
            for sample in buffer
                .make_in_place()
                .expect("the CLAP proof exposes paired main input/output channels")
            {
                *sample *= 0.5;
            }
        }
    }
}

clack_export_entry!(SingleComponentEntry<ConformanceEffect>);
