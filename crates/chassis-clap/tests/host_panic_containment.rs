//! In-process CLAP proof that product panics are contained at the Clack FFI boundary.

use std::{
    convert::Infallible,
    sync::atomic::{AtomicBool, Ordering},
};

use chassis_clap::{ClapStereoEffect, SingleComponentEntry};
use chassis_core::{
    process::{ActivationConfig, ProcessBlock, ProcessBufferSource},
    runtime::{Component, Process, Processor},
};
use clack_host::{factory::plugin::PluginFactory, prelude::*};

struct TestHostShared;
struct TestHostMainThread;
struct TestHostAudioProcessor;
struct TestHostHandlers;

impl SharedHandler<'_> for TestHostShared {
    fn request_restart(&self) {}
    fn request_process(&self) {}
    fn request_callback(&self) {}
}

impl MainThreadHandler<'_> for TestHostMainThread {}
impl AudioProcessorHandler<'_> for TestHostAudioProcessor {}

impl HostHandlers for TestHostHandlers {
    type Shared<'a> = TestHostShared;
    type MainThread<'a> = TestHostMainThread;
    type AudioProcessor<'a> = TestHostAudioProcessor;
}

fn host_info() -> HostInfo {
    HostInfo::new("chassis-test", "", "", "").expect("host info is valid")
}

struct ActivationPanicProbe {
    panic_next_activation: AtomicBool,
}

impl Default for ActivationPanicProbe {
    fn default() -> Self {
        Self {
            panic_next_activation: AtomicBool::new(true),
        }
    }
}

impl Component for ActivationPanicProbe {
    type Processor = NoopProcessor;
    type ActivationError = Infallible;

    fn activate(
        &self,
        _config: &ActivationConfig<'_>,
    ) -> Result<Self::Processor, Self::ActivationError> {
        if self.panic_next_activation.swap(false, Ordering::Relaxed) {
            panic!("intentional activation panic");
        }
        Ok(NoopProcessor)
    }
}

impl ClapStereoEffect for ActivationPanicProbe {
    const CLAP_ID: &'static str = "org.nijaru.chassis.activation-panic-probe";
    const CLAP_NAME: &'static str = "Chassis Activation Panic Probe";
}

struct NoopProcessor;

impl Processor for NoopProcessor {}

impl Process<f32> for NoopProcessor {
    fn process<B>(&mut self, _block: &mut ProcessBlock<'_, '_, '_, f32, B>)
    where
        B: ProcessBufferSource<f32> + ?Sized,
    {
    }
}

#[test]
fn activation_panic_is_contained_and_instance_can_activate_again() {
    let entry = PluginEntry::load_from_clack::<SingleComponentEntry<ActivationPanicProbe>>(c"")
        .expect("static activation panic entry loads");
    let descriptor = entry
        .get_factory::<PluginFactory>()
        .expect("plugin factory exists")
        .plugin_descriptor(0)
        .expect("activation panic descriptor exists");
    let mut plugin = PluginInstance::<TestHostHandlers>::new(
        |()| TestHostShared,
        |_| TestHostMainThread,
        &entry,
        descriptor.id().expect("activation panic id is valid"),
        &host_info(),
    )
    .expect("activation panic probe instantiates");
    let config = PluginAudioConfiguration {
        sample_rate: 48_000.0,
        min_frames_count: 1,
        max_frames_count: 64,
    };

    assert!(
        plugin
            .activate(|_, _| TestHostAudioProcessor, config)
            .is_err()
    );
    assert!(!plugin.is_active());

    let processor = plugin
        .activate(|_, _| TestHostAudioProcessor, config)
        .expect("same instance activates after contained product panic");
    plugin.deactivate(processor);
}

#[derive(Default)]
struct ProcessPanicProbe;

impl Component for ProcessPanicProbe {
    type Processor = ProcessPanicProcessor;
    type ActivationError = Infallible;

    fn activate(
        &self,
        _config: &ActivationConfig<'_>,
    ) -> Result<Self::Processor, Self::ActivationError> {
        Ok(ProcessPanicProcessor)
    }
}

impl ClapStereoEffect for ProcessPanicProbe {
    const CLAP_ID: &'static str = "org.nijaru.chassis.process-panic-probe";
    const CLAP_NAME: &'static str = "Chassis Process Panic Probe";
}

struct ProcessPanicProcessor;

impl Processor for ProcessPanicProcessor {}

impl Process<f32> for ProcessPanicProcessor {
    fn process<B>(&mut self, _block: &mut ProcessBlock<'_, '_, '_, f32, B>)
    where
        B: ProcessBufferSource<f32> + ?Sized,
    {
        panic!("intentional process panic");
    }
}

#[test]
fn process_panic_becomes_host_processing_failure_and_deactivates_cleanly() {
    let entry = PluginEntry::load_from_clack::<SingleComponentEntry<ProcessPanicProbe>>(c"")
        .expect("static process panic entry loads");
    let descriptor = entry
        .get_factory::<PluginFactory>()
        .expect("plugin factory exists")
        .plugin_descriptor(0)
        .expect("process panic descriptor exists");
    let mut plugin = PluginInstance::<TestHostHandlers>::new(
        |()| TestHostShared,
        |_| TestHostMainThread,
        &entry,
        descriptor.id().expect("process panic id is valid"),
        &host_info(),
    )
    .expect("process panic probe instantiates");
    let processor = plugin
        .activate(
            |_, _| TestHostAudioProcessor,
            PluginAudioConfiguration {
                sample_rate: 48_000.0,
                min_frames_count: 1,
                max_frames_count: 64,
            },
        )
        .expect("process panic probe activates");
    let mut processor = processor.start_processing().expect("processing starts");

    let mut main_inputs = [vec![0.0_f32; 64], vec![0.0_f32; 64]];
    let mut sidechain_inputs = [vec![0.0_f32; 64], vec![0.0_f32; 64]];
    let mut outputs = [vec![0.0_f32; 64], vec![0.0_f32; 64]];
    let mut input_ports = AudioPorts::with_capacity(4, 2);
    let mut output_ports = AudioPorts::with_capacity(2, 1);
    let input_audio = input_ports.with_input_buffers([
        AudioPortBuffer {
            channels: AudioPortBufferType::f32_input_only(
                main_inputs.iter_mut().map(InputChannel::variable),
            ),
            latency: 0,
        },
        AudioPortBuffer {
            channels: AudioPortBufferType::f32_input_only(
                sidechain_inputs.iter_mut().map(InputChannel::variable),
            ),
            latency: 0,
        },
    ]);
    let mut output_audio = output_ports.with_output_buffers([AudioPortBuffer {
        channels: AudioPortBufferType::f32_output_only(outputs.iter_mut().map(Vec::as_mut_slice)),
        latency: 0,
    }]);
    let input_events = EventBuffer::with_capacity(0);
    let mut output_events = EventBuffer::with_capacity(0);

    assert!(
        processor
            .process(
                &input_audio,
                &mut output_audio,
                &input_events.as_input(),
                &mut output_events.as_output(),
                None,
                None,
            )
            .is_err()
    );

    plugin.deactivate(processor.stop_processing());
}
