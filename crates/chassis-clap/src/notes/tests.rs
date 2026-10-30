use super::*;
use crate::{input_events::InputScratch, parameters::ClapStateProjection};
use clack_plugin::events::{
    event_types::NoteExpressionType,
    io::{EventBuffer, InputEvents},
};

const INPUT: EventPortKey = EventPortKey::new("in");
const OUTPUT: EventPortKey = EventPortKey::new("out");

fn descriptors() -> Vec<EventPortDescriptor> {
    vec![
        EventPortDescriptor::new(
            OUTPUT,
            "Output",
            EventPortDirection::Output,
            &[EventDialect::Notes],
        ),
        EventPortDescriptor::new(
            INPUT,
            "Input",
            EventPortDirection::Input,
            &[EventDialect::Notes],
        ),
    ]
}
fn mapping() -> [ClapNotePort; 2] {
    [ClapNotePort::new(INPUT, 77), ClapNotePort::new(OUTPUT, 77)]
}

#[test]
fn mapping_rejects_incomplete_ambiguous_and_lossy_port_metadata() {
    let schema = descriptors();
    assert!(ClapNoteConfiguration::new(&schema, &mapping()).is_ok()); // IDs are direction-local.
    assert!(ClapNoteConfiguration::new(&schema, &mapping()[..1]).is_err());
    assert!(
        ClapNoteConfiguration::new(
            &schema,
            &[ClapNotePort::new(INPUT, 1), ClapNotePort::new(INPUT, 2)]
        )
        .is_err()
    );
    assert!(
        ClapNoteConfiguration::new(
            &schema,
            &[
                ClapNotePort::new(INPUT, u32::MAX),
                ClapNotePort::new(OUTPUT, 1)
            ]
        )
        .is_err()
    );
    let mut bad = schema.clone();
    bad[0].direction = EventPortDirection::Input;
    assert!(ClapNoteConfiguration::new(&bad, &mapping()).is_err());
    for dialects in [
        vec![EventDialect::Midi1],
        vec![EventDialect::Notes, EventDialect::Midi2],
    ] {
        let mut bad = schema.clone();
        bad[0].dialects = dialects.into();
        assert!(ClapNoteConfiguration::new(&bad, &mapping()).is_err());
    }
    for name in [String::from("hidden\0suffix"), "é".repeat(128)] {
        let mut bad = schema.clone();
        bad[0].name = name.into();
        assert!(ClapNoteConfiguration::new(&bad, &mapping()).is_err());
    }
}

#[test]
fn note_budget_and_total_raw_budget_are_independent() {
    let config = ClapNoteConfiguration::new(&descriptors(), &mapping()).unwrap();
    let parameters = ClapStateProjection::new(&[], &[]).unwrap();
    let events = [NoteOnEvent::new(0, Pckn::new(0_u16, 0_u16, 60_u16, 1_u32), 0.5); 2];
    let input = InputEvents::from_buffer(&events);
    let mut scratch = InputScratch::new(0, 1, 3).unwrap();
    assert_eq!(
        scratch.normalize(&parameters, &config, &input, 4),
        Err("CLAP note event count exceeds the activation bound")
    );
    let mut scratch = InputScratch::new(0, 3, 1).unwrap();
    assert_eq!(
        scratch.normalize(&parameters, &config, &input, 4),
        Err("CLAP input event count exceeds the activation bound")
    );
    assert_eq!(scratch.notes, [] as [NoteEvent; 0]);
}

const EXPRESSIONS: [(NoteExpressionKind, NoteExpressionType, f64); 7] = [
    (NoteExpressionKind::Volume, NoteExpressionType::Volume, 2.0),
    (NoteExpressionKind::Pan, NoteExpressionType::Pan, 0.75),
    (
        NoteExpressionKind::Tuning,
        NoteExpressionType::Tuning,
        -120.0,
    ),
    (
        NoteExpressionKind::Vibrato,
        NoteExpressionType::Vibrato,
        0.25,
    ),
    (
        NoteExpressionKind::Expression,
        NoteExpressionType::Expression,
        0.5,
    ),
    (
        NoteExpressionKind::Brightness,
        NoteExpressionType::Brightness,
        1.0,
    ),
    (
        NoteExpressionKind::Pressure,
        NoteExpressionType::Pressure,
        0.0,
    ),
];

#[test]
fn native_egress_preserves_each_expression_and_end_uses_the_input_mapping() {
    let config = ClapNoteConfiguration::new(&descriptors(), &mapping()).unwrap();
    let output_address = NoteAddress::new(EventPortIndex::new(0), None, None, None);
    let input_address = NoteAddress::new(
        EventPortIndex::new(1),
        Some(NoteId::new(i32::MAX.unsigned_abs())),
        NoteChannel::new(15),
        NoteKey::new(127),
    );
    let mut captured = EventBuffer::with_capacity(16);
    {
        let mut output = OutputEvents::from_buffer(&mut captured);
        let mut sink = ClapNoteOutput {
            configuration: &config,
            output: &mut output,
        };
        for (kind, _, value) in EXPRESSIONS {
            sink.try_push(NoteEvent::new(
                0,
                NoteEventKind::Expression {
                    address: output_address,
                    expression: NoteExpression::new(kind, value).unwrap(),
                },
            ))
            .unwrap();
        }
        sink.try_push(NoteEvent::new(
            1,
            NoteEventKind::End {
                address: input_address,
            },
        ))
        .unwrap();
        let concrete_output = NoteAddress::new(
            EventPortIndex::new(0),
            None,
            NoteChannel::new(0),
            NoteKey::new(60),
        );
        for kind in [
            NoteEventKind::On {
                address: concrete_output,
                velocity: NormalizedValue::new(0.0).unwrap(),
            },
            NoteEventKind::Off {
                address: output_address,
                velocity: NormalizedValue::new(0.75).unwrap(),
            },
            NoteEventKind::Choke {
                address: output_address,
            },
        ] {
            sink.try_push(NoteEvent::new(2, kind)).unwrap();
        }
        let too_large = NoteAddress::new(
            EventPortIndex::new(1),
            Some(NoteId::new(u32::MAX)),
            NoteChannel::new(0),
            NoteKey::new(60),
        );
        assert_eq!(
            sink.try_push(NoteEvent::new(1, NoteEventKind::End { address: too_large })),
            Err(NoteOutputError::Rejected)
        );
    }
    for (index, (_, expected, value)) in EXPRESSIONS.into_iter().enumerate() {
        let event = captured
            .get(u32::try_from(index).unwrap())
            .unwrap()
            .as_event::<NoteExpressionEvent>()
            .unwrap();
        assert_eq!(event.expression_type(), Some(expected));
        assert_eq!(event.value().to_bits(), value.to_bits());
        assert_eq!(event.as_raw().port_index, 0);
    }
    let end = captured.get(7).unwrap().as_event::<NoteEndEvent>().unwrap();
    assert_eq!(end.as_raw().port_index, 0);
    assert_eq!(end.as_raw().note_id, i32::MAX);
    assert_eq!(end.as_raw().channel, 15);
    assert_eq!(end.as_raw().key, 127);
    let on = captured.get(8).unwrap().as_event::<NoteOnEvent>().unwrap();
    assert_eq!(on.as_raw().port_index, 0);
    assert_eq!(on.as_raw().key, 60);
    assert_eq!(on.velocity().to_bits(), 0.0_f64.to_bits());
    let off = captured.get(9).unwrap().as_event::<NoteOffEvent>().unwrap();
    assert_eq!(off.as_raw().key, -1);
    assert_eq!(off.velocity().to_bits(), 0.75_f64.to_bits());
    assert_eq!(
        captured
            .get(10)
            .unwrap()
            .as_event::<NoteChokeEvent>()
            .unwrap()
            .as_raw()
            .port_index,
        0
    );
}
