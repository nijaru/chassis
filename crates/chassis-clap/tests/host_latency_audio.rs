//! In-process CLAP proof that reported latency matches delayed audio.

use std::convert::Infallible;

use chassis_clap::{ClapStereoEffect, SingleComponentEntry};
use chassis_core::{
    audio::{AudioPortIndex, MAIN_INPUT, MAIN_OUTPUT, audio_port_index},
    process::{ActivationConfig, ProcessBlock, ProcessBufferSource, ProcessChannel},
    runtime::{Component, LatencySamples, Process, Processor},
};
use clack_extensions::latency::{HostLatency, HostLatencyImpl, PluginLatency};
use clack_host::{factory::plugin::PluginFactory, prelude::*};

#[derive(Default)]
struct DelayedProbe;

impl Component for DelayedProbe {
    type Processor = DelayedProcessor;
    type ActivationError = Infallible;

    fn activate(
        &self,
        config: &ActivationConfig<'_>,
    ) -> Result<Self::Processor, Self::ActivationError> {
        let samples = if config.process().sample_rate() >= 88_200.0 {
            128
        } else {
            64
        };
        let length = usize::try_from(samples).expect("test latency fits usize");
        Ok(DelayedProcessor {
            latency: LatencySamples::new(samples),
            main_input: audio_port_index(self.audio_ports(), MAIN_INPUT)
                .expect("default effect schema has main input"),
            main_output: audio_port_index(self.audio_ports(), MAIN_OUTPUT)
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
                .make_in_place()
                .expect("main channel has input and output")
            {
                std::mem::swap(sample, &mut delay[*position]);
                *position += 1;
                if *position == delay.len() {
                    *position = 0;
                }
            }
        }
    }
}

impl ClapStereoEffect for DelayedProbe {
    const CLAP_ID: &'static str = "org.nijaru.chassis.delayed-probe";
    const CLAP_NAME: &'static str = "Chassis Delayed Probe";
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
impl HostLatencyImpl for TestHostMainThread {
    fn changed(&self) {}
}

impl HostHandlers for TestHostHandlers {
    type Shared<'a> = TestHostShared;
    type MainThread<'a> = TestHostMainThread;
    type AudioProcessor<'a> = TestHostAudioProcessor;

    fn declare_extensions(builder: &mut HostExtensions<Self>, _shared: &Self::Shared<'_>) {
        builder.register::<HostLatency>();
    }
}

fn process_impulse(
    processor: &mut StartedPluginAudioProcessor<TestHostHandlers>,
    frame_count: usize,
) -> [Vec<f32>; 2] {
    let mut main_inputs = [vec![0.0_f32; frame_count], vec![0.0_f32; frame_count]];
    for channel in &mut main_inputs {
        channel[0] = 1.0;
    }
    let mut sidechain_inputs = [vec![0.0_f32; frame_count], vec![0.0_f32; frame_count]];
    let mut outputs = [vec![0.0_f32; frame_count], vec![0.0_f32; frame_count]];
    let mut input_ports = AudioPorts::with_capacity(4, 2);
    let mut output_ports = AudioPorts::with_capacity(2, 1);
    {
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
            channels: AudioPortBufferType::f32_output_only(
                outputs.iter_mut().map(Vec::as_mut_slice),
            ),
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
            .expect("delayed impulse callback succeeds");
    }
    outputs
}

#[test]
fn reported_latency_matches_impulse_delay() {
    let entry = PluginEntry::load_from_clack::<SingleComponentEntry<DelayedProbe>>(c"")
        .expect("static delayed probe entry loads");
    let descriptor = entry
        .get_factory::<PluginFactory>()
        .expect("plugin factory exists")
        .plugin_descriptor(0)
        .expect("delayed probe descriptor exists");
    let host_info = HostInfo::new("chassis-test", "", "", "").expect("host info is valid");
    let mut plugin = PluginInstance::<TestHostHandlers>::new(
        |()| TestHostShared,
        |_| TestHostMainThread,
        &entry,
        descriptor.id().expect("delayed probe id is valid"),
        &host_info,
    )
    .expect("delayed probe instantiates");

    let processor = plugin
        .activate(
            |_, _| TestHostAudioProcessor,
            PluginAudioConfiguration {
                sample_rate: 48_000.0,
                min_frames_count: 1,
                max_frames_count: 128,
            },
        )
        .expect("delayed probe activates");
    let plugin_handle = plugin.plugin_handle();
    let latency = plugin_handle
        .get_extension::<PluginLatency>()
        .expect("delayed probe exports latency")
        .get(&plugin_handle);
    assert_eq!(latency, 64);

    let mut processor = processor.start_processing().expect("processing starts");
    let outputs = process_impulse(&mut processor, 128);
    for channel in outputs {
        assert!(
            channel[..64]
                .iter()
                .all(|sample| sample.abs() <= f32::EPSILON)
        );
        assert!((channel[64] - 1.0).abs() <= f32::EPSILON);
        assert!(
            channel[65..]
                .iter()
                .all(|sample| sample.abs() <= f32::EPSILON)
        );
    }

    plugin.deactivate(processor.stop_processing());
}
