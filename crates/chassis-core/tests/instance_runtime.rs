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
        AudioIoConfiguration, AudioPortIndex, ChannelLayout, ConfiguredAudioPort,
        DEFAULT_EFFECT_CONFIGURATION, MAIN_INPUT, MAIN_OUTPUT,
    },
    automation::ParameterEvents,
    buffer::{ChannelBuffer, InputEndpoint, OutputEndpoint},
    parameters::{ParameterDescriptor, ParameterStore, ParameterValue},
    process::{
        ActivationConfig, ProcessBlock, ProcessBlockError, ProcessBufferSource, ProcessChannel,
        ProcessConfig, ProcessContext, ProcessMode, TransportSnapshot,
    },
    runtime::{
        Component, InstanceRuntime, InstanceStateError, InstanceStateLoadError, Process, Processor,
    },
    state::{StateDocument, StateEntry, StateMigration, StateMigrationError, StateValue},
};

const MAIN_INPUT_INDEX: AudioPortIndex = AudioPortIndex::new(0);
const MAIN_OUTPUT_INDEX: AudioPortIndex = AudioPortIndex::new(1);

fn main_input(channel: u32) -> InputEndpoint {
    InputEndpoint::new(MAIN_INPUT_INDEX, channel)
}

fn main_output(channel: u32) -> OutputEndpoint {
    OutputEndpoint::new(MAIN_OUTPUT_INDEX, channel)
}

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

    fn schema(
        &self,
    ) -> Result<chassis_core::schema::ComponentSchema, chassis_core::schema::ComponentSchemaError>
    {
        chassis_core::schema::ComponentSchema::stereo_effect(self.parameters.clone())
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
    fn process<B>(&mut self, block: &mut ProcessBlock<'_, '_, '_, f32, B>)
    where
        B: ProcessBufferSource<f32> + ?Sized,
    {
        let gain = match block.parameters().get("gain") {
            Some(ParameterValue::Float(value)) => *value as f32,
            _ => 1.0,
        };
        for mut buffer in block.channels() {
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

struct SplitChannelBuffers<'buffers, 'samples, S> {
    first: &'buffers mut [ChannelBuffer<'samples, S>],
    second: &'buffers mut [ChannelBuffer<'samples, S>],
}

impl<'samples, S> ProcessBufferSource<S> for SplitChannelBuffers<'_, 'samples, S> {
    type Channel<'a>
        = &'a mut ChannelBuffer<'samples, S>
    where
        Self: 'a,
        S: 'a;

    type Channels<'a>
        = core::iter::Chain<
        core::slice::IterMut<'a, ChannelBuffer<'samples, S>>,
        core::slice::IterMut<'a, ChannelBuffer<'samples, S>>,
    >
    where
        Self: 'a,
        S: 'a;

    fn validate_frame_count(&mut self, expected: usize) -> Result<(), ProcessBlockError> {
        for buffer in self.first.iter().chain(self.second.iter()) {
            if buffer.frame_count() != expected {
                return Err(ProcessBlockError::BufferFrameCountMismatch {
                    expected,
                    actual: buffer.frame_count(),
                });
            }
        }
        Ok(())
    }

    fn channels(&mut self) -> Self::Channels<'_> {
        self.first.iter_mut().chain(self.second.iter_mut())
    }
}

const LEGACY_STATE_V1: &[u8] = &[
    b'C', b'H', b'S', b'S', 1, 0, 18, 0, 1, 0, 0, 0, 1, 0, 0, 0, b'c', b'o', b'm', b'.', b'e',
    b'x', b'a', b'm', b'p', b'l', b'e', b'.', b'e', b'f', b'f', b'e', b'c', b't', 18, 0, 3, 0, 8,
    0, 0, 0, b'p', b'a', b'r', b'a', b'm', b'e', b't', b'e', b'r', b'/', b'o', b'l', b'd', b'_',
    b'g', b'a', b'i', b'n', 0, 0, 0, 0, 0, 0, 208, 63,
];

struct RenameGainMigration;

impl StateMigration for RenameGainMigration {
    type Error = Infallible;

    fn source_schema(&self) -> u32 {
        1
    }

    fn to_schema(&self) -> u32 {
        2
    }

    fn migrate(&self, document: StateDocument) -> Result<StateDocument, Self::Error> {
        let mut migrated = StateDocument::new(document.product_id().to_owned(), 2)
            .expect("migration identity is valid");
        for entry in document.entries() {
            let key = if entry.key() == "parameter/old_gain" {
                "parameter/gain"
            } else {
                entry.key()
            };
            migrated
                .insert(StateEntry::new(key, entry.value().clone()))
                .expect("migration keys are unique");
        }
        Ok(migrated)
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
            ConfiguredAudioPort::new(MAIN_INPUT, ChannelLayout::Stereo),
            ConfiguredAudioPort::new(MAIN_OUTPUT, ChannelLayout::Stereo),
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
fn state_bytes_are_decoded_migrated_and_applied_transactionally() {
    let metrics = Arc::new(Metrics::default());
    let component = Effect::new(metrics);
    let mut runtime: InstanceRuntime<EffectProcessor> =
        InstanceRuntime::for_component(&component).expect("instance schema is valid");
    runtime
        .parameters_mut()
        .set("gain", ParameterValue::Float(1.75))
        .expect("gain is valid");

    let bytes = LEGACY_STATE_V1;
    let migration = RenameGainMigration;
    runtime
        .apply_parameter_state_bytes(
            bytes,
            "com.example.effect",
            2,
            chassis_core::state::StateLimits::default(),
            &[&migration],
        )
        .expect("state migration and application succeed");
    assert_eq!(
        runtime.parameters().get("gain"),
        Some(&ParameterValue::Float(0.25))
    );

    runtime
        .parameters_mut()
        .set("gain", ParameterValue::Float(1.5))
        .expect("gain is valid");
    let error = runtime.apply_parameter_state_bytes(
        bytes,
        "com.example.effect",
        3,
        chassis_core::state::StateLimits::default(),
        &[&migration],
    );
    assert!(matches!(
        error,
        Err(InstanceStateLoadError::Migration(
            StateMigrationError::MissingMigration { from: 2, target: 3 }
        ))
    ));
    assert_eq!(
        runtime.parameters().get("gain"),
        Some(&ParameterValue::Float(1.5))
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
    let mut buffers = [
        ChannelBuffer::in_place(main_input(0), main_output(0), &mut left, 2)
            .expect("buffer is valid"),
    ];
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

#[test]
fn process_source_accepts_noncontiguous_channel_storage() {
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

    let left_input = [1.0_f32, 0.5];
    let right_input = [0.25_f32, 2.0];
    let mut left_output = [0.0_f32; 2];
    let mut right_output = [0.0_f32; 2];
    let mut first = [ChannelBuffer::separate(
        main_input(0),
        &left_input,
        main_output(0),
        &mut left_output,
        2,
    )
    .expect("left channel is valid")];
    let mut second = [ChannelBuffer::separate(
        main_input(1),
        &right_input,
        main_output(1),
        &mut right_output,
        2,
    )
    .expect("right channel is valid")];
    let mut source = SplitChannelBuffers {
        first: &mut first,
        second: &mut second,
    };
    let context = ProcessContext::new(
        ProcessMode::Realtime,
        TransportSnapshot::unknown(),
        ParameterEvents::empty_for_block(2),
    );

    runtime
        .process_source(2, context, &mut source)
        .expect("noncontiguous buffer source processes");

    assert_eq!(
        left_output.map(f32::to_bits),
        [0.5_f32.to_bits(), 0.25_f32.to_bits()]
    );
    assert_eq!(
        right_output.map(f32::to_bits),
        [0.125_f32.to_bits(), 1.0_f32.to_bits()]
    );
}
