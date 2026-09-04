//! Deterministic external conformance tests for the durable core runtime.
//!
//! These tests exercise `Component`, `InstanceRuntime`, and `Process` through
//! the public API so adapters can reuse the same semantic expectations.

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
    automation::{ParameterEventValue, ParameterEvents},
    buffer::{ChannelBuffer, InputEndpoint, OutputEndpoint},
    parameters::{ParameterDescriptor, ParameterIndex, ParameterValue},
    process::{
        ActivationConfig, ProcessBlock, ProcessBlockError, ProcessBufferSource, ProcessChannel,
        ProcessConfig, ProcessContext, ProcessMode, TransportSnapshot,
    },
    runtime::{
        ActivateError, Component, InstanceProcessError, InstanceRuntime, InstanceRuntimeError,
        Process, Processor,
    },
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
    parameters: Vec<ParameterDescriptor>,
}

impl ConformanceEffect {
    fn new(metrics: Arc<Metrics>, gain: f32) -> Self {
        Self {
            metrics,
            gain,
            parameters: Vec::new(),
        }
    }

    fn with_parameters(
        metrics: Arc<Metrics>,
        gain: f32,
        parameters: Vec<ParameterDescriptor>,
    ) -> Self {
        Self {
            metrics,
            gain,
            parameters,
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
    #[allow(clippy::cast_possible_truncation)]
    fn process<B>(&mut self, block: &mut ProcessBlock<'_, '_, '_, f32, B>)
    where
        B: ProcessBufferSource<f32> + ?Sized,
    {
        self.metrics.process_calls.fetch_add(1, Ordering::Relaxed);
        let parameter_events = block.parameter_events();
        let gain_index = block.parameters().index("input.gain");
        let base_gain = match block.parameters().get("input.gain") {
            Some(ParameterValue::Float(value)) => *value,
            _ => f64::from(self.gain),
        };

        for mut buffer in block.channels() {
            let is_main_pair = buffer
                .input_endpoint()
                .is_some_and(|endpoint| endpoint.port() == MAIN_INPUT)
                && buffer
                    .output_endpoint()
                    .is_some_and(|endpoint| endpoint.port() == MAIN_OUTPUT);
            if !is_main_pair {
                continue;
            }

            let samples = buffer
                .make_in_place()
                .expect("conformance main channels always have paired input/output");
            if let Some(gain_index) = gain_index {
                let mut gain = parameter_events
                    .float_cursor(gain_index, base_gain)
                    .expect("conformance gain cursor has a finite base");
                for (offset, sample) in samples.iter_mut().enumerate() {
                    *sample *= gain
                        .value_at(u32::try_from(offset).expect("test block fits in u32"))
                        .expect("conformance cursor offset is within the block")
                        as f32;
                }
            } else {
                for sample in samples {
                    *sample *= self.gain;
                }
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
    let minimum = minimum.map(|value| NonZeroU32::new(value).expect("test minimum is non-zero"));
    let maximum = NonZeroU32::new(maximum).expect("test maximum is non-zero");
    ProcessConfig::new(48_000.0, minimum, maximum, 8).expect("test process configuration is valid")
}

fn runtime(component: &ConformanceEffect) -> InstanceRuntime<ConformanceProcessor> {
    InstanceRuntime::new(component.parameter_descriptors()).expect("component schema is valid")
}

fn activate_runtime(
    component: &ConformanceEffect,
    minimum: Option<u32>,
    maximum: u32,
) -> InstanceRuntime<ConformanceProcessor> {
    let mut runtime = runtime(component);
    runtime
        .activate(
            component,
            process_config(minimum, maximum),
            DEFAULT_EFFECT_CONFIGURATION,
        )
        .expect("conformance activation is valid");
    runtime
}

fn metric(value: &AtomicU32) -> u32 {
    value.load(Ordering::Relaxed)
}

fn process_context(frame_count: u32, mode: ProcessMode) -> ProcessContext<'static> {
    ProcessContext::new(
        mode,
        TransportSnapshot::unknown(),
        ParameterEvents::empty_for_block(frame_count),
    )
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
fn runtime_owns_validated_parameter_base_state() {
    let metrics = Arc::new(Metrics::default());
    let descriptors = vec![
        ParameterDescriptor::float("input.gain", "Input gain", -1.0, 1.0, 0.0)
            .expect("parameter definition is valid"),
    ];
    let component = ConformanceEffect::with_parameters(metrics, 1.0, descriptors);
    let mut runtime = activate_runtime(&component, Some(1), 8);

    assert_eq!(
        runtime.parameters().get("input.gain"),
        Some(&ParameterValue::Float(0.0))
    );
    runtime
        .parameters_mut()
        .set("input.gain", ParameterValue::Float(0.75))
        .expect("value is in range");

    let input = [1.0_f32];
    let mut output = [0.0_f32];
    let mut buffers = [ChannelBuffer::separate(
        InputEndpoint::new(MAIN_INPUT, 0),
        &input,
        OutputEndpoint::new(MAIN_OUTPUT, 0),
        &mut output,
        1,
    )
    .expect("base-value test buffer is valid")];
    runtime
        .process(1, process_context(1, ProcessMode::Realtime), &mut buffers)
        .expect("base-value process block is valid");
    assert_samples(&output, &[0.75]);
}

#[test]
fn parameter_automation_is_sample_accurate_through_runtime() {
    let metrics = Arc::new(Metrics::default());
    let descriptor = ParameterDescriptor::float("input.gain", "Input gain", 0.0, 1.0, 1.0)
        .expect("parameter definition is valid");
    let component = ConformanceEffect::with_parameters(Arc::clone(&metrics), 1.0, vec![descriptor]);
    let mut runtime = activate_runtime(&component, Some(1), 8);
    let gain = runtime
        .parameters()
        .index("input.gain")
        .expect("gain index exists");

    let raw_events = [
        chassis_core::automation::ParameterEvent::set(1, gain, ParameterEventValue::Float(0.5)),
        chassis_core::automation::ParameterEvent::linear(3, gain, 0.0),
    ];
    let events = ParameterEvents::new(&raw_events, 4, 8).expect("events are valid");
    let context = ProcessContext::new(
        ProcessMode::Realtime,
        TransportSnapshot::new(Some(true), Some(false), Some(120.0), Some(48_000))
            .expect("transport snapshot is valid"),
        events,
    );

    let input = [1.0_f32; 4];
    let mut output = [0.0_f32; 4];
    let mut buffers = [ChannelBuffer::separate(
        InputEndpoint::new(MAIN_INPUT, 0),
        &input,
        OutputEndpoint::new(MAIN_OUTPUT, 0),
        &mut output,
        4,
    )
    .expect("test buffer is valid")];
    runtime
        .process(4, context, &mut buffers)
        .expect("valid automation block processes");

    assert_samples(&output, &[1.0, 0.5, 0.25, 0.0]);
    assert_eq!(metric(&metrics.process_calls), 1);
    assert_eq!(context.transport().tempo_bpm(), Some(120.0));
    assert_eq!(context.transport().sample_position(), Some(48_000));
}

#[test]
fn invalid_parameter_events_never_reach_product_dsp() {
    let metrics = Arc::new(Metrics::default());
    let descriptor = ParameterDescriptor::float("input.gain", "Input gain", 0.0, 1.0, 1.0)
        .expect("parameter definition is valid");
    let component = ConformanceEffect::with_parameters(Arc::clone(&metrics), 1.0, vec![descriptor]);
    let mut runtime = activate_runtime(&component, Some(1), 8);
    let raw_events = [chassis_core::automation::ParameterEvent::set(
        0,
        ParameterIndex::new(99),
        ParameterEventValue::Float(0.5),
    )];
    let events = ParameterEvents::new(&raw_events, 2, 8).expect("event shape is valid");
    let context = ProcessContext::new(ProcessMode::Realtime, TransportSnapshot::unknown(), events);
    let input = [1.0_f32; 2];
    let mut output = [0.0_f32; 2];
    let mut buffers = [ChannelBuffer::separate(
        InputEndpoint::new(MAIN_INPUT, 0),
        &input,
        OutputEndpoint::new(MAIN_OUTPUT, 0),
        &mut output,
        2,
    )
    .expect("test buffer is valid")];

    assert!(matches!(
        runtime.process(2, context, &mut buffers),
        Err(InstanceProcessError::InvalidBlock(
            ProcessBlockError::InvalidParameterEvents(_)
        ))
    ));
    assert_eq!(metric(&metrics.process_calls), 0);
    assert_samples(&output, &[0.0, 0.0]);
}

#[test]
fn invalid_parameter_schema_never_reaches_product_activation() {
    let metrics = Arc::new(Metrics::default());
    let descriptor = ParameterDescriptor::boolean("bypass", "Bypass", false)
        .expect("parameter definition is valid");
    let component = ConformanceEffect::with_parameters(
        Arc::clone(&metrics),
        1.0,
        vec![descriptor.clone(), descriptor],
    );

    let result: Result<InstanceRuntime<ConformanceProcessor>, _> =
        InstanceRuntime::new(component.parameter_descriptors());

    assert!(matches!(
        result,
        Err(InstanceRuntimeError::InvalidParameters(
            chassis_core::parameters::ParameterStoreError::InvalidDefinition(
                chassis_core::parameters::ParameterDefinitionError::DuplicateParameter(_)
            )
        ))
    ));
    assert_eq!(metric(&metrics.activations), 0);
}

#[test]
fn runtime_processes_separate_buffers_and_owns_lifecycle() {
    let metrics = Arc::new(Metrics::default());
    let component = ConformanceEffect::new(Arc::clone(&metrics), 2.0);
    let mut runtime = activate_runtime(&component, Some(1), 8);
    assert_eq!(metric(&metrics.activations), 1);

    let left_input = [0.25_f32, -0.5, 1.0, 0.0];
    let right_input = [0.5_f32, 0.25, -0.25, 1.0];
    let mut left_output = [0.0_f32; 4];
    let mut right_output = [0.0_f32; 4];
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
    runtime
        .process(4, process_context(4, ProcessMode::Realtime), &mut buffers)
        .expect("valid realtime block processes");

    assert_samples(&left_output, &[0.5, -1.0, 2.0, 0.0]);
    assert_samples(&right_output, &[1.0, 0.5, -0.5, 2.0]);
    runtime.reset().expect("active runtime resets");
    assert_eq!(metric(&metrics.resets), 1);
    runtime.deactivate().expect("active runtime deactivates");
    assert_eq!(metric(&metrics.drops), 1);
}

#[test]
fn exact_in_place_buffers_do_not_require_a_second_alias() {
    let metrics = Arc::new(Metrics::default());
    let component = ConformanceEffect::new(metrics, 0.5);
    let mut runtime = activate_runtime(&component, Some(1), 8);
    let mut left = [1.0_f32, 0.5, -1.0];
    let mut right = [0.25_f32, -0.5, 2.0];
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
    runtime
        .process(
            3,
            process_context(3, ProcessMode::BufferedRealtime),
            &mut buffers,
        )
        .expect("valid buffered-realtime block processes");

    assert_samples(&left, &[0.5, 0.25, -0.5]);
    assert_samples(&right, &[0.125, -0.25, 1.0]);
}

#[test]
fn malformed_audio_configuration_never_reaches_product_activation() {
    let metrics = Arc::new(Metrics::default());
    let component = ConformanceEffect::new(Arc::clone(&metrics), 1.0);
    let only_input = [ConfiguredAudioPort {
        key: MAIN_INPUT,
        layout: ChannelLayout::Stereo,
    }];
    let mut runtime = runtime(&component);
    let result = runtime.activate(
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
    let component = ConformanceEffect::new(Arc::clone(&metrics), 1.0);
    let mut runtime = activate_runtime(&component, Some(1), 8);

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
        runtime.process(9, process_context(9, ProcessMode::Realtime), &mut too_large),
        Err(InstanceProcessError::InvalidBlock(
            ProcessBlockError::ExceedsActivatedMaximum { .. }
        ))
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
        runtime.process(
            0,
            process_context(0, ProcessMode::Realtime),
            &mut below_minimum
        ),
        Err(InstanceProcessError::InvalidBlock(
            ProcessBlockError::BelowGuaranteedMinimum { .. }
        ))
    ));
    assert_eq!(metric(&metrics.process_calls), 0);
}

#[test]
fn zero_frame_callback_is_supported_when_no_positive_minimum_is_promised() {
    let metrics = Arc::new(Metrics::default());
    let component = ConformanceEffect::new(Arc::clone(&metrics), 1.0);
    let mut runtime = activate_runtime(&component, None, 8);
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
    runtime
        .process(0, process_context(0, ProcessMode::Offline), &mut buffers)
        .expect("zero-frame callback is valid without a positive minimum guarantee");
    assert_eq!(metric(&metrics.process_calls), 1);
}
