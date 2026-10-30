#![allow(unsafe_code)]
//! Actual process-ABI transport projection, discontinuities, and realtime bounds.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    convert::Infallible,
};

use chassis_clap::{ClapAudioPort, ClapComponent, F32AndF64, SingleComponentEntry};
use chassis_runtime::{
    audio::{
        AudioIoConfigurationSpec, AudioIoPolicy, AudioPortDescriptor, AudioPortDirection,
        AudioPortKey, AudioPortRole, ChannelLayout, ConfiguredAudioPort,
    },
    process::{ActivationConfig, ProcessBlock},
    runtime::{Component, Process, ProcessError, Processor},
    schema::{ComponentId, ComponentSchema, ComponentSchemaError, StateSchemaVersion},
    transport::TransportValues,
};
use clack_host::{
    events::{
        EventFlags, EventHeader,
        event_types::{TransportEvent, TransportFlags},
    },
    factory::plugin::PluginFactory,
    prelude::*,
    utils::FixedPoint,
};

thread_local! {
    static COUNTS: Cell<Option<(u32, u32)>> = const { Cell::new(None) };
    static STEADY_TIME: Cell<u64> = const { Cell::new(900_000) };
}
struct Allocator;
// SAFETY: allocator calls forward the original arguments and result to System.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = COUNTS.try_with(|counts| {
            if let Some((allocations, deallocations)) = counts.get() {
                counts.set(Some((allocations + 1, deallocations)));
            }
        });
        // SAFETY: forwards the caller's valid layout unchanged.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        let _ = COUNTS.try_with(|counts| {
            if let Some((allocations, deallocations)) = counts.get() {
                counts.set(Some((allocations, deallocations + 1)));
            }
        });
        // SAFETY: forwards the original matching allocation and layout.
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static GLOBAL: Allocator = Allocator;

const OUTPUT: AudioPortKey = AudioPortKey::new("out");
const FIELDS: usize = 17;
#[derive(Default)]
struct TransportProbe;
impl Component for TransportProbe {
    type Processor = Self;
    type ActivationError = Infallible;
    fn schema(&self) -> Result<ComponentSchema, ComponentSchemaError> {
        ComponentSchema::new(
            ComponentId::new("test.transport").unwrap(),
            StateSchemaVersion::new(1),
            vec![AudioPortDescriptor::new(
                OUTPUT,
                "Output",
                AudioPortDirection::Output,
                AudioPortRole::Main,
                false,
            )],
            AudioIoPolicy::enumerated(vec![AudioIoConfigurationSpec::new(vec![
                ConfiguredAudioPort::new(OUTPUT, ChannelLayout::Mono),
            ])]),
            vec![],
            vec![],
        )
    }
    fn activate(
        &self,
        _: &ActivationConfig<'_>,
        _: &chassis_runtime::parameters::ParameterStore,
        _: &[chassis_runtime::state::StateEntry],
    ) -> Result<Self, Infallible> {
        Ok(Self)
    }
}
impl Processor for TransportProbe {}
impl ClapComponent for TransportProbe {
    const CLAP_DESCRIPTOR: chassis_clap::ClapDescriptor = chassis_clap::ClapDescriptor {
        features: &[c"audio-effect", c"mono"],
        version: env!("CARGO_PKG_VERSION"),
        ..chassis_clap::ClapDescriptor::new("test.transport", "Transport probe")
    };
    const CLAP_AUDIO_PORTS: &'static [ClapAudioPort] =
        &[ClapAudioPort::new(OUTPUT, 1, ChannelLayout::Mono, None)];
}

#[expect(
    clippy::cast_precision_loss,
    reason = "test observation uses small bar numbers; CLAP has no sample position"
)]
fn observations(values: &TransportValues) -> [f64; FIELDS] {
    let unknown = f64::NAN;
    [
        values.playing.map_or(unknown, f64::from),
        values.recording.map_or(unknown, f64::from),
        values.loop_active.map_or(unknown, f64::from),
        values.within_pre_roll.map_or(unknown, f64::from),
        values.tempo_bpm.unwrap_or(unknown),
        values.tempo_increment.unwrap_or(unknown),
        values
            .sample_position
            .map_or(unknown, |position| position as f64),
        values.seconds_position.unwrap_or(unknown),
        values.beat_position.unwrap_or(unknown),
        values.bar_start_beats.unwrap_or(unknown),
        values.bar_number.map_or(unknown, |number| number as f64),
        values
            .time_signature
            .map_or(unknown, |signature| f64::from(signature.numerator)),
        values
            .time_signature
            .map_or(unknown, |signature| f64::from(signature.denominator)),
        values.loop_beats.map_or(unknown, |range| range.start),
        values.loop_beats.map_or(unknown, |range| range.end),
        values.loop_seconds.map_or(unknown, |range| range.start),
        values.loop_seconds.map_or(unknown, |range| range.end),
    ]
}
impl Process<f64> for TransportProbe {
    fn process(&mut self, block: &mut ProcessBlock<'_, f64>) -> Result<(), ProcessError> {
        let values = observations(block.transport().values());
        block.for_each_channel(|mut channel| {
            channel.output_mut().unwrap().copy_from_slice(&values);
        });
        Ok(())
    }
}
impl Process<f32> for TransportProbe {
    fn process(&mut self, block: &mut ProcessBlock<'_, f32>) -> Result<(), ProcessError> {
        // This fixture's observations use f64 so tempo slope is not truncated.
        block.for_each_channel(|mut channel| {
            channel.output_mut().unwrap().fill(0.0);
        });
        Ok(())
    }
}

struct Host;
impl SharedHandler<'_> for Host {
    fn request_restart(&self) {}
    fn request_process(&self) {}
    fn request_callback(&self) {}
}
impl MainThreadHandler<'_> for Host {}
impl AudioProcessorHandler<'_> for Host {}
impl HostHandlers for Host {
    type Shared<'a> = Host;
    type MainThread<'a> = Host;
    type AudioProcessor<'a> = Host;
}
fn plugin() -> PluginInstance<Host> {
    let entry =
        PluginEntry::load_from_clack::<SingleComponentEntry<TransportProbe, F32AndF64>>(c"")
            .unwrap();
    let id = entry
        .get_factory::<PluginFactory>()
        .unwrap()
        .plugin_descriptor(0)
        .unwrap()
        .id()
        .unwrap();
    PluginInstance::new(
        |()| Host,
        |_| Host,
        &entry,
        id,
        &HostInfo::new("transport test", "", "", "").unwrap(),
    )
    .unwrap()
}
fn activate(plugin: &mut PluginInstance<Host>) -> StartedPluginAudioProcessor<Host> {
    plugin
        .activate(
            |_, _| Host,
            PluginAudioConfiguration {
                sample_rate: 48_000.0,
                min_frames_count: 1,
                max_frames_count: u32::try_from(FIELDS).unwrap(),
            },
        )
        .unwrap()
        .start_processing()
        .unwrap()
}
fn invoke(
    processor: &mut StartedPluginAudioProcessor<Host>,
    transport: Option<&TransportEvent>,
) -> Result<[f64; FIELDS], PluginInstanceError> {
    // Sentinel output also proves invalid metadata is rejected before DSP.
    let mut samples = [999.0_f64; FIELDS];
    let mut inputs = AudioPorts::with_capacity(0, 0);
    let mut outputs = AudioPorts::with_capacity(1, 1);
    let input_audio = inputs.with_input_buffers(core::iter::empty::<
        AudioPortBuffer<core::iter::Empty<InputChannel<f32>>, core::iter::Empty<InputChannel<f64>>>,
    >());
    let mut output_audio = outputs.with_output_buffers([AudioPortBuffer {
        channels: AudioPortBufferType::f64_output_only([samples.as_mut_slice()]),
        latency: 0,
    }]);
    let input_events = EventBuffer::with_capacity(0);
    let mut output_events = EventBuffer::with_capacity(0);
    let steady_time = STEADY_TIME.with(|time| {
        let current = time.get();
        time.set(current + u64::try_from(FIELDS).unwrap());
        current
    });
    COUNTS.with(|counts| counts.set(Some((0, 0))));
    let result = processor.process(
        &input_audio,
        &mut output_audio,
        &input_events.as_input(),
        &mut output_events.as_output(),
        Some(steady_time),
        transport,
    );
    let counts = COUNTS.with(|counts| counts.replace(None)).unwrap();
    assert_eq!(
        counts,
        (0, 0),
        "accepted and rejected callbacks allocate/deallocate nothing"
    );
    if result.is_err() {
        assert_eq!(samples.map(f64::to_bits), [999.0_f64.to_bits(); FIELDS]);
    }
    result.map(|_| samples)
}
fn transport(flags: TransportFlags) -> TransportEvent {
    TransportEvent {
        header: EventHeader::new_core(0, EventFlags::empty()),
        flags,
        song_pos_beats: (-7.25).into(),
        song_pos_seconds: (-3.5).into(),
        tempo: 123.0,
        tempo_inc: -0.001,
        loop_start_beats: (-8.0).into(),
        loop_end_beats: 16.0.into(),
        loop_start_seconds: (-4.0).into(),
        loop_end_seconds: 8.0.into(),
        bar_start: (-10.5).into(),
        bar_number: -3,
        time_signature_numerator: 7,
        time_signature_denominator: 8,
    }
}
fn all_fields() -> TransportFlags {
    TransportFlags::HAS_TEMPO
        | TransportFlags::HAS_BEATS_TIMELINE
        | TransportFlags::HAS_SECONDS_TIMELINE
        | TransportFlags::HAS_TIME_SIGNATURE
}
fn assert_observations(actual: [f64; FIELDS], expected: [f64; FIELDS]) {
    for (index, (actual, expected)) in actual.into_iter().zip(expected).enumerate() {
        if expected.is_nan() {
            assert!(actual.is_nan(), "field {index} must be unavailable");
        } else {
            assert_eq!(actual.to_bits(), expected.to_bits(), "field {index}");
        }
    }
}

#[test]
fn projects_musical_and_seconds_timelines_without_inventing_a_sample_clock() {
    let mut plugin = plugin();
    let mut processor = activate(&mut plugin);
    let event = transport(
        all_fields()
            | TransportFlags::IS_PLAYING
            | TransportFlags::IS_RECORDING
            | TransportFlags::IS_LOOP_ACTIVE
            | TransportFlags::IS_WITHIN_PRE_ROLL,
    );
    assert_observations(
        invoke(&mut processor, Some(&event)).unwrap(),
        [
            1.0,
            1.0,
            1.0,
            1.0,
            123.0,
            -0.001,
            f64::NAN,
            -3.5,
            -7.25,
            -10.5,
            -3.0,
            7.0,
            8.0,
            -8.0,
            16.0,
            -4.0,
            8.0,
        ],
    );
    // Stopped transport retains positions; inactive CLAP loop storage is unavailable.
    let stopped = transport(all_fields());
    let mut expected = observations_from_stopped();
    assert_observations(invoke(&mut processor, Some(&stopped)).unwrap(), expected);
    // A seek is a fresh host observation, never an accumulated process clock.
    let mut seek = stopped;
    seek.song_pos_beats = 42.5.into();
    seek.song_pos_seconds = 81.0.into();
    expected[7] = 81.0;
    expected[8] = 42.5;
    assert_observations(invoke(&mut processor, Some(&seek)).unwrap(), expected);
    assert_observations(invoke(&mut processor, None).unwrap(), [f64::NAN; FIELDS]);
    plugin.deactivate(processor.stop_processing());
}
fn observations_from_stopped() -> [f64; FIELDS] {
    [
        0.0,
        0.0,
        0.0,
        0.0,
        123.0,
        -0.001,
        f64::NAN,
        -3.5,
        -7.25,
        -10.5,
        -3.0,
        7.0,
        8.0,
        f64::NAN,
        f64::NAN,
        f64::NAN,
        f64::NAN,
    ]
}

#[test]
fn validity_flags_are_independent_and_unavailable_storage_is_not_validated() {
    let mut plugin = plugin();
    let mut processor = activate(&mut plugin);
    for flags in [
        TransportFlags::empty(),
        TransportFlags::HAS_TEMPO,
        TransportFlags::HAS_BEATS_TIMELINE,
        TransportFlags::HAS_SECONDS_TIMELINE,
        TransportFlags::HAS_TIME_SIGNATURE,
    ] {
        let event = transport(flags | TransportFlags::IS_LOOP_ACTIVE);
        let mut expected = [f64::NAN; FIELDS];
        expected[..4].fill(0.0);
        expected[2] = 1.0;
        if flags.contains(TransportFlags::HAS_TEMPO) {
            expected[4..6].copy_from_slice(&[123.0, -0.001]);
        }
        if flags.contains(TransportFlags::HAS_BEATS_TIMELINE) {
            expected[8..11].copy_from_slice(&[-7.25, -10.5, -3.0]);
            expected[13..15].copy_from_slice(&[-8.0, 16.0]);
        }
        if flags.contains(TransportFlags::HAS_SECONDS_TIMELINE) {
            expected[7] = -3.5;
            expected[15..17].copy_from_slice(&[-4.0, 8.0]);
        }
        if flags.contains(TransportFlags::HAS_TIME_SIGNATURE) {
            expected[11..13].copy_from_slice(&[7.0, 8.0]);
        }
        assert_observations(invoke(&mut processor, Some(&event)).unwrap(), expected);
    }
    let mut unavailable = transport(TransportFlags::empty());
    unavailable.tempo = f64::NAN;
    unavailable.tempo_inc = f64::INFINITY;
    unavailable.time_signature_denominator = 0;
    unavailable.loop_end_beats = (-99.0).into();
    unavailable.loop_end_seconds = (-99.0).into();
    let mut expected = [f64::NAN; FIELDS];
    expected[..4].fill(0.0);
    assert_observations(
        invoke(&mut processor, Some(&unavailable)).unwrap(),
        expected,
    );
    // Invalid optional BPM follows the established recovery policy. Discard its
    // dependent slope too, rather than manufacturing a tempo or failing DSP.
    for bpm in [
        0.0,
        -23.558_807_703_497_62,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ] {
        let mut event = transport(all_fields());
        event.tempo = bpm;
        event.tempo_inc = f64::NAN;
        let mut expected = observations_from_stopped();
        expected[4] = f64::NAN;
        expected[5] = f64::NAN;
        assert_observations(invoke(&mut processor, Some(&event)).unwrap(), expected);
    }
    // The validator and ordinary wrappers do not promise inactive loop ranges,
    // even with both timelines available. Poisoned endpoints must stay unread.
    let mut inactive = transport(all_fields());
    inactive.loop_start_beats = FixedPoint::from_bits(i64::MAX);
    inactive.loop_end_beats = FixedPoint::from_bits(i64::MIN);
    inactive.loop_start_seconds = inactive.loop_start_beats;
    inactive.loop_end_seconds = inactive.loop_end_beats;
    assert_observations(
        invoke(&mut processor, Some(&inactive)).unwrap(),
        observations_from_stopped(),
    );
    plugin.deactivate(processor.stop_processing());
}

#[test]
fn invalid_available_metadata_fails_before_dsp_without_callback_allocation() {
    let original = transport(all_fields() | TransportFlags::IS_LOOP_ACTIVE);
    let mut invalid = [original; 6];
    invalid[0].tempo_inc = f64::INFINITY;
    invalid[1].time_signature_denominator = 0;
    invalid[2].loop_end_beats = (-9.0).into();
    invalid[3].loop_end_seconds = (-5.0).into();
    // Distinct fixed-point endpoints collapse to the same f64 at this scale.
    // Reversal must still be rejected at the source, before projection.
    invalid[4].loop_start_beats = FixedPoint::from_bits(9_007_199_254_740_993);
    invalid[4].loop_end_beats = FixedPoint::from_bits(9_007_199_254_740_992);
    invalid[5].loop_start_seconds = invalid[4].loop_start_beats;
    invalid[5].loop_end_seconds = invalid[4].loop_end_beats;
    for event in invalid {
        // Process failure terminates this run; do not promise DSP recovery.
        let mut plugin = plugin();
        let mut processor = activate(&mut plugin);
        invoke(&mut processor, Some(&original)).unwrap();
        let result = invoke(&mut processor, Some(&event));
        plugin.deactivate(processor.stop_processing());
        assert!(result.is_err());
    }
}
