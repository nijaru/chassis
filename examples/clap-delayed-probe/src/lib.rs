//! Nonzero-latency exported CLAP component used to qualify host PDC alignment.
//!
//! The processor delays the main stereo pair by exactly 64 samples at/below
//! 48 kHz (128 samples at 88.2 kHz and above) using a swap-through ring
//! buffer, and reports that delay as its activation latency. Hosts that
//! honor plugin latency compensation render the delayed output aligned with
//! the source; comparing against the in-process impulse evidence then proves
//! the PDC path end to end.

use core::convert::Infallible;

use chassis_clap::{ClapComponent, SingleComponentEntry, clack_export_entry};
use chassis_runtime::{
    audio::{AudioPortIndex, MAIN_INPUT, MAIN_OUTPUT},
    process::{ActivationConfig, ProcessBlock, ProcessBufferSource, ProcessChannel},
    runtime::{Component, LatencySamples, Process, Processor},
};

struct DelayedProbeEffect;

impl Default for DelayedProbeEffect {
    fn default() -> Self {
        Self
    }
}

impl Component for DelayedProbeEffect {
    type Processor = DelayedProcessor;
    type ActivationError = Infallible;

    fn schema(
        &self,
    ) -> Result<
        chassis_runtime::schema::ComponentSchema,
        chassis_runtime::schema::ComponentSchemaError,
    > {
        chassis_runtime::schema::ComponentSchema::stereo_effect_with_state(
            chassis_runtime::schema::ComponentId::new("org.nijaru.chassis.semantic.examples.clap-delayed-probe.src.lib.delayedprobeeffect")
                .expect("semantic component identity is valid"),
            chassis_runtime::schema::StateSchemaVersion::new(1),
            vec![],
        )
    }

    fn activate(
        &self,
        config: &ActivationConfig<'_>,
        _parameters: &chassis_runtime::parameters::ParameterStore,
        _custom_state: &[chassis_runtime::state::StateEntry],
    ) -> Result<Self::Processor, Self::ActivationError> {
        let samples = if config.process().sample_rate() >= 88_200.0 {
            128
        } else {
            64
        };
        let length = usize::try_from(samples).expect("probe latency fits usize");
        Ok(DelayedProcessor {
            latency: LatencySamples::new(samples),
            main_input: config
                .schema()
                .audio_port_index(&MAIN_INPUT)
                .expect("default effect schema has main input"),
            main_output: config
                .schema()
                .audio_port_index(&MAIN_OUTPUT)
                .expect("default effect schema has main output"),
            delay: [vec![0.0; length], vec![0.0; length]],
            positions: [0, 0],
        })
    }
}

struct DelayedProcessor {
    latency: LatencySamples,
    main_input: AudioPortIndex,
    main_output: AudioPortIndex,
    delay: [Vec<f32>; 2],
    positions: [usize; 2],
}

impl Processor for DelayedProcessor {
    fn latency(&self) -> LatencySamples {
        self.latency
    }

    fn reset(&mut self) {
        for line in &mut self.delay {
            line.fill(0.0);
        }
        self.positions.fill(0);
    }
}

impl Process<f32> for DelayedProcessor {
    fn process<B>(&mut self, block: &mut ProcessBlock<'_, '_, '_, f32, B>)
    where
        B: ProcessBufferSource<f32> + ?Sized,
    {
        for mut channel in block.channels() {
            let is_main = channel
                .input_endpoint()
                .is_some_and(|endpoint| endpoint.port_index() == self.main_input)
                && channel
                    .output_endpoint()
                    .is_some_and(|endpoint| endpoint.port_index() == self.main_output);
            if !is_main {
                continue;
            }

            let index = usize::try_from(
                channel
                    .output_endpoint()
                    .expect("main channel has an output endpoint")
                    .channel(),
            )
            .expect("channel index fits usize");
            let delay = &mut self.delay[index];
            let position = &mut self.positions[index];
            for sample in channel
                .copy_input_to_output()
                .expect("main channel is an in-place pair")
            {
                std::mem::swap(sample, &mut delay[*position]);
                // Flush subnormal output to zero: denormals can trigger large
                // CPU penalties on some processors, and hosts fuzz with
                // subnormal input. Input subnormals are flushed on the way
                // out of the delay line, so both directions are covered.
                if sample.is_subnormal() {
                    *sample = 0.0;
                }
                *position += 1;
                if *position == delay.len() {
                    *position = 0;
                }
            }
        }
    }
}

impl ClapComponent for DelayedProbeEffect {
    const CLAP_ID: &'static str = "com.nijaru.chassis.delayed-probe";
    const CLAP_NAME: &'static str = "Chassis Delayed Probe";
    const CLAP_FEATURES: &'static [&'static std::ffi::CStr] = &[c"audio-effect", c"stereo"];
    const CLAP_PARAMETER_IDS: &'static [(&'static str, u32)] = &[];
    const CLAP_MAX_PARAMETER_EVENTS: u32 = 64;
}

clack_export_entry!(SingleComponentEntry<DelayedProbeEffect>);
