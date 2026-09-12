//! Runtime qualification for activation-resolved audio endpoint legality.

use std::{
    convert::Infallible,
    num::NonZeroU32,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};

use chassis_core::{
    audio::{AudioEndpointError, AudioPortIndex, DEFAULT_EFFECT_CONFIGURATION},
    automation::ParameterEvents,
    buffer::{ChannelBuffer, InputEndpoint, OutputEndpoint},
    process::{
        ActivationConfig, ProcessBlock, ProcessBufferSource, ProcessConfig, ProcessContext,
        ProcessMode, TransportSnapshot,
    },
    runtime::{Component, InstanceProcessError, InstanceRuntime, Process, Processor},
};

const MAIN_INPUT_INDEX: AudioPortIndex = AudioPortIndex::new(0);
const MAIN_OUTPUT_INDEX: AudioPortIndex = AudioPortIndex::new(1);
const SIDECHAIN_INPUT_INDEX: AudioPortIndex = AudioPortIndex::new(2);

struct ProbeComponent {
    process_calls: Arc<AtomicU32>,
}

impl Component for ProbeComponent {
    type Processor = ProbeProcessor;
    type ActivationError = Infallible;

    fn activate(
        &self,
        _config: &ActivationConfig<'_>,
    ) -> Result<Self::Processor, Self::ActivationError> {
        Ok(ProbeProcessor {
            process_calls: Arc::clone(&self.process_calls),
        })
    }
}

struct ProbeProcessor {
    process_calls: Arc<AtomicU32>,
}

impl Processor for ProbeProcessor {}

impl Process<f32> for ProbeProcessor {
    fn process<B>(&mut self, _block: &mut ProcessBlock<'_, '_, '_, f32, B>)
    where
        B: ProcessBufferSource<f32> + ?Sized,
    {
        self.process_calls.fetch_add(1, Ordering::Relaxed);
    }
}

fn process_config() -> ProcessConfig {
    ProcessConfig::new(
        48_000.0,
        NonZeroU32::new(1),
        NonZeroU32::new(64).expect("maximum is non-zero"),
        0,
    )
    .expect("process configuration is valid")
}

fn process_context(frame_count: u32) -> ProcessContext<'static> {
    ProcessContext::new(
        ProcessMode::Realtime,
        TransportSnapshot::unknown(),
        ParameterEvents::empty_for_block(frame_count),
    )
}

fn active_runtime() -> (Arc<AtomicU32>, InstanceRuntime<ProbeProcessor>) {
    let process_calls = Arc::new(AtomicU32::new(0));
    let component = ProbeComponent {
        process_calls: Arc::clone(&process_calls),
    };
    let mut runtime = InstanceRuntime::for_component(&component).expect("schema is valid");
    runtime
        .activate(&component, process_config(), DEFAULT_EFFECT_CONFIGURATION)
        .expect("activation succeeds");
    (process_calls, runtime)
}

#[test]
fn valid_dense_endpoints_reach_product_dsp() {
    let (process_calls, mut runtime) = active_runtime();
    let mut samples = [1.0_f32];
    let mut buffers = [ChannelBuffer::in_place(
        InputEndpoint::new(MAIN_INPUT_INDEX, 0),
        OutputEndpoint::new(MAIN_OUTPUT_INDEX, 0),
        &mut samples,
        1,
    )
    .expect("buffer is valid")];

    runtime
        .process(1, process_context(1), &mut buffers)
        .expect("valid resolved endpoints process");
    assert_eq!(process_calls.load(Ordering::Relaxed), 1);
}

#[test]
fn inactive_optional_port_is_rejected_before_product_dsp() {
    let (process_calls, mut runtime) = active_runtime();
    let samples = [1.0_f32];
    let mut buffers =
        [
            ChannelBuffer::input_only(InputEndpoint::new(SIDECHAIN_INPUT_INDEX, 0), &samples, 1)
                .expect("buffer is structurally valid"),
        ];

    assert!(matches!(
        runtime.process(1, process_context(1), &mut buffers),
        Err(InstanceProcessError::InvalidAudioEndpoint(
            AudioEndpointError::InactivePort(index)
        )) if index == SIDECHAIN_INPUT_INDEX
    ));
    assert_eq!(process_calls.load(Ordering::Relaxed), 0);
}

#[test]
fn wrong_endpoint_direction_is_rejected_before_product_dsp() {
    let (process_calls, mut runtime) = active_runtime();
    let samples = [1.0_f32];
    let mut buffers =
        [
            ChannelBuffer::input_only(InputEndpoint::new(MAIN_OUTPUT_INDEX, 0), &samples, 1)
                .expect("buffer is structurally valid"),
        ];

    assert!(matches!(
        runtime.process(1, process_context(1), &mut buffers),
        Err(InstanceProcessError::InvalidAudioEndpoint(
            AudioEndpointError::WrongDirection { port, .. }
        )) if port == MAIN_OUTPUT_INDEX
    ));
    assert_eq!(process_calls.load(Ordering::Relaxed), 0);
}

#[test]
fn out_of_range_channel_is_rejected_before_product_dsp() {
    let (process_calls, mut runtime) = active_runtime();
    let samples = [1.0_f32];
    let mut buffers =
        [
            ChannelBuffer::input_only(InputEndpoint::new(MAIN_INPUT_INDEX, 2), &samples, 1)
                .expect("buffer is structurally valid"),
        ];

    assert!(matches!(
        runtime.process(1, process_context(1), &mut buffers),
        Err(InstanceProcessError::InvalidAudioEndpoint(
            AudioEndpointError::ChannelOutOfRange {
                port,
                channel: 2,
                channel_count: 2,
            }
        )) if port == MAIN_INPUT_INDEX
    ));
    assert_eq!(process_calls.load(Ordering::Relaxed), 0);
}

#[test]
fn unknown_dense_port_is_rejected_before_product_dsp() {
    let (process_calls, mut runtime) = active_runtime();
    let samples = [1.0_f32];
    let unknown = AudioPortIndex::new(99);
    let mut buffers = [
        ChannelBuffer::input_only(InputEndpoint::new(unknown, 0), &samples, 1)
            .expect("buffer is structurally valid"),
    ];

    assert!(matches!(
        runtime.process(1, process_context(1), &mut buffers),
        Err(InstanceProcessError::InvalidAudioEndpoint(
            AudioEndpointError::UnknownPort(index)
        )) if index == unknown
    ));
    assert_eq!(process_calls.load(Ordering::Relaxed), 0);
}
