use std::{
    convert::Infallible,
    num::NonZeroU32,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};

use chassis_core::{
    audio::{
        AudioIoConfiguration, AudioIoConfigurationError, ChannelLayout, ConfiguredAudioPort,
        DEFAULT_EFFECT_CONFIGURATION, MAIN_INPUT, MAIN_OUTPUT,
    },
    buffer::{ChannelBuffer, InputEndpoint, OutputEndpoint},
    process::{ActivationConfig, ProcessBlock, ProcessBlockError, ProcessConfig, ProcessMode},
    runtime::{ActivateError, Component, Process, Processor, activate},
};

#[derive(Default)]
struct Metrics {
    activations: AtomicU32,
    resets: AtomicU32,
    process_calls: AtomicU32,
    drops: AtomicU32,
}

struct ConformanceEffect {
    metrics: Arc<Metrics>,
    gain: f32,
}

impl Component for ConformanceEffect {
    type Processor = ConformanceProcessor;
    type ActivationError = Infallible;

    fn activate(
        &self,
        _config: &ActivationConfig<'_>,
    ) -> Result<Self::Processor, Self::ActivationError> {
        self.metrics.activations.fetch_add(1, Ordering::Relaxed);
        Ok(ConformanceProcessor {
            metrics: Arc::clone(&self.metrics),
            gain: self.gain,
        })
    }
}

struct ConformanceProcessor {
    metrics: Arc<Metrics>,
    gain: f32,
}

impl Processor for ConformanceProcessor {
    fn reset(&mut self) {
        self.metrics.resets.fetch_add(1, Ordering::Relaxed);
    }
}

impl Process<f32> for ConformanceProcessor {
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, f32>) {
        self.metrics.process_calls.fetch_add(1, Ordering::Relaxed);

        for buffer in block.buffers_mut() {
            let is_main_pair = buffer
                .input_endpoint()
                .is_some_and(|endpoint| endpoint.port() == MAIN_INPUT)
                && buffer
                    .output_endpoint()
                    .is_some_and(|endpoint| endpoint.port() == MAIN_OUTPUT);

            if !is_main_pair {
                continue;
            }

            for sample in buffer
                .make_in_place()
                .expect("conformance main channels always have paired input/output")
            {
                *sample *= self.gain;
            }
        }
    }
}

impl Drop for ConformanceProcessor {
    fn drop(&mut self) {
        self.metrics.drops.fetch_add(1, Ordering::Relaxed);
    }
}

fn process_config(minimum: Option<u32>, maximum: u32) -> ProcessConfig {
    let minimum = minimum
        .map(|value| NonZeroU32::new(value).expect("test minimum is non-zero"));
    let maximum = NonZeroU32::new(maximum).expect("test maximum is non-zero");
    ProcessConfig::new(48_000.0, minimum, maximum).expect("test process configuration is valid")
}

fn metric(value: &AtomicU32) -> u32 {
    value.load(Ordering::Relaxed)
}

fn assert_samples(actual: &[f32], expected: &[f32]) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        assert!((*actual - *expected).abs() <= f32::EPSILON);
    }
}

fn assert_send<T: Send>() {}

#[test]
fn conformance_processor_can_cross_a_plugin_thread_boundary() {
    assert_send::<ConformanceProcessor>();
}

#[test]
fn explicit_runtime_processes_separate_buffers_and_owns_lifecycle() {
    let metrics = Arc::new(Metrics::default());
    let component = ConformanceEffect {
        metrics: Arc::clone(&metrics),
        gain: 2.0,
    };
    let mut active = activate(
        &component,
        process_config(Some(1), 8),
        DEFAULT_EFFECT_CONFIGURATION,
    )
    .expect("default conformance activation is valid");

    assert_eq!(metric(&metrics.activations), 1);

    let left_input = [0.25_f32, -0.5, 1.0, 0.0];
    let right_input = [0.5_f32, 0.25, -0.25, 1.0];
    let mut left_output = [0.0_f32; 4];
    let mut right_output = [0.0_f32; 4];

    {
        let mut buffers = [
            ChannelBuffer::separate(
                InputEndpoint::new(MAIN_INPUT, 0),
                &left_input,
                OutputEndpoint::new(MAIN_OUTPUT, 0),
                &mut left_output,
                4,
            )
            .expect("left test buffers are valid"),
            ChannelBuffer::separate(
                InputEndpoint::new(MAIN_INPUT, 1),
                &right_input,
                OutputEndpoint::new(MAIN_OUTPUT, 1),
                &mut right_output,
                4,
            )
            .expect("right test buffers are valid"),
        ];

        active
            .process(4, ProcessMode::Realtime, &mut buffers)
            .expect("valid realtime block processes");
    }

    assert_samples(&left_output, &[0.5, -1.0, 2.0, 0.0]);
    assert_samples(&right_output, &[1.0, 0.5, -0.5, 2.0]);
    assert_eq!(metric(&metrics.process_calls), 1);

    active.reset();
    assert_eq!(metric(&metrics.resets), 1);

    active.deactivate();
    assert_eq!(metric(&metrics.drops), 1);
}

#[test]
fn exact_in_place_buffers_do_not_require_a_second_alias() {
    let metrics = Arc::new(Metrics::default());
    let component = ConformanceEffect {
        metrics,
        gain: 0.5,
    };
    let mut active = activate(
        &component,
        process_config(Some(1), 8),
        DEFAULT_EFFECT_CONFIGURATION,
    )
    .expect("default conformance activation is valid");

    let mut left = [1.0_f32, 0.5, -1.0];
    let mut right = [0.25_f32, -0.5, 2.0];

    {
        let mut buffers = [
            ChannelBuffer::in_place(
                InputEndpoint::new(MAIN_INPUT, 0),
                OutputEndpoint::new(MAIN_OUTPUT, 0),
                &mut left,
                3,
            )
            .expect("left in-place buffer is valid"),
            ChannelBuffer::in_place(
                InputEndpoint::new(MAIN_INPUT, 1),
                OutputEndpoint::new(MAIN_OUTPUT, 1),
                &mut right,
                3,
            )
            .expect("right in-place buffer is valid"),
        ];

        active
            .process(3, ProcessMode::BufferedRealtime, &mut buffers)
            .expect("valid buffered-realtime block processes");
    }

    assert_samples(&left, &[0.5, 0.25, -0.5]);
    assert_samples(&right, &[0.125, -0.25, 1.0]);
}

#[test]
fn malformed_audio_configuration_never_reaches_product_activation() {
    let metrics = Arc::new(Metrics::default());
    let component = ConformanceEffect {
        metrics: Arc::clone(&metrics),
        gain: 1.0,
    };
    let only_input = [ConfiguredAudioPort {
        key: MAIN_INPUT,
        layout: ChannelLayout::Stereo,
    }];

    let result = activate(
        &component,
        process_config(Some(1), 8),
        AudioIoConfiguration::new(&only_input),
    );

    assert!(matches!(
        result,
        Err(ActivateError::InvalidAudioIo(
            AudioIoConfigurationError::MissingRequiredPort(MAIN_OUTPUT)
        ))
    ));
    assert_eq!(metric(&metrics.activations), 0);
}

#[test]
fn callback_dimension_failures_are_contained_before_product_dsp() {
    let metrics = Arc::new(Metrics::default());
    let component = ConformanceEffect {
        metrics: Arc::clone(&metrics),
        gain: 1.0,
    };
    let mut active = activate(
        &component,
        process_config(Some(1), 8),
        DEFAULT_EFFECT_CONFIGURATION,
    )
    .expect("default conformance activation is valid");

    let mut left = [0.0_f32; 9];
    let mut right = [0.0_f32; 9];
    let mut too_large = [
        ChannelBuffer::in_place(
            InputEndpoint::new(MAIN_INPUT, 0),
            OutputEndpoint::new(MAIN_OUTPUT, 0),
            &mut left,
            9,
        )
        .expect("storage is intentionally large enough for malformed callback"),
        ChannelBuffer::in_place(
            InputEndpoint::new(MAIN_INPUT, 1),
            OutputEndpoint::new(MAIN_OUTPUT, 1),
            &mut right,
            9,
        )
        .expect("storage is intentionally large enough for malformed callback"),
    ];

    assert!(matches!(
        active.process(9, ProcessMode::Realtime, &mut too_large),
        Err(ProcessBlockError::ExceedsActivatedMaximum { .. })
    ));
    assert_eq!(metric(&metrics.process_calls), 0);

    let mut empty_left = [0.0_f32; 0];
    let mut empty_right = [0.0_f32; 0];
    let mut below_minimum = [
        ChannelBuffer::in_place(
            InputEndpoint::new(MAIN_INPUT, 0),
            OutputEndpoint::new(MAIN_OUTPUT, 0),
            &mut empty_left,
            0,
        )
        .expect("zero-length storage is valid for a zero-frame view"),
        ChannelBuffer::in_place(
            InputEndpoint::new(MAIN_INPUT, 1),
            OutputEndpoint::new(MAIN_OUTPUT, 1),
            &mut empty_right,
            0,
        )
        .expect("zero-length storage is valid for a zero-frame view"),
    ];

    assert!(matches!(
        active.process(0, ProcessMode::Realtime, &mut below_minimum),
        Err(ProcessBlockError::BelowGuaranteedMinimum { .. })
    ));
    assert_eq!(metric(&metrics.process_calls), 0);
}

#[test]
fn zero_frame_callback_is_supported_when_no_positive_minimum_is_promised() {
    let metrics = Arc::new(Metrics::default());
    let component = ConformanceEffect {
        metrics: Arc::clone(&metrics),
        gain: 1.0,
    };
    let mut active = activate(
        &component,
        process_config(None, 8),
        DEFAULT_EFFECT_CONFIGURATION,
    )
    .expect("activation with unknown minimum is valid");

    let mut left = [0.0_f32; 0];
    let mut right = [0.0_f32; 0];
    let mut buffers = [
        ChannelBuffer::in_place(
            InputEndpoint::new(MAIN_INPUT, 0),
            OutputEndpoint::new(MAIN_OUTPUT, 0),
            &mut left,
            0,
        )
        .expect("zero-length storage is valid"),
        ChannelBuffer::in_place(
            InputEndpoint::new(MAIN_INPUT, 1),
            OutputEndpoint::new(MAIN_OUTPUT, 1),
            &mut right,
            0,
        )
        .expect("zero-length storage is valid"),
    ];

    active
        .process(0, ProcessMode::Offline, &mut buffers)
        .expect("zero-frame callback is valid without a positive minimum guarantee");
    assert_eq!(metric(&metrics.process_calls), 1);
}
