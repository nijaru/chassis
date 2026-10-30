use super::*;
use crate::{
    event_ports::ClapNotePort, input_events::InputScratch, output_events::ClapEventOutput,
    parameters::ClapStateProjection,
};
use chassis_runtime::{
    automation::ParameterEvents,
    events::{
        EventPortDescriptor, EventPortDirection, EventPortKey, EventSink, NoteEvents, OutputEvent,
    },
    midi::{MidiEventLimits, MidiEvents},
    parameters::ParameterDescriptor,
    process::{InputEventIndex, InputEventRef, ProcessContext, ProcessMode, TransportSnapshot},
};
use clack_plugin::{
    events::{
        Pckn,
        event_types::{NoteOnEvent, ParamValueEvent},
        io::EventBuffer,
    },
    utils::ClapId,
};

fn ports() -> ClapEventPorts {
    let native = EventPortKey::new("native");
    let midi_in = EventPortKey::new("midi.in");
    let midi_out = EventPortKey::new("midi.out");
    ClapEventPorts::new(
        &[
            EventPortDescriptor::new(
                native.clone(),
                "Native",
                EventPortDirection::Input,
                &[EventDialect::Notes],
            ),
            EventPortDescriptor::new(
                midi_out.clone(),
                "MIDI Output",
                EventPortDirection::Output,
                &[EventDialect::Midi1],
            ),
            EventPortDescriptor::new(
                midi_in.clone(),
                "Mixed Input",
                EventPortDirection::Input,
                &[EventDialect::Notes, EventDialect::Midi1],
            ),
        ],
        &[
            ClapNotePort::new(native, 90, EventDialect::Notes),
            ClapNotePort::new(midi_out, 30, EventDialect::Midi1),
            ClapNotePort::new(midi_in, 10, EventDialect::Midi1),
        ],
    )
    .unwrap()
}

#[test]
fn mixed_admission_preserves_three_family_order_and_borrowed_payload_when_controls_are_withheld() {
    let ports = ports();
    let parameters = ClapStateProjection::new(
        &[ParameterDescriptor::float("level", "Level", 0.0, 1.0, 1.0).unwrap()],
        &[("level", 7)],
    )
    .unwrap();
    let bytes = [0xff, 0, 0x81]; // deliberately not framed or restricted to MIDI data bytes
    // SAFETY: bytes outlive both the input list and every borrowed process view.
    let sysex = unsafe { MidiSysExEvent::new(1, 0, &bytes) }.with_flags(ClapFlags::DONT_RECORD);
    let mut raw = EventBuffer::with_capacity(4);
    raw.push(&ParamValueEvent::new(
        0,
        ClapId::new(7),
        Pckn::match_all(),
        0.5,
    ));
    raw.push(&NoteOnEvent::new(
        0,
        Pckn::new(0_u16, 0_u16, 60_u16, 4_u32),
        0.75,
    ));
    raw.push(&ClapMidi::new(0, 0, [0x90, 60, 0]).with_flags(ClapFlags::IS_LIVE));
    raw.push(&sysex);
    let input = InputEvents::from_buffer(&raw);
    let mut scratch = InputScratch::new(1, 1, 4, MidiEventLimits::new(2, 3)).unwrap();
    scratch.normalize(&parameters, &ports, &input, 8).unwrap();
    assert_eq!(
        scratch.order,
        [
            InputEventIndex::Parameter(0),
            InputEventIndex::Note(0),
            InputEventIndex::Midi(0),
            InputEventIndex::Midi(1)
        ]
    );
    let source = ClapMidiInput {
        input: &input,
        records: &scratch.midi,
    };
    let midi = MidiEvents::new(&source, 8, 2, 3).unwrap();
    let events = midi.iter().collect::<Vec<_>>();
    assert_eq!(events[0].port(), EventPortIndex::new(2));
    assert_eq!(events[0].message(), MidiMessage::Short([0x90, 60, 0]));
    assert!(events[0].flags().is_live());
    assert!(events[1].flags().do_not_record());
    let MidiMessage::SysEx(loan) = events[1].message() else {
        panic!("expected SysEx")
    };
    assert_eq!(loan, bytes);
    assert_eq!(loan.as_ptr(), bytes.as_ptr());
    let context = ProcessContext::with_events(
        ProcessMode::Realtime,
        TransportSnapshot::unknown(),
        ParameterEvents::new(&scratch.parameters, 8, 1).unwrap(),
        NoteEvents::new(&scratch.notes, 8, 1).unwrap(),
        midi,
    )
    .with_input_order(&scratch.order)
    .unwrap();
    let families = |context: ProcessContext<'_>| {
        context
            .ordered_events()
            .unwrap()
            .map(|event| match event {
                InputEventRef::Parameter(_) => 0,
                InputEventRef::Note(_) => 1,
                InputEventRef::Midi(_) => 2,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(families(context), [0, 1, 2, 2]);
    assert_eq!(families(context.without_parameter_events()), [1, 2, 2]);
}

#[test]
fn generic_output_bridge_preserves_midi_layout_fields_and_refuses_input_ports() {
    let ports = ports();
    let mut capture = EventBuffer::with_capacity(2);
    let mut output = OutputEvents::from_buffer(&mut capture);
    let mut sink = ClapEventOutput {
        configuration: &ports,
        output: &mut output,
    };
    let event = MidiEvent::new(
        3,
        EventPortIndex::new(1),
        MidiMessage::Short([0xf1, 0xff, 0x80]),
    )
    .with_flags(EventFlags::new(true, true));
    sink.try_push(OutputEvent::Midi(event)).unwrap();
    assert_eq!(
        sink.try_push(OutputEvent::Midi(event.with_port(EventPortIndex::new(2)))),
        Err(EventOutputError::Rejected)
    );
    let raw = capture
        .get(0)
        .unwrap()
        .as_event::<ClapMidi>()
        .unwrap()
        .as_raw();
    assert_eq!(raw.header.time, 3);
    assert_eq!(
        raw.header.flags,
        (ClapFlags::IS_LIVE | ClapFlags::DONT_RECORD).bits()
    );
    assert_eq!(raw.port_index, 0);
    assert_eq!(raw.data, [0xf1, 0xff, 0x80]);
}
