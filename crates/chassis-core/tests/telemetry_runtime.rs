//! External telemetry fixture through the public Chassis runtime.

use std::{convert::Infallible, num::NonZeroU32, sync::Arc};

use chassis_core::{
    audio::{AudioPortIndex, DEFAULT_EFFECT_CONFIGURATION},
    automation::ParameterEvents,
    buffer::{ChannelBuffer, InputEndpoint, OutputEndpoint},
    process::{
        ActivationConfig, ProcessBlock, ProcessBufferSource, ProcessChannel, ProcessConfig,
        ProcessContext, ProcessMode, TransportSnapshot,
    },
    runtime::{Component, InstanceRuntime, Process, Processor},
    telemetry::F32Telemetry,
};

const MAIN_INPUT_INDEX: AudioPortIndex = AudioPortIndex::new(0);
const MAIN_OUTPUT_INDEX: AudioPortIndex = AudioPortIndex::new(1);

struct MeterEffect {
    telemetry: Arc<F32Telemetry>,
}

impl Component for MeterEffect {
    type Processor = MeterProcessor;
    type ActivationError = Infallible;

    fn schema(
        &self,
    ) -> Result<chassis_core::schema::ComponentSchema, chassis_core::schema::ComponentSchemaError>
    {
        chassis_core::schema::ComponentSchema::stereo_effect(vec![])
    }

    fn activate(
        &self,
        _config: &ActivationConfig<'_>,
    ) -> Result<Self::Processor, Self::ActivationError> {
        Ok(MeterProcessor {
            telemetry: Arc::clone(&self.telemetry),
        })
    }
}

struct MeterProcessor {
    telemetry: Arc<F32Telemetry>,
}

impl Processor for MeterProcessor {}

impl Process<f32> for MeterProcessor {
    #[allow(clippy::cast_precision_loss)]
    fn process<B>(&mut self, block: &mut ProcessBlock<'_, '_, '_, f32, B>)
    where
        B: ProcessBufferSource<f32> + ?Sized,
    {
        let mut peak = 0.0_f32;
        let mut sum_squares = 0.0_f32;
        let mut sample_count = 0_u32;

        for buffer in block.channels() {
            let Some(samples) = buffer.input() else {
                continue;
            };
            for sample in samples {
                peak = peak.max(sample.abs());
                sum_squares += sample * sample;
                sample_count += 1;
            }
        }

        let rms = if sample_count == 0 {
            0.0
        } else {
            (sum_squares / sample_count as f32).sqrt()
        };

        let _ = self.telemetry.try_publish(&[peak, rms]);
    }
}

fn process_config() -> ProcessConfig {
    ProcessConfig::new(
        48_000.0,
        NonZeroU32::new(1),
        NonZeroU32::new(64).expect("maximum is non-zero"),
        0,
    )
    .expect("meter process configuration is valid")
}

fn process_context(frame_count: u32) -> ProcessContext<'static> {
    ProcessContext::new(
        ProcessMode::Realtime,
        TransportSnapshot::unknown(),
        ParameterEvents::empty_for_block(frame_count),
    )
}

#[test]
fn processor_publishes_meter_snapshot_without_owning_display_state() {
    let telemetry = Arc::new(F32Telemetry::new(&[0.0, 0.0]));
    let component = MeterEffect {
        telemetry: Arc::clone(&telemetry),
    };
    let mut runtime: InstanceRuntime<MeterProcessor> =
        InstanceRuntime::for_component(&component).expect("meter schema is valid");
    runtime
        .activate(&component, process_config(), DEFAULT_EFFECT_CONFIGURATION)
        .expect("meter activation succeeds");

    let mut left = [1.0_f32, -1.0];
    let mut right = [0.0_f32, 0.0];
    let mut buffers = [
        ChannelBuffer::in_place(
            InputEndpoint::new(MAIN_INPUT_INDEX, 0),
            OutputEndpoint::new(MAIN_OUTPUT_INDEX, 0),
            &mut left,
            2,
        )
        .expect("left meter buffer is valid"),
        ChannelBuffer::in_place(
            InputEndpoint::new(MAIN_INPUT_INDEX, 1),
            OutputEndpoint::new(MAIN_OUTPUT_INDEX, 1),
            &mut right,
            2,
        )
        .expect("right meter buffer is valid"),
    ];

    runtime
        .process(2, process_context(2), &mut buffers)
        .expect("meter callback succeeds");

    let mut observed = [0.0_f32; 2];
    let generation = telemetry
        .try_snapshot_into(&mut observed)
        .expect("published meter snapshot is readable");
    assert_eq!(generation.get(), 1);
    assert!((observed[0] - 1.0).abs() <= f32::EPSILON);
    assert!((observed[1] - 0.5_f32.sqrt()).abs() <= f32::EPSILON);

    runtime.deactivate().expect("meter deactivation succeeds");
    let generation_after_deactivation = telemetry
        .try_snapshot_into(&mut observed)
        .expect("control-side telemetry outlives the processor");
    assert_eq!(generation_after_deactivation, generation);
}
