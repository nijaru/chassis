//! External conformance tests for durable instance ownership.

use std::{
    convert::Infallible,
    num::NonZeroU32,
    sync::{
        Arc,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
};

use chassis_core::{
    audio::{
        AudioIoConfiguration, ChannelLayout, ConfiguredAudioPort, DEFAULT_EFFECT_CONFIGURATION,
        MAIN_INPUT, MAIN_OUTPUT,
    },
    automation::ParameterEvents,
    buffer::ChannelBuffer,
    parameters::{ParameterDescriptor, ParameterStore, ParameterValue},
    process::{
        ActivationConfig, ProcessBlock, ProcessConfig, ProcessContext, ProcessMode,
        TransportSnapshot,
    },
    runtime::{Component, InstanceRuntime, InstanceStateError, Process, Processor},
    state::{StateDocument, StateEntry, StateValue},
};

#[derive(Default)]
struct Metrics {
    activations: AtomicU32,
    drops: AtomicU32,
    activation_gain_bits: AtomicU64,
}

struct Effect {
    metrics: Arc<Metrics>,
    parameters: Vec<ParameterDescriptor>,
}

impl Effect {
    fn new(metrics: Arc<Metrics>) -> Self {
        Self {
            metrics,
            parameters: vec![
                ParameterDescriptor::float("gain", "Gain", 0.0, 2.0, 1.0)
                    .expect("test parameter is valid"),
            ],
        }
    }
}

impl Component for Effect {
    type Processor = EffectProcessor;
    type ActivationError = Infallible;

    fn parameter_descriptors(&self) -> &[ParameterDescriptor] {
        &self.parameters
    }

    fn activate(
        &self,
        _config: &ActivationConfig<'_>,
    ) -> Result<Self::Processor, Self::ActivationError> {
        self.metrics.activations.fetch_add(1, Ordering::Relaxed);
        Ok(EffectProcessor {
            metrics: Arc::clone(&self.metrics),
        })
    }

    fn activate_with_parameters(
        &self,
        config: &ActivationConfig<'_>,
        parameters: &ParameterStore,
    ) -> Result<Self::Processor, Self::ActivationError> {
        let gain = match parameters.get("gain") {
            Some(ParameterValue::Float(value)) => *value,
            _ => 0.0,
        };
        self.metrics
            .activation_gain_bits
            .store(gain.to_bits(), Ordering::Relaxed);
        self.activate(config)
    }
}

struct EffectProcessor {
    metrics: Arc<Metrics>,
}

impl Processor for EffectProcessor {}

impl Process<f32> for EffectProcessor {
    #[allow(clippy::cast_possible_truncation)]
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, '_, f32>) {
        let gain = match block.parameters().get("gain") {
            Some(ParameterValue::Float(value)) => *value as f32,
            _ => 1.0,
        };
        for buffer in block.buffers_mut() {
            if let Ok(samples) = buffer.make_in_place() {
                for sample in samples {
                    *sample *= gain;
                }
            }
        }
    }
}

impl Drop for EffectProcessor {
    fn drop(&mut self) {
        self.metrics.drops.fetch_add(1, Ordering::Relaxed);
    }
}

fn process_config() -> ProcessConfig {
    ProcessConfig::new(
        48_000.0,
        NonZeroU32::new(1),
        NonZeroU32::new(64).expect("maximum is non-zero"),
        8,
    )
    .expect("process config is valid")
}

#[test]
fn parameters_survive_deactivation_and_reactivation() {
    let metrics = Arc::new(Metrics::default());
    let component = Effect::new(Arc::clone(&metrics));
    let mut runtime: InstanceRuntime<EffectProcessor> =
        InstanceRuntime::for_component(&component).expect("instance schema is valid");
    runtime
        .parameters_mut()
        .set("gain", ParameterValue::Float(1.5))
        .expect("gain is valid");

    runtime
        .activate(&component, process_config(), DEFAULT_EFFECT_CONFIGURATION)
        .expect("first activation succeeds");
    assert_eq!(
        f64::from_bits(metrics.activation_gain_bits.load(Ordering::Relaxed)).to_bits(),
        1.5_f64.to_bits()
    );
    runtime.deactivate().expect("first deactivation succeeds");

    assert_eq!(
        runtime.parameters().get("gain"),
        Some(&ParameterValue::Float(1.5))
    );
    runtime
        .activate(&component, process_config(), DEFAULT_EFFECT_CONFIGURATION)
        .expect("second activation succeeds");
    assert_eq!(
        f64::from_bits(metrics.activation_gain_bits.load(Ordering::Relaxed)).to_bits(),
        1.5_f64.to_bits()
    );
    runtime.deactivate().expect("second deactivation succeeds");

    assert_eq!(metrics.activations.load(Ordering::Relaxed), 2);
    assert_eq!(metrics.drops.load(Ordering::Relaxed), 2);
}

#[test]
fn activation_owns_a_dynamic_audio_configuration() {
    let metrics = Arc::new(Metrics::default());
    let component = Effect::new(metrics);
    let mut runtime: InstanceRuntime<EffectProcessor> =
        InstanceRuntime::for_component(&component).expect("instance schema is valid");

    {
        let ports = vec![
            ConfiguredAudioPort {
                key: MAIN_INPUT,
                layout: ChannelLayout::Stereo,
            },
            ConfiguredAudioPort {
                key: MAIN_OUTPUT,
                layout: ChannelLayout::Stereo,
            },
        ];
        runtime
            .activate(
                &component,
                process_config(),
                AudioIoConfiguration::new(&ports),
            )
            .expect("dynamic configuration activates");
    }

    let active = runtime.active_config().expect("runtime remains active");
    assert_eq!(active.audio_io().ports().len(), 2);
    assert_eq!(active.audio_io().ports()[0].key, MAIN_INPUT);
    runtime.deactivate().expect("deactivation succeeds");
}

#[test]
fn complete_state_replacement_is_transactional() {
    let metrics = Arc::new(Metrics::default());
    let component = Effect::new(metrics);
    let mut runtime: InstanceRuntime<EffectProcessor> =
        InstanceRuntime::for_component(&component).expect("instance schema is valid");
    runtime
        .parameters_mut()
        .set("gain", ParameterValue::Float(1.75))
        .expect("gain is valid");

    let mut incomplete = StateDocument::new("com.example.effect", 1).expect("identity is valid");
    incomplete
        .insert(StateEntry::new("state/custom", StateValue::Boolean(true)))
        .expect("custom entry is valid");
    assert!(matches!(
        runtime.apply_parameter_state_for_product(&incomplete, "com.example.effect", 1),
        Err(InstanceStateError::IncompleteParameterState {
            expected: 1,
            actual: 0,
        })
    ));
    assert_eq!(
        runtime.parameters().get("gain"),
        Some(&ParameterValue::Float(1.75))
    );

    let mut complete = StateDocument::new("com.example.effect", 1).expect("identity is valid");
    complete
        .insert(StateEntry::new("parameter/gain", StateValue::Float(0.25)))
        .expect("parameter entry is valid");
    runtime
        .apply_parameter_state_for_product(&complete, "com.example.effect", 1)
        .expect("complete state replaces current values");
    assert_eq!(
        runtime.parameters().get("gain"),
        Some(&ParameterValue::Float(0.25))
    );
}

#[test]
fn process_uses_durable_base_state() {
    let metrics = Arc::new(Metrics::default());
    let component = Effect::new(metrics);
    let mut runtime: InstanceRuntime<EffectProcessor> =
        InstanceRuntime::for_component(&component).expect("instance schema is valid");
    runtime
        .parameters_mut()
        .set("gain", ParameterValue::Float(0.5))
        .expect("gain is valid");
    runtime
        .activate(&component, process_config(), DEFAULT_EFFECT_CONFIGURATION)
        .expect("activation succeeds");

    let mut left = [1.0_f32, 0.5];
    let mut buffers = [ChannelBuffer::in_place(
        chassis_core::buffer::InputEndpoint::new(MAIN_INPUT, 0),
        chassis_core::buffer::OutputEndpoint::new(MAIN_OUTPUT, 0),
        &mut left,
        2,
    )
    .expect("buffer is valid")];
    let context = ProcessContext::new(
        ProcessMode::Realtime,
        TransportSnapshot::unknown(),
        ParameterEvents::empty_for_block(2),
    );
    runtime
        .process(2, context, &mut buffers)
        .expect("process succeeds");
    assert_eq!(
        left.map(f32::to_bits),
        [0.5_f32.to_bits(), 0.25_f32.to_bits()]
    );
}
