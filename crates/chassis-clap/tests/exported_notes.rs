#![allow(unsafe_code)]
//! Real exported native-note ABI: topology, mixed timing, voice feedback and failure.

use clack_extensions::{
    audio_ports::{AudioPortInfoBuffer, PluginAudioPorts},
    note_ports::{NoteDialect, NoteDialects, NotePortInfoBuffer, PluginNotePorts},
    params::PluginParams,
};
use clack_host::{
    events::{
        Match, UnknownEvent,
        event_types::{
            NoteChokeEvent, NoteEndEvent, NoteExpressionEvent, NoteExpressionType, NoteOffEvent,
            NoteOnEvent, ParamValueEvent,
        },
        io::{InputEventBuffer, OutputEventBuffer, TryPushError},
    },
    prelude::*,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    sync::atomic::{AtomicU64, Ordering},
};

#[path = "../../../examples/clap-note-probe/src/lib.rs"]
mod exported;

thread_local! { static COUNT: Cell<bool> = const { Cell::new(false) }; }
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static DEALLOCATIONS: AtomicU64 = AtomicU64::new(0);
struct Allocator;
// SAFETY: every allocation/deallocation is forwarded unchanged to System.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNT.try_with(Cell::get).unwrap_or(false) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: original allocator contract.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if COUNT.try_with(Cell::get).unwrap_or(false) {
            DEALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: original allocation pair.
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static GLOBAL: Allocator = Allocator;

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
    // SAFETY: the actual exported fixture's static entry and matching CLAP ABI.
    let entry = unsafe { PluginEntry::load_from_raw(&exported::clap_entry, c"") }.unwrap();
    PluginInstance::new(
        |()| Host,
        |_| Host,
        &entry,
        c"com.nijaru.chassis.note-probe",
        &HostInfo::new("notes", "", "", "").unwrap(),
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
                max_frames_count: 8,
            },
        )
        .unwrap()
        .start_processing()
        .unwrap()
}

struct Capture {
    events: Vec<NoteEndEvent>,
    reject: bool,
}
impl Capture {
    fn new() -> Self {
        Self {
            events: Vec::with_capacity(1030),
            reject: false,
        }
    }
}
impl OutputEventBuffer for Capture {
    fn try_push(&mut self, event: &UnknownEvent) -> Result<(), TryPushError> {
        if self.reject || self.events.len() == self.events.capacity() {
            return Err(TryPushError::new());
        }
        let event = event
            .as_event::<NoteEndEvent>()
            .ok_or_else(TryPushError::new)?;
        self.events.push(*event);
        Ok(())
    }
}

type Render = fn(
    &mut StartedPluginAudioProcessor<Host>,
    &InputEvents<'_>,
    &mut Capture,
) -> (bool, [[f64; 8]; 2]);
macro_rules! renderer {
    ($name:ident, $sample:ty, $output:ident) => {
        fn $name(
            processor: &mut StartedPluginAudioProcessor<Host>,
            events: &InputEvents<'_>,
            capture: &mut Capture,
        ) -> (bool, [[f64; 8]; 2]) {
            let mut samples = [[-99.0 as $sample; 8]; 2];
            let mut ports = AudioPorts::with_capacity(2, 1);
            let mut audio = ports.with_output_buffers([AudioPortBuffer {
                channels: AudioPortBufferType::$output(
                    samples.iter_mut().map(|channel| &mut channel[..]),
                ),
                latency: 0,
            }]);
            let input = InputAudioBuffers::empty();
            COUNT.set(true);
            let result = processor.process(
                &input,
                &mut audio,
                events,
                &mut OutputEvents::from_buffer(capture),
                None,
                None,
            );
            COUNT.set(false);
            (
                result.is_ok(),
                samples.map(|channel| channel.map(f64::from)),
            )
        }
    };
}
renderer!(render_f32, f32, f32_output_only);
renderer!(render_f64, f64, f64_output_only);

fn target(port: u16, key: u16, id: u32) -> Pckn {
    Pckn::new(port, 0_u16, key, id)
}
fn set(events: &mut EventBuffer, offset: u32, value: f64) {
    events.push(&ParamValueEvent::new(
        offset,
        ClapId::new(7007),
        Pckn::match_all(),
        value,
    ));
}
fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-7, "{actual} != {expected}");
}

#[test]
fn exported_instrument_metadata_uses_stable_ids_and_direction_local_indices() {
    let mut plugin = plugin();
    let handle = plugin.plugin_handle();
    let notes = handle.get_extension::<PluginNotePorts>().unwrap();
    assert_eq!(notes.count(&handle, true), 2);
    assert_eq!(notes.count(&handle, false), 0);
    for (index, id, name) in [
        (0, 10, b"Secondary Notes".as_slice()),
        (1, 90, b"Primary Notes".as_slice()),
    ] {
        let mut buffer = NotePortInfoBuffer::new();
        let info = notes.get(&handle, index, true, &mut buffer).unwrap();
        assert_eq!(info.id, ClapId::new(id));
        assert_eq!(info.name, name);
        assert_eq!(info.supported_dialects, NoteDialects::CLAP);
        assert_eq!(info.preferred_dialect, Some(NoteDialect::Clap));
    }
    assert!(
        notes
            .get(&handle, 2, true, &mut NotePortInfoBuffer::new())
            .is_none()
    );
    let audio = handle.get_extension::<PluginAudioPorts>().unwrap();
    assert_eq!(audio.count(&handle, true), 0);
    assert_eq!(audio.count(&handle, false), 1);
    let mut buffer = AudioPortInfoBuffer::new();
    let info = audio.get(&handle, 0, false, &mut buffer).unwrap();
    assert_eq!(info.channel_count, 2);
}

#[test]
fn mixed_order_expression_and_voice_feedback_are_sample_accurate_in_both_precisions() {
    let allocations = ALLOCATIONS.load(Ordering::Relaxed);
    let deallocations = DEALLOCATIONS.load(Ordering::Relaxed);
    for render in [render_f32 as Render, render_f64] {
        let mut plugin = plugin();
        let mut processor = activate(&mut plugin);
        let mut events = EventBuffer::with_capacity(12);
        set(&mut events, 0, 0.25);
        events.push(&NoteExpressionEvent::new(
            0,
            target(1, 60, 42),
            NoteExpressionType::Volume,
            2.0,
        ));
        events.push(&NoteOnEvent::new(0, target(1, 60, 42), 1.0));
        set(&mut events, 0, 0.75); // Must not change the already captured strike.
        events.push(&NoteExpressionEvent::new(
            0,
            target(1, 60, 42),
            NoteExpressionType::Pan,
            0.0,
        ));
        events.push(&NoteOffEvent::new(
            2,
            Pckn::new(1_u16, 0_u16, 60_u16, Match::All),
            0.0,
        ));
        events.push(&NoteOnEvent::new(3, target(0, 62, 43), 1.0));
        events.push(&NoteChokeEvent::new(5, Pckn::match_all()));
        let mut capture = Capture::new();
        let (ok, samples) = render(&mut processor, &events.as_input(), &mut capture);
        assert!(ok);
        near(samples[0][0], 0.025);
        near(samples[1][0], 0.0);
        near(samples[0][2], 0.0);
        near(samples[0][3], 0.01875);
        near(samples[1][3], 0.01875);
        near(samples[0][5], 0.0);
        let tuples: Vec<_> = capture
            .events
            .iter()
            .map(|event| {
                let raw = event.as_raw();
                (raw.port_index, raw.channel, raw.key, raw.note_id)
            })
            .collect();
        assert_eq!(tuples, [(1, 0, 60, 42), (0, 0, 62, 43)]);
        plugin.deactivate(processor.stop_processing());
    }
    assert_eq!(ALLOCATIONS.load(Ordering::Relaxed), allocations);
    assert_eq!(DEALLOCATIONS.load(Ordering::Relaxed), deallocations);
}

#[test]
fn native_callback_bounds_cover_notes_mixed_controls_expressions_and_overflow() {
    let before = (
        ALLOCATIONS.load(Ordering::Relaxed),
        DEALLOCATIONS.load(Ordering::Relaxed),
    );
    for render in [render_f32 as Render, render_f64] {
        let mut plugin = plugin();
        let mut processor = activate(&mut plugin);
        let mut capture = Capture::new();
        let mut events = EventBuffer::with_capacity(1025);
        for id in 0..1024_u32 {
            events.push(&NoteOnEvent::new(0, target(0, 60, id), 1.0));
        }
        assert!(render(&mut processor, &events.as_input(), &mut capture).0);
        assert_eq!(capture.events.len(), 1023);
        capture.events.clear();
        events.clear();
        for id in 0..512_u32 {
            set(&mut events, 0, 0.5);
            events.push(&NoteOnEvent::new(0, target(1, 60, id), 1.0));
        }
        assert!(render(&mut processor, &events.as_input(), &mut capture).0);
        assert_eq!(capture.events.len(), 512);
        capture.events.clear();
        events.clear();
        for _ in 0..1024 {
            events.push(&NoteExpressionEvent::new(
                0,
                Pckn::match_all(),
                NoteExpressionType::Expression,
                0.5,
            ));
        }
        assert!(render(&mut processor, &events.as_input(), &mut capture).0);
        set(&mut events, 0, 0.5); // Total raw budget, independently of family bounds.
        let (ok, samples) = render(&mut processor, &events.as_input(), &mut capture);
        assert!(!ok);
        assert_eq!(samples, [[-99.0; 8]; 2]);
        plugin.deactivate(processor.stop_processing());
    }
    assert_eq!(
        (
            ALLOCATIONS.load(Ordering::Relaxed),
            DEALLOCATIONS.load(Ordering::Relaxed)
        ),
        before
    );
}

#[test]
fn zero_velocity_is_an_on_and_same_tuple_retrigger_does_not_end_the_new_voice() {
    let mut plugin = plugin();
    let mut processor = activate(&mut plugin);
    let mut capture = Capture::new();
    let mut events = EventBuffer::with_capacity(4);
    events.push(&NoteOnEvent::new(0, target(0, 60, 1), 1.0));
    events.push(&NoteOnEvent::new(1, target(0, 61, 2), 0.0));
    events.push(&NoteOffEvent::new(2, target(0, 61, 2), 0.5));
    assert!(render_f64(&mut processor, &events.as_input(), &mut capture).0);
    assert_eq!(
        capture
            .events
            .iter()
            .map(|event| event.as_raw().note_id)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    capture.events.clear();
    events.clear();
    let tuple = Pckn::new(1_u16, 0_u16, 60_u16, Match::All);
    events.push(&NoteOnEvent::new(0, tuple, 1.0));
    events.push(&NoteOffEvent::new(1, tuple, 0.0));
    events.push(&NoteOnEvent::new(3, tuple, 1.0));
    assert!(render_f64(&mut processor, &events.as_input(), &mut capture).0);
    assert_eq!(capture.events, [] as [NoteEndEvent; 0]);
    events.clear();
    events.push(&NoteOffEvent::new(0, tuple, 0.0));
    assert!(render_f64(&mut processor, &events.as_input(), &mut capture).0);
    assert_eq!(capture.events.len(), 1);
    assert_eq!(capture.events[0].as_raw().note_id, -1);
    plugin.deactivate(processor.stop_processing());
}

#[test]
fn wildcard_voice_end_debt_survives_blocks_until_the_matching_retrigger_finishes() {
    let mut plugin = plugin();
    let mut processor = activate(&mut plugin);
    let mut events = EventBuffer::with_capacity(3);
    let wildcard_id = Pckn::new(0_u16, 0_u16, 60_u16, Match::All);
    events.push(&NoteOnEvent::new(0, wildcard_id, 1.0));
    events.push(&NoteOffEvent::new(1, wildcard_id, 0.0));
    events.push(&NoteOnEvent::new(3, target(0, 60, 42), 1.0));
    let mut capture = Capture::new();
    assert!(render_f64(&mut processor, &events.as_input(), &mut capture).0);
    assert_eq!(capture.events, [] as [NoteEndEvent; 0]);
    events.clear();
    assert!(render_f64(&mut processor, &events.as_input(), &mut capture).0);
    assert_eq!(capture.events, [] as [NoteEndEvent; 0]);
    events.push(&NoteOffEvent::new(0, target(0, 60, 42), 0.0));
    assert!(render_f64(&mut processor, &events.as_input(), &mut capture).0);
    assert!(
        capture.events.iter().any(|event| {
            let raw = event.as_raw();
            (raw.port_index, raw.channel, raw.key, raw.note_id) == (0, 0, 60, -1)
        }),
        "the original wildcard voice still needs completion feedback"
    );
    plugin.deactivate(processor.stop_processing());
}

struct RawNotes<'a>(&'a [clap_sys::events::clap_event_note]);
impl InputEventBuffer for RawNotes<'_> {
    fn len(&self) -> u32 {
        u32::try_from(self.0.len()).expect("small fixture batch")
    }
    fn get(&self, index: u32) -> Option<&UnknownEvent> {
        let event = self.0.get(usize::try_from(index).ok()?)?;
        // SAFETY: complete live note storage; tests only reduce reported size,
        // never widen it beyond the allocation. Downcasts must check that size.
        Some(unsafe { UnknownEvent::from_raw(std::ptr::from_ref(event).cast()) })
    }
}

#[test]
fn malformed_native_input_is_rejected_before_dsp_without_allocating() {
    let valid = *NoteOnEvent::new(0, target(0, 60, 1), 1.0).as_raw();
    let mut cases = [valid; 15];
    cases[0].port_index = -2;
    cases[1].port_index = 2;
    cases[2].channel = -1;
    cases[3].channel = 16;
    cases[4].key = 128;
    cases[5].note_id = -2;
    cases[6].velocity = f64::NAN;
    cases[7].velocity = 1.01;
    cases[8].header.time = 8;
    cases[9].header.type_ = clap_sys::events::CLAP_EVENT_NOTE_END;
    cases[10].header.size =
        u32::try_from(std::mem::size_of::<clap_sys::events::clap_event_header>()).unwrap();
    for raw in &mut cases[11..] {
        raw.header.type_ = clap_sys::events::CLAP_EVENT_NOTE_OFF;
    }
    cases[11].port_index = -2;
    cases[12].channel = -2;
    cases[13].key = -2;
    cases[14].note_id = -2;
    let before = (
        ALLOCATIONS.load(Ordering::Relaxed),
        DEALLOCATIONS.load(Ordering::Relaxed),
    );
    for raw in cases {
        let mut plugin = plugin();
        let mut processor = activate(&mut plugin);
        let batch = [valid, raw];
        let input = RawNotes(&batch);
        let mut capture = Capture::new();
        let (ok, samples) = render_f64(
            &mut processor,
            &InputEvents::from_buffer(&input),
            &mut capture,
        );
        assert!(!ok);
        assert_eq!(samples, [[-99.0; 8]; 2]);
        assert_eq!(capture.events, [] as [NoteEndEvent; 0]);
        plugin.deactivate(processor.stop_processing());
    }
    assert_eq!(
        (
            ALLOCATIONS.load(Ordering::Relaxed),
            DEALLOCATIONS.load(Ordering::Relaxed)
        ),
        before
    );
}

#[test]
fn native_domain_edges_ignore_unknown_spaces_and_unused_choke_velocity() {
    let mut plugin = plugin();
    let mut processor = activate(&mut plugin);
    let mut on = *NoteOnEvent::new(0, target(1, 127, i32::MAX.unsigned_abs()), 0.0).as_raw();
    on.channel = 15;
    let mut unknown = on;
    unknown.header.space_id = 42;
    unknown.port_index = -2; // No interpretation of an unclaimed event space.
    let mut choke = on;
    choke.header.type_ = clap_sys::events::CLAP_EVENT_NOTE_CHOKE;
    choke.header.time = 1;
    choke.port_index = -1;
    choke.channel = -1;
    choke.key = -1;
    choke.note_id = -1;
    choke.velocity = f64::NAN; // The native choke record does not consume this field.
    let batch = [unknown, on, choke];
    let input = RawNotes(&batch);
    let mut capture = Capture::new();
    let (ok, samples) = render_f64(
        &mut processor,
        &InputEvents::from_buffer(&input),
        &mut capture,
    );
    assert!(ok);
    assert_eq!(samples, [[0.0; 8]; 2]);
    assert_eq!(capture.events.len(), 1);
    let end = capture.events[0].as_raw();
    assert_eq!(
        (end.port_index, end.channel, end.key, end.note_id),
        (1, 15, 127, i32::MAX)
    );
    plugin.deactivate(processor.stop_processing());
}

#[test]
fn rejected_voice_feedback_fails_the_run_without_publishing_parameter_completion() {
    let mut plugin = plugin();
    let params = plugin
        .plugin_handle()
        .get_extension::<PluginParams>()
        .unwrap();
    let mut processor = activate(&mut plugin);
    let mut events = EventBuffer::with_capacity(3);
    set(&mut events, 0, 0.25);
    events.push(&NoteOnEvent::new(0, target(0, 60, 1), 1.0));
    events.push(&NoteOffEvent::new(1, Pckn::match_all(), 0.0));
    let mut capture = Capture::new();
    capture.reject = true;
    assert!(!render_f64(&mut processor, &events.as_input(), &mut capture).0);
    plugin.deactivate(processor.stop_processing());
    assert_eq!(
        params.get_value(&plugin.plugin_handle(), ClapId::new(7007)),
        Some(1.0)
    );
}
