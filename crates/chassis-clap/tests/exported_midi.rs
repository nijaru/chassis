#![allow(unsafe_code)]
//! Actual exported MIDI ABI, opaque payload ownership, ordered controls and bounds.

use clack_extensions::{
    audio_ports::PluginAudioPorts,
    note_ports::{NoteDialect, NoteDialects, NotePortInfoBuffer, PluginNotePorts},
    params::PluginParams,
};
use clack_host::{
    events::{
        UnknownEvent,
        event_types::{MidiEvent, MidiSysExEvent, NoteOnEvent, ParamValueEvent},
        io::{InputEventBuffer, OutputEventBuffer, TryPushError},
    },
    prelude::*,
};
use clap_sys::events::{
    CLAP_EVENT_DONT_RECORD, CLAP_EVENT_IS_LIVE, CLAP_EVENT_MIDI, CLAP_EVENT_MIDI_SYSEX,
    CLAP_EVENT_MIDI2, clap_event_header, clap_event_midi, clap_event_midi_sysex, clap_event_midi2,
    clap_event_note, clap_event_param_value,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    mem::size_of,
    ptr,
    sync::atomic::{AtomicU64, Ordering},
};

#[path = "../../../examples/clap-midi-probe/src/lib.rs"]
mod exported;

thread_local! { static COUNT: Cell<bool> = const { Cell::new(false) }; }
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static DEALLOCATIONS: AtomicU64 = AtomicU64::new(0);
struct Allocator;
// SAFETY: all allocator operations forward their original arguments to System.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNT.try_with(Cell::get).unwrap_or(false) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: the caller's allocator contract is forwarded unchanged.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if COUNT.try_with(Cell::get).unwrap_or(false) {
            DEALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: the original matching allocation and layout are forwarded.
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static GLOBAL: Allocator = Allocator;

// Deliberately no HostNotePortsImpl: the pinned upstream host vtable is defective.
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
    // SAFETY: static exported entry with the matching pinned CLAP ABI.
    let entry = unsafe { PluginEntry::load_from_raw(&exported::clap_entry, c"") }.unwrap();
    PluginInstance::new(
        |()| Host,
        |_| Host,
        &entry,
        c"com.nijaru.chassis.midi-probe",
        &HostInfo::new("midi qualification", "", "", "").unwrap(),
    )
    .unwrap()
}
fn activate(plugin: &mut PluginInstance<Host>) -> StartedPluginAudioProcessor<Host> {
    let mut processor = plugin
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
        .unwrap();
    // Warm the real callback before enabling allocation accounting. This also
    // prevents malformed-input tests from passing on a nonfunctional deployment.
    assert!(invoke(&mut processor, &Batch(&[]), &mut Capture::new()));
    processor
}
#[allow(
    clippy::float_cmp,
    reason = "Boolean wire values are exactly zero or one"
)]
fn assert_enabled(plugin: &mut PluginInstance<Host>, expected: bool) {
    let handle = plugin.plugin_handle();
    let value = handle
        .get_extension::<PluginParams>()
        .unwrap()
        .get_value(&handle, ClapId::new(7008))
        .unwrap();
    assert_eq!(value, f64::from(expected), "published enabled endpoint");
}

#[derive(Debug)]
enum Message {
    Short([u8; 3]),
    SysEx { start: usize, length: usize },
}
#[derive(Debug)]
struct Captured {
    size: u32,
    space: u16,
    kind: u16,
    time: u32,
    flags: u32,
    port: u16,
    message: Message,
}
struct Capture {
    records: Vec<Captured>,
    bytes: Vec<u8>,
    reject_after: Option<usize>,
}
impl Capture {
    fn new() -> Self {
        Self {
            records: Vec::with_capacity(1024),
            bytes: Vec::with_capacity(64 * 1024),
            reject_after: None,
        }
    }
    fn clear(&mut self) {
        self.records.clear();
        self.bytes.clear();
    }
    fn check(&self, index: usize, time: u32, flags: u32, port: u16, bytes: &[u8], sysex: bool) {
        let record = &self.records[index];
        assert_eq!(
            (record.time, record.flags, record.port),
            (time, flags, port)
        );
        assert_eq!(record.space, 0);
        if sysex {
            assert_eq!(record.kind, CLAP_EVENT_MIDI_SYSEX);
            assert_eq!(record.size, wire_size::<clap_event_midi_sysex>());
            let Message::SysEx { start, length } = record.message else {
                panic!("expected SysEx")
            };
            assert_eq!(length, bytes.len());
            assert_eq!(&self.bytes[start..start + length], bytes);
        } else {
            assert_eq!(record.kind, CLAP_EVENT_MIDI);
            assert_eq!(record.size, wire_size::<clap_event_midi>());
            let Message::Short(data) = record.message else {
                panic!("expected short MIDI")
            };
            assert_eq!(data, bytes);
        }
    }
}
impl OutputEventBuffer for Capture {
    fn try_push(&mut self, event: &UnknownEvent) -> Result<(), TryPushError> {
        if self.reject_after == Some(self.records.len())
            || self.records.len() == self.records.capacity()
        {
            return Err(TryPushError::new());
        }
        let (port, message) = if let Some(event) = event.as_event::<MidiEvent>() {
            (event.port_index(), Message::Short(event.data()))
        } else if let Some(event) = event.as_event::<MidiSysExEvent>() {
            let length = usize::try_from(event.buffer_size()).map_err(|_| TryPushError::new())?;
            if length > self.bytes.capacity() - self.bytes.len() {
                return Err(TryPushError::new());
            }
            let start = self.bytes.len();
            if length != 0 {
                // SAFETY: plugin output payloads remain readable throughout this
                // try_push only. Deep-copy now, never retain their pointers.
                self.bytes.extend_from_slice(unsafe { event.data() });
            }
            (event.port_index(), Message::SysEx { start, length })
        } else {
            return Err(TryPushError::new());
        };
        let header = event.header().as_raw();
        self.records.push(Captured {
            size: header.size,
            space: header.space_id,
            kind: header.type_,
            time: header.time,
            flags: header.flags,
            port,
            message,
        });
        Ok(())
    }
}

fn invoke(
    processor: &mut StartedPluginAudioProcessor<Host>,
    input: &impl InputEventBuffer,
    capture: &mut Capture,
) -> bool {
    // Pinned Clack derives clap_process.frames_count from these views. None/None
    // empty views would yield zero, not eight. With no ports, all channel extent
    // and aliasing obligations of from_raw_buffers are vacuously satisfied.
    // SAFETY: empty port collection contains no pointers or channel buffers.
    let audio_in = unsafe { InputAudioBuffers::from_raw_buffers(&[], 8) };
    let mut audio_out = OutputAudioBuffers::empty();
    let input = InputEvents::from_buffer(input);
    let mut output = OutputEvents::from_buffer(capture);
    processor
        .process(&audio_in, &mut audio_out, &input, &mut output, None, None)
        .is_ok()
}

fn process(
    processor: &mut StartedPluginAudioProcessor<Host>,
    input: &impl InputEventBuffer,
    capture: &mut Capture,
) -> bool {
    let before = (
        ALLOCATIONS.load(Ordering::Relaxed),
        DEALLOCATIONS.load(Ordering::Relaxed),
    );
    COUNT.set(true);
    let ok = invoke(processor, input, capture);
    COUNT.set(false);
    assert_eq!(
        (
            ALLOCATIONS.load(Ordering::Relaxed),
            DEALLOCATIONS.load(Ordering::Relaxed)
        ),
        before,
        "activated ABI process must neither allocate nor deallocate",
    );
    ok
}

fn wire_size<T>() -> u32 {
    u32::try_from(size_of::<T>()).unwrap()
}
fn header<T>(kind: u16, time: u32, flags: u32) -> clap_event_header {
    clap_event_header {
        size: wire_size::<T>(),
        time,
        space_id: 0,
        type_: kind,
        flags,
    }
}

// Raw records deliberately permit malformed boundary inputs. The list and all
// valid SysEx backing allocations stay immutable/live until process returns.
#[derive(Clone, Copy)]
enum Record {
    Short(clap_event_midi),
    SysEx(clap_event_midi_sysex),
    Parameter(clap_event_param_value),
    Note(clap_event_note),
    Midi2(clap_event_midi2),
}
struct Batch<'a>(&'a [Record]);
impl InputEventBuffer for Batch<'_> {
    fn len(&self) -> u32 {
        u32::try_from(self.0.len()).unwrap()
    }
    fn get(&self, index: u32) -> Option<&UnknownEvent> {
        let (pointer, available) = match self.0.get(usize::try_from(index).ok()?)? {
            Record::Short(raw) => (
                ptr::from_ref(raw).cast::<clap_event_header>(),
                size_of_val(raw),
            ),
            Record::SysEx(raw) => (ptr::from_ref(raw).cast(), size_of_val(raw)),
            Record::Parameter(raw) => (ptr::from_ref(raw).cast(), size_of_val(raw)),
            Record::Note(raw) => (ptr::from_ref(raw).cast(), size_of_val(raw)),
            Record::Midi2(raw) => (ptr::from_ref(raw).cast(), size_of_val(raw)),
        };
        // SAFETY: pointer is a live repr(C) event with a first-field header.
        let declared = unsafe { (*pointer).size };
        assert!(usize::try_from(declared).unwrap() <= available);
        assert!(declared >= wire_size::<clap_event_header>());
        // SAFETY: storage covers the complete declared record, including malformed
        // sizes/types. Checked adapter downcasts must reject those before reading
        // concrete fields. No event storage or valid payload changes during process.
        Some(unsafe { UnknownEvent::from_raw(pointer) })
    }
}
fn short(time: u32, port: u16, flags: u32, data: [u8; 3]) -> Record {
    Record::Short(clap_event_midi {
        header: header::<clap_event_midi>(CLAP_EVENT_MIDI, time, flags),
        port_index: port,
        data,
    })
}
fn sysex(time: u32, port: u16, flags: u32, bytes: &[u8]) -> Record {
    Record::SysEx(clap_event_midi_sysex {
        header: header::<clap_event_midi_sysex>(CLAP_EVENT_MIDI_SYSEX, time, flags),
        port_index: port,
        buffer: bytes.as_ptr(),
        size: u32::try_from(bytes.len()).unwrap(),
    })
}
fn set(time: u32, value: bool) -> Record {
    Record::Parameter(
        *ParamValueEvent::new(time, ClapId::new(7008), Pckn::match_all(), f64::from(value))
            .as_raw(),
    )
}

#[test]
fn exported_ports_are_midi_only_and_audio_topology_is_empty() {
    let mut plugin = plugin();
    let handle = plugin.plugin_handle();
    let descriptor = handle.descriptor().unwrap();
    assert_eq!(descriptor.name(), Some(c"Chassis MIDI Probe"));
    assert_eq!(descriptor.features().collect::<Vec<_>>(), [c"note-effect"]);
    let audio = handle.get_extension::<PluginAudioPorts>().unwrap();
    assert_eq!(audio.count(&handle, true), 0);
    assert_eq!(audio.count(&handle, false), 0);
    let ports = handle.get_extension::<PluginNotePorts>().unwrap();
    for (input, expected) in [
        (
            true,
            [
                (10, b"Secondary MIDI Input".as_slice()),
                (90, b"Primary MIDI Input".as_slice()),
            ],
        ),
        (
            false,
            [
                (20, b"Primary MIDI Output".as_slice()),
                (80, b"Secondary MIDI Output".as_slice()),
            ],
        ),
    ] {
        assert_eq!(ports.count(&handle, input), 2);
        for (index, (id, name)) in expected.into_iter().enumerate() {
            let mut buffer = NotePortInfoBuffer::new();
            let info = ports
                .get(&handle, u32::try_from(index).unwrap(), input, &mut buffer)
                .unwrap();
            assert_eq!(info.id, ClapId::new(id));
            assert_eq!(info.name, name);
            assert_eq!(info.supported_dialects, NoteDialects::MIDI);
            assert_eq!(info.preferred_dialect, Some(NoteDialect::Midi));
        }
        for index in [2, u32::MAX] {
            assert!(
                ports
                    .get(&handle, index, input, &mut NotePortInfoBuffer::new())
                    .is_none()
            );
        }
    }
}

#[test]
fn opaque_bytes_offsets_flags_and_independent_port_routes_survive_the_abi() {
    let mut plugin = plugin();
    let mut processor = activate(&mut plugin);
    let mut capture = Capture::new();
    let both = CLAP_EVENT_IS_LIVE | CLAP_EVENT_DONT_RECORD;
    // Includes zero-velocity on, system status/unused padding, vendor/data bytes,
    // high-bit data and no status. The router must not interpret MIDI grammar.
    let messages = [
        [0x90, 60, 0],
        [0xf8, 0xa5, 0xff],
        [0xc0, 7, 0x8f],
        [0x01, 0xff, 0x80],
        [0xff, 0, 0x55],
    ];
    let flags = [0, CLAP_EVENT_IS_LIVE, CLAP_EVENT_DONT_RECORD, both, 0];
    let mut payload = vec![0x23, 0xff, 0, 0xf7, 0x80]; // Deliberately unframed.
    let mut records: Vec<_> = messages
        .iter()
        .enumerate()
        .map(|(index, &data)| {
            short(
                u32::try_from(index).unwrap(),
                u16::try_from(index % 2).unwrap(),
                flags[index],
                data,
            )
        })
        .collect();
    records.push(sysex(5, 1, both, &payload));
    records.push(sysex(5, 0, 0, &payload[..2])); // Separate same-time payload boundary.
    records.push(sysex(6, 0, CLAP_EVENT_DONT_RECORD, &[]));
    // Explicitly legal empty SysEx with null storage (never form a null slice).
    let Record::SysEx(mut empty) = sysex(7, 1, CLAP_EVENT_IS_LIVE, &[]) else {
        unreachable!()
    };
    empty.buffer = ptr::null();
    records.push(Record::SysEx(empty));
    // Unclaimed spaces/types remain ignored, even with non-MIDI field values.
    let Record::Short(mut unknown) = short(7, u16::MAX, u32::MAX, [0xff; 3]) else {
        unreachable!()
    };
    unknown.header.space_id = 42;
    records.push(Record::Short(unknown));
    unknown.header.space_id = 0;
    unknown.header.type_ = u16::MAX;
    records.push(Record::Short(unknown));
    assert!(process(&mut processor, &Batch(&records), &mut capture));
    assert_eq!(capture.records.len(), 9);
    // Prove host ownership after both source mutation and destruction.
    payload.fill(0x44);
    drop(payload);
    drop(records);
    for (index, data) in messages.iter().enumerate() {
        capture.check(
            index,
            u32::try_from(index).unwrap(),
            flags[index],
            u16::try_from(1 - index % 2).unwrap(),
            data,
            false,
        );
    }
    capture.check(5, 5, both, 0, &[0x23, 0xff, 0, 0xf7, 0x80], true);
    capture.check(6, 5, 0, 1, &[0x23, 0xff], true);
    capture.check(7, 6, CLAP_EVENT_DONT_RECORD, 1, &[], true);
    capture.check(8, 7, CLAP_EVENT_IS_LIVE, 0, &[], true);
    plugin.deactivate(processor.stop_processing());
}

#[test]
fn equal_time_controls_follow_list_order_and_publish_the_next_blocks_baseline() {
    let mut plugin = plugin();
    let mut processor = activate(&mut plugin);
    let mut capture = Capture::new();
    let records = [
        set(0, false),
        short(0, 1, 0, [1, 2, 3]),
        set(0, true),
        short(0, 0, 0, [4, 5, 6]),
        set(0, false),
        short(0, 1, 0, [7, 8, 9]),
    ];
    assert!(process(&mut processor, &Batch(&records), &mut capture));
    assert_eq!(capture.records.len(), 1);
    capture.check(0, 0, 0, 1, &[4, 5, 6], false);
    assert_enabled(&mut plugin, false);
    capture.clear();
    assert!(process(
        &mut processor,
        &Batch(&[short(7, 0, 0, [9, 9, 9])]),
        &mut capture
    ));
    assert!(capture.records.is_empty());
    assert!(process(
        &mut processor,
        &Batch(&[set(0, true)]),
        &mut capture
    ));
    assert_enabled(&mut plugin, true);
    processor.reset(); // No history, and reset must not change durable enabled.
    assert!(process(
        &mut processor,
        &Batch(&[short(7, 1, 0, [9, 8, 7])]),
        &mut capture
    ));
    assert_eq!(capture.records.len(), 1);
    capture.check(0, 7, 0, 0, &[9, 8, 7], false);
    plugin.deactivate(processor.stop_processing());
}

fn rejected_before_dsp(records: &[Record]) {
    let mut plugin = plugin();
    let mut processor = activate(&mut plugin);
    let mut capture = Capture::new();
    assert!(!process(&mut processor, &Batch(records), &mut capture));
    assert!(
        capture.records.is_empty(),
        "invalid batch must not call the router"
    );
    plugin.deactivate(processor.stop_processing());
    assert_enabled(&mut plugin, true);
}

#[test]
fn malformed_record_after_valid_input_rejects_the_whole_batch_before_dsp() {
    let valid = short(0, 1, 0, [0x90, 60, 0]);
    let Record::Short(base) = valid else {
        unreachable!()
    };
    let Record::SysEx(sx) = sysex(0, 0, 0, &[]) else {
        unreachable!()
    };
    let mut small = base;
    small.header.size -= 1;
    // Larger-than-short declared size has actual SysEx-sized backing storage.
    let mut large = sx;
    large.header.type_ = CLAP_EVENT_MIDI;
    let mut bad_port = base;
    bad_port.port_index = 2; // Host indices are direction-local, not schema indices.
    let mut late = base;
    late.header.time = 8;
    let mut flags = base;
    flags.header.flags = 4;
    let mut sysex_flags = sx;
    sysex_flags.header.flags = 0x8000_0000;
    let mut null = sx;
    null.size = 1;
    null.buffer = ptr::null();
    let mut sysex_small = sx;
    sysex_small.header.size -= 1;
    let mut sysex_port = sx;
    sysex_port.port_index = u16::MAX;
    let native = *NoteOnEvent::new(0, Pckn::new(0_u16, 0_u16, 60_u16, 1_u32), 1.0).as_raw();
    let midi2 = clap_event_midi2 {
        header: header::<clap_event_midi2>(CLAP_EVENT_MIDI2, 0, 0),
        port_index: 0,
        data: [0; 4],
    };
    for bad in [
        Record::Short(small),
        Record::SysEx(large),
        Record::Short(bad_port),
        Record::Short(late),
        Record::Short(flags),
        Record::SysEx(sysex_flags),
        Record::SysEx(null),
        Record::SysEx(sysex_small),
        Record::SysEx(sysex_port),
        Record::Note(native),
        Record::Midi2(midi2),
    ] {
        // A valid MIDI event before the bad record makes pre-DSP rejection
        // observable; the final control would change the endpoint on success.
        rejected_before_dsp(&[valid, set(0, false), bad]);
    }
    rejected_before_dsp(&[short(3, 0, 0, [1, 2, 3]), set(2, false)]);
}

#[test]
fn host_rejection_is_fatal_without_rolling_back_prior_outputs_or_publishing_controls() {
    for accepted in [0, 1] {
        let mut plugin = plugin();
        let mut processor = activate(&mut plugin);
        let mut capture = Capture::new();
        capture.reject_after = Some(accepted);
        let mut payload = vec![0xf0, 0xff, 3, 0xf7];
        let records = [
            sysex(0, 1, CLAP_EVENT_IS_LIVE, &payload),
            short(1, 0, 0, [0x90, 60, 0]),
            set(2, false),
        ];
        assert!(!process(&mut processor, &Batch(&records), &mut capture));
        payload.fill(0);
        drop(payload);
        assert_eq!(capture.records.len(), accepted);
        if accepted == 1 {
            capture.check(0, 0, CLAP_EVENT_IS_LIVE, 0, &[0xf0, 0xff, 3, 0xf7], true);
        }
        plugin.deactivate(processor.stop_processing());
        assert_enabled(&mut plugin, true);
    }
}

#[test]
#[cfg_attr(
    miri,
    ignore = "maximum-size ABI stress; focused malformed/ownership tests cover Miri"
)]
fn accepted_maximum_and_independent_overflow_bounds_are_allocation_free() {
    let payload = vec![0xff; 64 * 1024];
    let mut records: Vec<_> = payload
        .as_chunks::<64>()
        .0
        .iter()
        .enumerate()
        .map(|(index, bytes)| {
            sysex(
                7,
                u16::try_from(index % 2).unwrap(),
                CLAP_EVENT_IS_LIVE | CLAP_EVENT_DONT_RECORD,
                bytes,
            )
        })
        .collect();
    // Simultaneously fill every accepted input family and the raw-list budget.
    // Controls follow the MIDI at the same offset, so the final disable must
    // publish without retroactively suppressing any of the 1024 outputs.
    records.extend(std::iter::repeat_n(set(7, true), 511));
    records.push(set(7, false));
    let Record::Short(mut ignored) = short(7, u16::MAX, u32::MAX, [0xff; 3]) else {
        unreachable!()
    };
    ignored.header.space_id = 42;
    records.extend(std::iter::repeat_n(Record::Short(ignored), 512));
    assert_eq!(records.len(), 2048);
    let mut plugin = plugin();
    let mut processor = activate(&mut plugin);
    let mut capture = Capture::new();
    assert!(process(&mut processor, &Batch(&records), &mut capture));
    assert_eq!(capture.records.len(), 1024);
    assert_eq!(capture.bytes.len(), 64 * 1024);
    for index in 0..1024 {
        capture.check(
            index,
            7,
            CLAP_EVENT_IS_LIVE | CLAP_EVENT_DONT_RECORD,
            u16::try_from(1 - index % 2).unwrap(),
            &[0xff; 64],
            true,
        );
    }
    plugin.deactivate(processor.stop_processing());

    assert_enabled(&mut plugin, false);

    // Count overflow independent of byte budget, with a state-changing control.
    let mut too_many = vec![short(0, 0, 0, [0, 0, 0]); 1025];
    too_many.push(set(0, false));
    rejected_before_dsp(&too_many);
    // Parameter budget independently of raw and MIDI limits.
    let mut too_many_parameters = vec![set(0, false); 513];
    too_many_parameters.insert(0, short(0, 0, 0, [0, 0, 0]));
    rejected_before_dsp(&too_many_parameters);
    // Raw budget includes ignored records, without exceeding a family budget.
    let Record::Short(mut unknown) = short(0, 0, 0, [0, 0, 0]) else {
        unreachable!()
    };
    unknown.header.space_id = 42;
    let mut raw_overflow = vec![Record::Short(unknown); 2049];
    raw_overflow[0] = short(0, 0, 0, [1, 2, 3]);
    raw_overflow[1] = set(0, false);
    rejected_before_dsp(&raw_overflow);

    // Invalid nonnull payload pointers must be rejected from scalar sizes BEFORE
    // any payload borrow. First exceeds the entire budget, second the remaining
    // aggregate, and u32::MAX also exercises checked overflow-sized admission.
    let Record::SysEx(mut over) = sysex(0, 0, 0, &[]) else {
        unreachable!()
    };
    over.buffer = ptr::dangling();
    for size in [64 * 1024 + 1, u32::MAX] {
        over.size = size;
        rejected_before_dsp(&[
            short(0, 0, 0, [1, 2, 3]),
            set(0, false),
            Record::SysEx(over),
        ]);
    }
    over.size = 1;
    rejected_before_dsp(&[sysex(0, 1, 0, &payload), set(0, false), Record::SysEx(over)]);
}
