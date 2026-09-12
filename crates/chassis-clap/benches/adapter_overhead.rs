//! Manual adapter-only CLAP overhead benchmark.
//!
//! Run on stable representative hardware with:
//! `cargo bench -p chassis-clap --bench adapter_overhead`

use std::{
    convert::Infallible,
    hint::black_box,
    time::{Duration, Instant},
};

use chassis_clap::{ClapStereoEffect, SingleComponentEntryWithF64};
use chassis_core::{
    parameters::ParameterDescriptor,
    process::{ActivationConfig, ProcessBlock, ProcessBufferSource},
    runtime::{Component, Process, Processor},
};
use clack_host::{
    events::event_types::ParamValueEvent, factory::plugin::PluginFactory, prelude::*,
};

const PARAMETER_ID: u32 = 23;
const MAX_EVENTS: u32 = 64;
const BATCHES: usize = 25;
const ITERATIONS_PER_BATCH: u32 = 5_000;
const WARMUP_ITERATIONS: u32 = 1_000;
const FRAME_COUNTS: [u32; 4] = [32, 64, 128, 512];
const EVENT_COUNTS: [u32; 2] = [0, MAX_EVENTS];

struct BenchComponent {
    parameters: Vec<ParameterDescriptor>,
}

impl Default for BenchComponent {
    fn default() -> Self {
        Self {
            parameters: vec![
                ParameterDescriptor::float("value", "Value", 0.0, 1.0, 0.5)
                    .expect("benchmark parameter is valid"),
            ],
        }
    }
}

impl Component for BenchComponent {
    type Processor = BenchProcessor;
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
        Ok(BenchProcessor)
    }
}

struct BenchProcessor;

impl Processor for BenchProcessor {}

impl Process<f32> for BenchProcessor {
    fn process<B>(&mut self, _block: &mut ProcessBlock<'_, '_, '_, f32, B>)
    where
        B: ProcessBufferSource<f32> + ?Sized,
    {
    }
}

impl Process<f64> for BenchProcessor {
    fn process<B>(&mut self, _block: &mut ProcessBlock<'_, '_, '_, f64, B>)
    where
        B: ProcessBufferSource<f64> + ?Sized,
    {
    }
}

impl ClapStereoEffect for BenchComponent {
    const CLAP_ID: &'static str = "org.nijaru.chassis.adapter-bench";
    const CLAP_NAME: &'static str = "Chassis Adapter Benchmark";
    const CLAP_PARAMETER_IDS: &'static [(&'static str, u32)] = &[("value", PARAMETER_ID)];
    const CLAP_MAX_PARAMETER_EVENTS: u32 = MAX_EVENTS;
}

struct BenchHostShared;
struct BenchHostMainThread;
struct BenchHostAudioProcessor;
struct BenchHostHandlers;

impl SharedHandler<'_> for BenchHostShared {
    fn request_restart(&self) {}
    fn request_process(&self) {}
    fn request_callback(&self) {}
}

impl MainThreadHandler<'_> for BenchHostMainThread {}
impl AudioProcessorHandler<'_> for BenchHostAudioProcessor {}

impl HostHandlers for BenchHostHandlers {
    type Shared<'a> = BenchHostShared;
    type MainThread<'a> = BenchHostMainThread;
    type AudioProcessor<'a> = BenchHostAudioProcessor;
}

fn parameter_events(frame_count: u32, event_count: u32) -> EventBuffer {
    let mut events = EventBuffer::with_capacity(
        usize::try_from(event_count).expect("benchmark event count fits usize"),
    );
    if event_count == 0 {
        return events;
    }

    for index in 0..event_count {
        let offset =
            u32::try_from(u64::from(index) * u64::from(frame_count) / u64::from(event_count))
                .expect("benchmark event offset fits u32");
        let value = if index % 2 == 0 { 0.25 } else { 0.75 };
        events.push(&ParamValueEvent::new(
            offset,
            ClapId::new(PARAMETER_ID),
            Pckn::match_all(),
            value,
        ));
    }
    events
}

fn measure_batches(mut callback: impl FnMut()) -> Vec<Duration> {
    for _ in 0..WARMUP_ITERATIONS {
        callback();
    }

    let mut samples = Vec::with_capacity(BATCHES);
    for _ in 0..BATCHES {
        let started = Instant::now();
        for _ in 0..ITERATIONS_PER_BATCH {
            callback();
        }
        samples.push(started.elapsed());
    }
    samples
}

fn nanoseconds_per_call(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000_000_000.0 / f64::from(ITERATIONS_PER_BATCH)
}

fn percentile(sorted: &[f64], numerator: usize, denominator: usize) -> f64 {
    let rank = (sorted.len() * numerator).div_ceil(denominator);
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

fn report(precision: &str, frames: u32, events: u32, samples: Vec<Duration>) {
    let mut per_call: Vec<f64> = samples.into_iter().map(nanoseconds_per_call).collect();
    per_call.sort_by(f64::total_cmp);
    let median = percentile(&per_call, 50, 100);
    let p95 = percentile(&per_call, 95, 100);
    let p99 = percentile(&per_call, 99, 100);
    let callbacks_per_second = 1_000_000_000.0 / median;
    println!(
        "{precision},{frames},{events},{median:.2},{p95:.2},{p99:.2},{callbacks_per_second:.0}"
    );
}

fn bench_f32(
    processor: &mut StartedPluginAudioProcessor<BenchHostHandlers>,
    frame_count: u32,
    event_count: u32,
) {
    let frames = usize::try_from(frame_count).expect("benchmark frame count fits usize");
    let mut main_inputs = [vec![0.0_f32; frames], vec![0.0_f32; frames]];
    let mut sidechain_inputs = [vec![0.0_f32; frames], vec![0.0_f32; frames]];
    let mut outputs = [vec![0.0_f32; frames], vec![0.0_f32; frames]];
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
    let events = parameter_events(frame_count, event_count);
    let mut output_events = EventBuffer::with_capacity(0);

    let samples = measure_batches(|| {
        let status = processor
            .process(
                &input_audio,
                &mut output_audio,
                &events.as_input(),
                &mut output_events.as_output(),
                None,
                None,
            )
            .expect("f32 benchmark callback succeeds");
        black_box(status);
    });
    report("f32", frame_count, event_count, samples);
}

fn bench_f64(
    processor: &mut StartedPluginAudioProcessor<BenchHostHandlers>,
    frame_count: u32,
    event_count: u32,
) {
    let frames = usize::try_from(frame_count).expect("benchmark frame count fits usize");
    let mut main_inputs = [vec![0.0_f64; frames], vec![0.0_f64; frames]];
    let mut sidechain_inputs = [vec![0.0_f64; frames], vec![0.0_f64; frames]];
    let mut outputs = [vec![0.0_f64; frames], vec![0.0_f64; frames]];
    let mut input_ports = AudioPorts::with_capacity(4, 2);
    let mut output_ports = AudioPorts::with_capacity(2, 1);
    let input_audio = input_ports.with_input_buffers([
        AudioPortBuffer {
            channels: AudioPortBufferType::f64_input_only(
                main_inputs.iter_mut().map(InputChannel::variable),
            ),
            latency: 0,
        },
        AudioPortBuffer {
            channels: AudioPortBufferType::f64_input_only(
                sidechain_inputs.iter_mut().map(InputChannel::variable),
            ),
            latency: 0,
        },
    ]);
    let mut output_audio = output_ports.with_output_buffers([AudioPortBuffer {
        channels: AudioPortBufferType::f64_output_only(outputs.iter_mut().map(Vec::as_mut_slice)),
        latency: 0,
    }]);
    let events = parameter_events(frame_count, event_count);
    let mut output_events = EventBuffer::with_capacity(0);

    let samples = measure_batches(|| {
        let status = processor
            .process(
                &input_audio,
                &mut output_audio,
                &events.as_input(),
                &mut output_events.as_output(),
                None,
                None,
            )
            .expect("f64 benchmark callback succeeds");
        black_box(status);
    });
    report("f64", frame_count, event_count, samples);
}

fn main() {
    let entry = PluginEntry::load_from_clack::<SingleComponentEntryWithF64<BenchComponent>>(c"")
        .expect("static benchmark entry loads");
    let descriptor = entry
        .get_factory::<PluginFactory>()
        .expect("plugin factory exists")
        .plugin_descriptor(0)
        .expect("benchmark descriptor exists");
    let host_info = HostInfo::new("chassis-bench", "", "", "").expect("host info is valid");
    let mut plugin = PluginInstance::<BenchHostHandlers>::new(
        |()| BenchHostShared,
        |_| BenchHostMainThread,
        &entry,
        descriptor.id().expect("benchmark plugin id is valid"),
        &host_info,
    )
    .expect("benchmark plugin instantiates");
    let processor = plugin
        .activate(
            |_, _| BenchHostAudioProcessor,
            PluginAudioConfiguration {
                sample_rate: 48_000.0,
                min_frames_count: 1,
                max_frames_count: 512,
            },
        )
        .expect("benchmark plugin activates");
    let mut processor = processor
        .start_processing()
        .expect("benchmark processing starts");

    println!("# chassis-clap adapter-only benchmark");
    println!(
        "# os={} arch={}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!("# run with repository toolchain and record CPU model + rustc -V alongside results");
    println!("precision,frames,events,median_ns,p95_ns,p99_ns,median_callbacks_per_second");

    for frame_count in FRAME_COUNTS {
        for event_count in EVENT_COUNTS {
            bench_f32(&mut processor, frame_count, event_count);
            bench_f64(&mut processor, frame_count, event_count);
        }
    }

    plugin.deactivate(processor.stop_processing());
}
