//! In-process CLAP host coverage for process frame-count bounds.

use std::convert::Infallible;

use chassis_clap::{ClapStereoEffect, SingleComponentEntry};
use chassis_core::{
    audio::{MAIN_INPUT, MAIN_OUTPUT},
    process::{ActivationConfig, ProcessBlock, ProcessBufferSource, ProcessChannel},
    runtime::{Component, Process, Processor},
};
use clack_host::{factory::plugin::PluginFactory, prelude::*};

#[derive(Default)]
struct FrameProbe;

impl Component for FrameProbe {
    type Processor = FrameProcessor;
    type ActivationError = Infallible;

    fn activate(
        &self,
        _config: &ActivationConfig<'_>,
    ) -> Result<Self::Processor, Self::ActivationError> {
        Ok(FrameProcessor)
    }
}

struct FrameProcessor;

impl Processor for FrameProcessor {}

impl Process<f32> for FrameProcessor {
    fn process<B>(&mut self, block: &mut ProcessBlock<'_, '_, '_, f32, B>)
    where
        B: ProcessBufferSource<f32> + ?Sized,
    {
        for mut channel in block.channels() {
            if channel
                .input_endpoint()
                .is_some_and(|endpoint| endpoint.port() == MAIN_INPUT)
                && channel
                    .output_endpoint()
                    .is_some_and(|endpoint| endpoint.port() == MAIN_OUTPUT)
            {
                let _ = channel.make_in_place();
            }
        }
    }
}

impl ClapStereoEffect for FrameProbe {
    const CLAP_ID: &'static str = "org.nijaru.chassis.frame-probe";
    const CLAP_NAME: &'static str = "Chassis Frame Probe";
}

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

fn process_frames(
    processor: &mut StartedPluginAudioProcessor<TestHostHandlers>,
    frame_count: usize,
) -> Result<(), PluginInstanceError> {
    let mut main_inputs = [vec![1.0_f32; frame_count], vec![1.0_f32; frame_count]];
    let mut sidechain_inputs = [vec![0.0_f32; frame_count], vec![0.0_f32; frame_count]];
    let mut outputs = [vec![0.0_f32; frame_count], vec![0.0_f32; frame_count]];
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

    processor
        .process(
            &input_audio,
            &mut output_audio,
            &input_events.as_input(),
            &mut output_events.as_output(),
            None,
            None,
        )
        .map(|_| ())
}

#[test]
fn clap_process_accepts_activation_frame_extremes_and_rejects_over_bound_blocks() {
    let entry = PluginEntry::load_from_clack::<SingleComponentEntry<FrameProbe>>(c"")
        .expect("static frame probe entry loads");
    let descriptor = entry
        .get_factory::<PluginFactory>()
        .expect("plugin factory exists")
        .plugin_descriptor(0)
        .expect("frame probe descriptor exists");
    let host_info = HostInfo::new("chassis-test", "", "", "").expect("host info is valid");
    let mut plugin = PluginInstance::<TestHostHandlers>::new(
        |()| TestHostShared,
        |_| TestHostMainThread,
        &entry,
        descriptor.id().expect("frame probe id is valid"),
        &host_info,
    )
    .expect("frame probe instantiates");

    let processor = plugin
        .activate(
            |_, _| TestHostAudioProcessor,
            PluginAudioConfiguration {
                sample_rate: 48_000.0,
                min_frames_count: 1,
                max_frames_count: 512,
            },
        )
        .expect("frame probe activates");
    let mut processor = processor.start_processing().expect("processing starts");

    process_frames(&mut processor, 1).expect("minimum frame block succeeds");
    process_frames(&mut processor, 512).expect("maximum frame block succeeds");
    assert!(process_frames(&mut processor, 513).is_err());

    plugin.deactivate(processor.stop_processing());
}
