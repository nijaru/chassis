//! One useful, stepped-control DSP workload shared by the benchmark and its regressions.
#![allow(unsafe_code)]

use chassis_clap::{ClapAudioPort, ClapComponent, ClapDescriptor, F32AndF64, SingleComponentEntry};
use chassis_runtime::{
    audio::{
        AudioIoConfiguration, AudioIoConfigurationSpec, AudioIoPolicy, AudioPortIndex,
        ChannelCount, ChannelLayout, ConfiguredAudioPort, DEFAULT_EFFECT_PORTS, MAIN_INPUT,
        MAIN_OUTPUT, SIDECHAIN_INPUT,
    },
    automation::{ParameterCursorError, ParameterEvent, ParameterEventValue, ParameterEvents},
    buffer::{ChannelBuffer, InputEndpoint, OutputEndpoint},
    modulation::ModulatedFloatCursor,
    parameters::{FloatParameter, ParameterDescriptor, ParameterIndex, ParameterStore},
    process::{
        ActivationConfig, ProcessBlock, ProcessBufferSource, ProcessConfig, ProcessContext,
        ProcessMode,
    },
    runtime::{Component, InstanceRuntime, Process, ProcessError, Processor},
    schema::{ComponentId, ComponentSchema, ComponentSchemaError, StateSchemaVersion},
    state::StateEntry,
    transport::TransportSnapshot,
};
use clack_extensions::params::PluginParams;
use clack_host::{
    events::{
        event_types::{ParamModEvent, ParamValueEvent},
        io::{InputEvents, OutputEvents},
    },
    prelude::*,
};
use clap_sys::{
    audio_buffer::clap_audio_buffer,
    process::{CLAP_PROCESS_CONTINUE, clap_process},
};
use std::{
    convert::Infallible,
    fmt::Debug,
    hint::black_box,
    num::NonZeroU32,
    ops::{Add, Mul, Sub},
    ptr,
};

pub const MAX_EVENTS: u32 = 64;
// Two base/modulation assignments per parameter keep replayed blocks dynamic.
pub const SPARSE_EVENTS: u32 = 12;
const KEYS: [&str; 3] = ["gain", "wet", "depth"];
const IDS: [u32; 3] = [23, 59, 11];
const MAXIMUM: [f64; 3] = [2.0, 1.0, 1.0];
const DEFAULT: [f64; 3] = [1.5, 0.75, 0.5];

pub trait Sample:
    Copy
    + Debug
    + PartialEq
    + PartialOrd
    + Add<Output = Self>
    + Sub<Output = Self>
    + Mul<Output = Self>
    + 'static
{
    const NAME: &'static str;
    fn from_value(value: f64) -> Self;
    fn abs(self) -> Self;
    fn descriptor(table: *mut *mut Self, count: usize) -> clap_audio_buffer;
}
impl Sample for f32 {
    const NAME: &'static str = "f32";
    #[allow(clippy::cast_possible_truncation)] // Deliberate native-f32 DSP/control quantization.
    fn from_value(value: f64) -> Self {
        value as Self
    }
    fn abs(self) -> Self {
        self.abs()
    }
    fn descriptor(table: *mut *mut Self, count: usize) -> clap_audio_buffer {
        clap_audio_buffer {
            data32: table,
            data64: ptr::null_mut(),
            channel_count: u32::try_from(count).unwrap(),
            latency: 0,
            constant_mask: 0,
        }
    }
}
impl Sample for f64 {
    const NAME: &'static str = "f64";
    fn from_value(value: f64) -> Self {
        value
    }
    fn abs(self) -> Self {
        self.abs()
    }
    fn descriptor(table: *mut *mut Self, count: usize) -> clap_audio_buffer {
        clap_audio_buffer {
            data32: ptr::null_mut(),
            data64: table,
            channel_count: u32::try_from(count).unwrap(),
            latency: 0,
            constant_mask: 0,
        }
    }
}

const fn layout(channels: usize) -> ChannelLayout {
    match channels {
        1 => ChannelLayout::Mono,
        2 => ChannelLayout::Stereo,
        8 => ChannelLayout::Discrete(ChannelCount::new(8).unwrap()),
        _ => panic!("unsupported benchmark topology"),
    }
}
fn configuration<const N: usize, const SC: usize>() -> Vec<ConfiguredAudioPort> {
    let mut ports = vec![
        ConfiguredAudioPort::new(MAIN_INPUT, layout(N)),
        ConfiguredAudioPort::new(MAIN_OUTPUT, layout(N)),
    ];
    if SC != 0 {
        ports.push(ConfiguredAudioPort::new(SIDECHAIN_INPUT, layout(SC)));
    }
    ports
}
#[derive(Default)]
struct Effect<const N: usize, const SC: usize, const INDEPENDENT: bool>;
impl<const N: usize, const SC: usize, const INDEPENDENT: bool> Component
    for Effect<N, SC, INDEPENDENT>
{
    type Processor = Dsp<N, SC, INDEPENDENT>;
    type ActivationError = Infallible;
    fn schema(&self) -> Result<ComponentSchema, ComponentSchemaError> {
        ComponentSchema::new(
            ComponentId::new("org.nijaru.chassis.cost-probe").unwrap(),
            StateSchemaVersion::new(1),
            DEFAULT_EFFECT_PORTS.to_vec(),
            AudioIoPolicy::enumerated(vec![
                AudioIoConfigurationSpec::new(configuration::<N, SC>()),
            ]),
            vec![],
            KEYS.iter()
                .enumerate()
                .map(|(i, key)| {
                    ParameterDescriptor::float(*key, *key, 0.0, MAXIMUM[i], DEFAULT[i])
                        .unwrap()
                        .with_global_modulation()
                        .unwrap()
                })
                .collect(),
        )
    }
    fn activate(
        &self,
        config: &ActivationConfig<'_>,
        parameters: &ParameterStore,
        _: &[StateEntry],
    ) -> Result<Self::Processor, Infallible> {
        Ok(Dsp {
            ports: Ports::new(config.schema()),
            handles: KEYS.map(|key| parameters.float_parameter(key).unwrap()),
        })
    }
}
impl<const N: usize, const SC: usize, const INDEPENDENT: bool> ClapComponent
    for Effect<N, SC, INDEPENDENT>
{
    const CLAP_DESCRIPTOR: ClapDescriptor =
        ClapDescriptor::new("org.nijaru.chassis.cost-probe", "Chassis Cost Probe");
    const CLAP_PARAMETER_IDS: &'static [(&'static str, u32)] =
        &[("gain", IDS[0]), ("wet", IDS[1]), ("depth", IDS[2])];
    const CLAP_MAX_PARAMETER_EVENTS: u32 = MAX_EVENTS;
    const CLAP_AUDIO_PORTS: &'static [ClapAudioPort] = if SC == 0 {
        &[
            ClapAudioPort::new(MAIN_INPUT, 0, layout(N), Some(MAIN_OUTPUT)),
            ClapAudioPort::new(MAIN_OUTPUT, 0, layout(N), Some(MAIN_INPUT)),
        ]
    } else {
        &[
            ClapAudioPort::new(MAIN_INPUT, 0, layout(N), Some(MAIN_OUTPUT)),
            ClapAudioPort::new(MAIN_OUTPUT, 0, layout(N), Some(MAIN_INPUT)),
            ClapAudioPort::new(SIDECHAIN_INPUT, 1, layout(SC), None),
        ]
    };
}
#[derive(Clone, Copy)]
struct Ports {
    input: AudioPortIndex,
    output: AudioPortIndex,
    side: AudioPortIndex,
}
impl Ports {
    fn new(schema: &ComponentSchema) -> Self {
        Self {
            input: schema.audio_port_index(&MAIN_INPUT).unwrap(),
            output: schema.audio_port_index(&MAIN_OUTPUT).unwrap(),
            side: schema.audio_port_index(&SIDECHAIN_INPUT).unwrap(),
        }
    }
}
struct Dsp<const N: usize, const SC: usize, const INDEPENDENT: bool> {
    ports: Ports,
    handles: [FloatParameter; 3],
}
impl<const N: usize, const SC: usize, const INDEPENDENT: bool> Processor
    for Dsp<N, SC, INDEPENDENT>
{
}
fn values<S: Sample>(
    cursors: &mut [ModulatedFloatCursor<'_>; 3],
    frame: u32,
) -> Result<[S; 3], ParameterCursorError> {
    Ok([
        S::from_value(cursors[0].value_at(frame)?),
        S::from_value(cursors[1].value_at(frame)?),
        S::from_value(cursors[2].value_at(frame)?),
    ])
}
impl<S: Sample, const N: usize, const SC: usize, const INDEPENDENT: bool> Process<S>
    for Dsp<N, SC, INDEPENDENT>
{
    fn process(&mut self, block: &mut ProcessBlock<'_, S>) -> Result<(), ProcessError> {
        let initial = [
            self.handles[0].cursor(block)?,
            self.handles[1].cursor(block)?,
            self.handles[2].cursor(block)?,
        ];
        if INDEPENDENT {
            let mut cursors: [_; N] = std::array::from_fn(|_| initial.clone());
            block.with_channels(|channels| {
                render::<S, N, SC, INDEPENDENT>(channels, self.ports, |frame, channel| {
                    values(&mut cursors[channel], frame)
                })
            })
        } else {
            let mut cursors = initial;
            block.with_channels(|channels| {
                render::<S, N, SC, INDEPENDENT>(channels, self.ports, |frame, _| {
                    values(&mut cursors, frame)
                })
            })
        }
    }
}
fn read<S: Copy>(channel: &ChannelBuffer<'_, S>, frame: usize) -> S {
    match channel {
        ChannelBuffer::InPlace { samples, .. } => samples[frame],
        ChannelBuffer::Separate { input_samples, .. } => input_samples[frame],
        _ => panic!("main channel must be paired"),
    }
}
fn render<S: Sample, const N: usize, const SC: usize, const INDEPENDENT: bool>(
    channels: &mut dyn Iterator<Item = ChannelBuffer<'_, S>>,
    ports: Ports,
    mut evaluate: impl FnMut(u32, usize) -> Result<[S; 3], ParameterCursorError>,
) -> Result<(), ProcessError> {
    let mut main: [Option<ChannelBuffer<'_, S>>; N] = std::array::from_fn(|_| None);
    let mut side: [Option<&[S]>; SC] = [None; SC];
    for channel in channels {
        match channel {
            ChannelBuffer::InputOnly { input, samples } if input.port_index() == ports.side => {
                side[usize::try_from(input.channel()).unwrap()] = Some(samples);
            }
            ChannelBuffer::InPlace { input, output, .. }
            | ChannelBuffer::Separate { input, output, .. }
                if input.port_index() == ports.input && output.port_index() == ports.output =>
            {
                assert_eq!(input.channel(), output.channel());
                main[usize::try_from(input.channel()).unwrap()] = Some(channel);
            }
            _ => panic!("unexpected benchmark channel"),
        }
    }
    let frames = main[0].as_ref().unwrap().frame_count();
    let zero = S::from_value(0.0);
    for frame in 0..frames {
        let mut detector = zero;
        if SC == 0 {
            for channel in &main {
                let value = read(channel.as_ref().unwrap(), frame).abs();
                if value > detector {
                    detector = value;
                }
            }
        } else {
            for samples in side {
                let value = samples.unwrap()[frame].abs();
                if value > detector {
                    detector = value;
                }
            }
        }
        let offset = u32::try_from(frame).unwrap();
        let shared = if INDEPENDENT {
            None
        } else {
            Some(evaluate(offset, 0)?)
        };
        for (index, channel) in main.iter_mut().enumerate() {
            let controls = match shared {
                Some(value) => value,
                None => evaluate(offset, index)?,
            };
            match channel.as_mut().unwrap() {
                ChannelBuffer::InPlace { samples, .. } => {
                    samples[frame] = kernel(samples[frame], detector, controls);
                }
                ChannelBuffer::Separate {
                    input_samples,
                    output_samples,
                    ..
                } => output_samples[frame] = kernel(input_samples[frame], detector, controls),
                _ => unreachable!(),
            }
        }
    }
    Ok(())
}

fn kernel<S: Sample>(input: S, detector: S, [gain, wet, depth]: [S; 3]) -> S {
    let one = S::from_value(1.0);
    let attenuation = if detector > S::from_value(0.5) {
        one - depth
    } else {
        one
    };
    input * ((one - wet) + wet * gain * attenuation)
}

#[derive(Clone, Copy)]
struct Step {
    offset: u32,
    parameter: usize,
    modulation: bool,
    value: f64,
}
struct Events {
    steps: Vec<Step>,
    core: Vec<ParameterEvent>,
    clap: EventBuffer,
}
impl Events {
    fn steady_changes(&self, frames: u32) -> [bool; 3] {
        let mut completed = Controls::default();
        for step in &self.steps {
            completed.apply(*step);
        }
        let at = |frame| {
            let mut state = completed;
            for step in self.steps.iter().take_while(|s| s.offset <= frame) {
                state.apply(*step);
            }
            state.values::<f64>().map(f64::to_bits)
        };
        let first = at(0);
        let mut changes = [false; 3];
        for frame in 1..frames {
            let value = at(frame);
            for i in 0..3 {
                changes[i] |= value[i] != first[i];
            }
        }
        changes
    }
    fn new(frames: u32, count: u32) -> Self {
        assert!(count <= MAX_EVENTS);
        let steps: Vec<_> = (0..count)
            .map(|i| {
                let parameter = usize::try_from((i / 2) % 3).unwrap();
                Step {
                    offset: i * frames / count,
                    parameter,
                    modulation: i % 2 != 0,
                    value: if i % 2 == 0 {
                        [0.25, 0.75, 0.5, 1.0][usize::try_from((i / 6) % 4).unwrap()]
                            * MAXIMUM[parameter]
                    } else if (i / 6) % 2 == 0 {
                        0.5
                    } else {
                        -0.25
                    },
                }
            })
            .collect();
        let mut core = Vec::new();
        let mut clap = EventBuffer::with_capacity(usize::try_from(count).unwrap());
        for step in &steps {
            let parameter = ParameterIndex::new(u32::try_from(step.parameter).unwrap());
            if step.modulation {
                core.push(ParameterEvent::modulation(
                    step.offset,
                    parameter,
                    step.value,
                ));
                clap.push(&ParamModEvent::new(
                    step.offset,
                    ClapId::new(IDS[step.parameter]),
                    Pckn::match_all(),
                    step.value,
                ));
            } else {
                core.push(ParameterEvent::set(
                    step.offset,
                    parameter,
                    ParameterEventValue::Float(step.value),
                ));
                clap.push(&ParamValueEvent::new(
                    step.offset,
                    ClapId::new(IDS[step.parameter]),
                    Pckn::match_all(),
                    step.value,
                ));
            }
        }
        Self { steps, core, clap }
    }
}
#[derive(Clone, Copy)]
struct Controls {
    base: [f64; 3],
    amount: [f64; 3],
}
impl Default for Controls {
    fn default() -> Self {
        Self {
            base: DEFAULT,
            amount: [0.0; 3],
        }
    }
}
impl Controls {
    fn apply(&mut self, step: Step) {
        if step.modulation {
            self.amount[step.parameter] = step.value;
        } else {
            self.base[step.parameter] = step.value;
        }
    }
    fn values<S: Sample>(self) -> [S; 3] {
        std::array::from_fn(|i| {
            S::from_value((self.base[i] + self.amount[i]).clamp(0.0, MAXIMUM[i]))
        })
    }
    fn assert_eq(self, other: Self) {
        assert_eq!(self.base.map(f64::to_bits), other.base.map(f64::to_bits));
        assert_eq!(
            self.amount.map(f64::to_bits),
            other.amount.map(f64::to_bits)
        );
    }
}

struct Buffers<S> {
    seed: Vec<Vec<S>>,
    main: Vec<Vec<S>>,
    side: Vec<Vec<S>>,
    output: Vec<Vec<S>>,
    inplace: bool,
}
impl<S: Sample> Buffers<S> {
    fn new<const N: usize, const SC: usize>(frames: u32, inplace: bool) -> Self {
        let frames = usize::try_from(frames).unwrap();
        let seed: Vec<Vec<S>> = (0..N)
            .map(|c| {
                (0..frames)
                    .map(|f| {
                        S::from_value(
                            f64::from(u32::try_from((f * 3 + c * 5) % 31).unwrap()) / 16.0 - 0.875,
                        )
                    })
                    .collect()
            })
            .collect();
        let side = (0..SC)
            .map(|c| {
                (0..frames)
                    .map(|f| {
                        S::from_value(f64::from(u32::try_from((f + c * 3) % 5).unwrap()) / 4.0)
                    })
                    .collect()
            })
            .collect();
        Self {
            main: seed.clone(),
            seed,
            side,
            output: vec![vec![S::from_value(f64::NAN); frames]; N],
            inplace,
        }
    }
    fn refill(&mut self) {
        for (input, seed) in self.main.iter_mut().zip(&self.seed) {
            input.copy_from_slice(black_box(seed));
        }
    }
    fn output(&self) -> &[Vec<S>] {
        if self.inplace {
            &self.main
        } else {
            &self.output
        }
    }
    // Deliberately independent reference: rescan all preceding steps for every
    // frame, outside timing. No framework cursor, bare evaluator or render helper.
    fn expected(&self, before: Controls, steps: &[Step]) -> Vec<Vec<S>> {
        let mut output = self.seed.clone();
        for frame in 0..self.seed[0].len() {
            let mut state = before;
            for step in steps
                .iter()
                .take_while(|s| usize::try_from(s.offset).unwrap() <= frame)
            {
                state.apply(*step);
            }
            let [gain, wet, depth] = state.values::<S>();
            let detector = if self.side.is_empty() {
                &self.seed
            } else {
                &self.side
            }
            .iter()
            .map(|s| s[frame].abs())
            .fold(S::from_value(0.0), |a, b| if a > b { a } else { b });
            let one = S::from_value(1.0);
            let attenuation = if detector > S::from_value(0.5) {
                one - depth
            } else {
                one
            };
            for (out, input) in output.iter_mut().zip(&self.seed) {
                out[frame] = input[frame] * ((one - wet) + wet * gain * attenuation);
            }
        }
        output
    }
}
struct Source<'a, S> {
    buffers: &'a mut Buffers<S>,
    ports: Ports,
}
impl<S: Sample> ProcessBufferSource<S> for Source<'_, S> {
    fn with_channels(
        &mut self,
        visitor: &mut dyn FnMut(&mut dyn Iterator<Item = ChannelBuffer<'_, S>>),
    ) {
        let ports = self.ports;
        let side = self
            .buffers
            .side
            .iter()
            .enumerate()
            .map(|(c, s)| ChannelBuffer::InputOnly {
                input: InputEndpoint::new(ports.side, u32::try_from(c).unwrap()),
                samples: s,
            });
        if self.buffers.inplace {
            let main = self.buffers.main.iter_mut().enumerate().map(|(c, s)| {
                let c = u32::try_from(c).unwrap();
                ChannelBuffer::InPlace {
                    input: InputEndpoint::new(ports.input, c),
                    output: OutputEndpoint::new(ports.output, c),
                    samples: s,
                }
            });
            visitor(&mut main.chain(side));
        } else {
            let main = self
                .buffers
                .main
                .iter()
                .zip(&mut self.buffers.output)
                .enumerate()
                .map(|(c, (i, o))| {
                    let c = u32::try_from(c).unwrap();
                    ChannelBuffer::Separate {
                        input: InputEndpoint::new(ports.input, c),
                        output: OutputEndpoint::new(ports.output, c),
                        input_samples: i,
                        output_samples: o,
                    }
                });
            visitor(&mut main.chain(side));
        }
    }
}
// Owned heap tables remain stable across moves. Derive each table base once;
// do not use the pinned host's multiport builder or create sample references.
struct Tables<S> {
    _inputs: Vec<*mut S>,
    _side: Vec<*mut S>,
    _outputs: Vec<*mut S>,
    inputs: Vec<clap_audio_buffer>,
    outputs: Vec<clap_audio_buffer>,
}
impl<S: Sample> Tables<S> {
    fn new(buffers: &mut Buffers<S>) -> Self {
        let mut input: Vec<_> = buffers.main.iter_mut().map(Vec::as_mut_ptr).collect();
        let mut side: Vec<_> = buffers.side.iter_mut().map(Vec::as_mut_ptr).collect();
        let mut output = if buffers.inplace {
            input.clone()
        } else {
            buffers.output.iter_mut().map(Vec::as_mut_ptr).collect()
        };
        let mut inputs = vec![S::descriptor(input.as_mut_ptr(), input.len())];
        if !side.is_empty() {
            inputs.push(S::descriptor(side.as_mut_ptr(), side.len()));
        }
        let outputs = vec![S::descriptor(output.as_mut_ptr(), output.len())];
        Self {
            _inputs: input,
            _side: side,
            _outputs: output,
            inputs,
            outputs,
        }
    }
}
struct Host;
struct Shared;
struct Main;
struct Audio;
impl SharedHandler<'_> for Shared {
    fn request_restart(&self) {}
    fn request_process(&self) {}
    fn request_callback(&self) {}
}
impl MainThreadHandler<'_> for Main {}
impl AudioProcessorHandler<'_> for Audio {}
impl HostHandlers for Host {
    type Shared<'a> = Shared;
    type MainThread<'a> = Main;
    type AudioProcessor<'a> = Audio;
}
struct Clap {
    plugin: PluginInstance<Host>,
    processor: Option<StartedPluginAudioProcessor<Host>>,
}
impl Clap {
    fn new<const N: usize, const SC: usize, const INDEPENDENT: bool>(frames: u32) -> Self {
        let entry = PluginEntry::load_from_clack::<
            SingleComponentEntry<Effect<N, SC, INDEPENDENT>, F32AndF64>,
        >(c"")
        .unwrap();
        let mut plugin = PluginInstance::new(
            |()| Shared,
            |_| Main,
            &entry,
            c"org.nijaru.chassis.cost-probe",
            &HostInfo::new("cost-probe", "", "", "").unwrap(),
        )
        .unwrap();
        let processor = plugin
            .activate(
                |_, _| Audio,
                PluginAudioConfiguration {
                    sample_rate: 48_000.0,
                    min_frames_count: 1,
                    max_frames_count: frames,
                },
            )
            .unwrap()
            .start_processing()
            .unwrap();
        Self {
            plugin,
            processor: Some(processor),
        }
    }
}
impl Drop for Clap {
    fn drop(&mut self) {
        self.plugin
            .deactivate(self.processor.take().unwrap().stop_processing());
    }
}
#[derive(Clone, Copy)]
pub enum Path {
    Bare,
    Runtime,
    Clap,
}
impl Path {
    pub fn name(self) -> &'static str {
        match self {
            Self::Bare => "bare-trusted",
            Self::Runtime => "runtime",
            Self::Clap => "clap",
        }
    }
}
enum Backend<const N: usize, const SC: usize, const INDEPENDENT: bool> {
    Bare(Controls),
    Runtime(Box<InstanceRuntime<Dsp<N, SC, INDEPENDENT>>>),
    Clap(Clap),
}
struct Trial<S, const N: usize, const SC: usize, const INDEPENDENT: bool> {
    backend: Backend<N, SC, INDEPENDENT>,
    buffers: Buffers<S>,
    tables: Tables<S>,
    events: Events,
    ports: Ports,
    frames: u32,
}
pub trait Runner {
    fn path(&self) -> Path;
    fn strategy(&self) -> &'static str;
    fn render(&mut self, events: bool);
}
impl<S: Sample, const N: usize, const SC: usize, const INDEPENDENT: bool>
    Trial<S, N, SC, INDEPENDENT>
{
    fn new(path: Path, frames: u32, count: u32, inplace: bool) -> Self {
        let component = Effect::<N, SC, INDEPENDENT>;
        let mut runtime = InstanceRuntime::for_component(&component).unwrap();
        let ports = Ports::new(runtime.schema());
        let backend = match path {
            Path::Bare => Backend::Bare(Controls::default()),
            Path::Runtime => {
                runtime
                    .activate(
                        &component,
                        ProcessConfig::new(
                            48_000.0,
                            NonZeroU32::new(1),
                            NonZeroU32::new(frames).unwrap(),
                            MAX_EVENTS,
                        )
                        .unwrap(),
                        AudioIoConfiguration::new(&configuration::<N, SC>()),
                    )
                    .unwrap();
                Backend::Runtime(Box::new(runtime))
            }
            Path::Clap => Backend::Clap(Clap::new::<N, SC, INDEPENDENT>(frames)),
        };
        let mut buffers = Buffers::new::<N, SC>(frames, inplace);
        let tables = Tables::new(&mut buffers);
        let mut trial = Self {
            backend,
            buffers,
            tables,
            events: Events::new(frames, count),
            ports,
            frames,
        };
        trial.verify();
        trial
    }
    fn verify(&mut self) {
        if !self.events.steps.is_empty() {
            assert!(
                self.events
                    .steady_changes(self.frames)
                    .into_iter()
                    .all(|changed| changed),
                "every effective parameter must change within a steady-state callback"
            );
        }
        let mut state = Controls::default();
        // First use, held controls with no events, and repeated steady-state use.
        for with_events in [true, false, true] {
            let steps = if with_events {
                &self.events.steps[..]
            } else {
                &[]
            };
            let expected = self.buffers.expected(state, steps);
            for step in steps {
                state.apply(*step);
            }
            self.render(with_events);
            assert_eq!(
                self.buffers.output(),
                expected,
                "{} {} {}",
                S::NAME,
                self.path().name(),
                self.strategy()
            );
            if !self.buffers.inplace {
                assert_eq!(self.buffers.main, self.buffers.seed);
            }
            match &mut self.backend {
                Backend::Bare(actual) => actual.assert_eq(state),
                Backend::Runtime(runtime) => {
                    for (i, key) in KEYS.iter().enumerate() {
                        let handle = runtime.parameters().float_parameter(key).unwrap();
                        assert_eq!(
                            handle.value(runtime.parameters()).unwrap().to_bits(),
                            state.base[i].to_bits()
                        );
                        assert_eq!(
                            runtime.modulation(handle.index()).unwrap().to_bits(),
                            state.amount[i].to_bits()
                        );
                    }
                }
                Backend::Clap(clap) => {
                    let handle = clap.plugin.plugin_handle();
                    let params = handle.get_extension::<PluginParams>().unwrap();
                    for (i, id) in IDS.iter().enumerate() {
                        assert_eq!(
                            params
                                .get_value(&handle, ClapId::new(*id))
                                .unwrap()
                                .to_bits(),
                            state.base[i].to_bits()
                        );
                    }
                }
            }
        }
    }
}
impl<S: Sample, const N: usize, const SC: usize, const INDEPENDENT: bool> Runner
    for Trial<S, N, SC, INDEPENDENT>
{
    fn path(&self) -> Path {
        match self.backend {
            Backend::Bare(_) => Path::Bare,
            Backend::Runtime(_) => Path::Runtime,
            Backend::Clap(_) => Path::Clap,
        }
    }
    fn strategy(&self) -> &'static str {
        if INDEPENDENT { "independent" } else { "shared" }
    }
    fn render(&mut self, with_events: bool) {
        // Include identical input refill in every path and relationship. Otherwise
        // repeated in-place processing would benchmark cumulative attenuation.
        self.buffers.refill();
        match &mut self.backend {
            Backend::Bare(state) => {
                let steps = black_box(if with_events {
                    &self.events.steps[..]
                } else {
                    &[]
                });
                let mut states = [*state; N];
                let mut next = [0_usize; N];
                // Ordinary slices and a trusted combined step evaluator: no
                // runtime, channel views, source traversal or publication.
                let mut evaluate = |frame: u32, channel: usize| {
                    while let Some(step) = steps.get(next[channel]).filter(|s| s.offset <= frame) {
                        states[channel].apply(*step);
                        next[channel] += 1;
                    }
                    states[channel].values::<S>()
                };
                for frame in 0..usize::try_from(self.frames).unwrap() {
                    let detector_inputs = if SC == 0 {
                        &self.buffers.main
                    } else {
                        &self.buffers.side
                    };
                    let detector = detector_inputs
                        .iter()
                        .map(|s| s[frame].abs())
                        .fold(S::from_value(0.0), |a, b| if a > b { a } else { b });
                    let offset = u32::try_from(frame).unwrap();
                    let shared = if INDEPENDENT {
                        None
                    } else {
                        Some(evaluate(offset, 0))
                    };
                    for channel in 0..N {
                        let controls = match shared {
                            Some(value) => value,
                            None => evaluate(offset, channel),
                        };
                        let sample = kernel(self.buffers.main[channel][frame], detector, controls);
                        if self.buffers.inplace {
                            self.buffers.main[channel][frame] = sample;
                        } else {
                            self.buffers.output[channel][frame] = sample;
                        }
                    }
                }
                *state = states[0];
            }
            Backend::Runtime(runtime) => {
                let events = ParameterEvents::new(
                    if with_events {
                        &self.events.core[..]
                    } else {
                        &[]
                    },
                    self.frames,
                    MAX_EVENTS,
                )
                .unwrap();
                let context =
                    ProcessContext::new(ProcessMode::Realtime, &TransportSnapshot::UNKNOWN, events);
                let mut source = Source {
                    buffers: &mut self.buffers,
                    ports: self.ports,
                };
                runtime
                    .process_source(self.frames, context, &mut source)
                    .unwrap();
            }
            Backend::Clap(clap) => {
                let events = if with_events {
                    self.events.clap.as_input()
                } else {
                    InputEvents::empty()
                };
                let mut output = OutputEvents::void();
                let process = clap_process {
                    steady_time: -1,
                    frames_count: self.frames,
                    transport: ptr::null(),
                    audio_inputs: self.tables.inputs.as_ptr(),
                    audio_outputs: self.tables.outputs.as_mut_ptr(),
                    audio_inputs_count: u32::try_from(self.tables.inputs.len()).unwrap(),
                    audio_outputs_count: 1,
                    in_events: events.as_raw(),
                    out_events: output.as_raw(),
                };
                let handle = clap.processor.as_mut().unwrap().plugin_handle();
                let raw = handle.as_raw_ptr();
                // SAFETY: started, exclusively invoked processor; owned descriptors,
                // stable heap tables and initialized sample storage outlive the call.
                // Only declared reciprocal main pairs alias exactly; sidechains and
                // separate outputs are disjoint. No sample references survive here.
                let status = unsafe { ((*raw).process.unwrap())(raw, ptr::from_ref(&process)) };
                assert_eq!(status, CLAP_PROCESS_CONTINUE);
            }
        }
        black_box(self.buffers.output());
    }
}

/// Construct all six equivalent paths; every path verifies all audio and
/// completed base controls, including a subsequent event-free block, before use.
pub fn runners<S: Sample, const N: usize, const SC: usize>(
    frames: u32,
    count: u32,
    inplace: bool,
) -> Vec<Box<dyn Runner>> {
    let mut runners: Vec<Box<dyn Runner>> = Vec::new();
    for path in [Path::Bare, Path::Runtime, Path::Clap] {
        runners.push(Box::new(Trial::<S, N, SC, false>::new(
            path, frames, count, inplace,
        )));
    }
    for path in [Path::Bare, Path::Runtime, Path::Clap] {
        runners.push(Box::new(Trial::<S, N, SC, true>::new(
            path, frames, count, inplace,
        )));
    }
    runners
}
