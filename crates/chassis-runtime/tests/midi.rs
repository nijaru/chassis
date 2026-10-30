//! Opaque MIDI admission and source/lifetime contracts, independent of adapters.

use chassis_runtime::{
    events::{
        EventDialect, EventFlags, EventPortDescriptor, EventPortDirection, EventPortIndex,
        EventPortKey,
    },
    midi::{
        MidiEvent, MidiEventLimits, MidiEventPortError, MidiEventSource, MidiEvents,
        MidiEventsError, MidiMessage,
    },
};

const PORT: EventPortIndex = EventPortIndex::new(0);

#[test]
fn opaque_fixed_bytes_empty_and_unframed_sysex_and_portable_flags_are_preserved() {
    let payload = [0x01, 0xff, 0x80]; // Deliberately not F0..F7 framed.
    for live in [false, true] {
        for do_not_record in [false, true] {
            let flags = EventFlags::new(live, do_not_record);
            assert_eq!(flags.is_live(), live);
            assert_eq!(flags.do_not_record(), do_not_record);
            let events = [
                MidiEvent::new(0, PORT, MidiMessage::Short([0x90, 60, 0])).with_flags(flags),
                MidiEvent::new(0, PORT, MidiMessage::Short([0x01, 0xff, 0xab])).with_flags(flags),
                MidiEvent::new(1, PORT, MidiMessage::SysEx(&[])).with_flags(flags),
                MidiEvent::new(1, PORT, MidiMessage::SysEx(&payload)).with_flags(flags),
            ];
            let slice = events.as_slice();
            let view = MidiEvents::new(&slice, 2, 4, 3).unwrap();
            assert_eq!(view.len(), 4);
            assert_eq!(view.frame_count(), 2);
            assert_eq!(view.sysex_bytes(), 3);
            assert_eq!(view.iter().collect::<Vec<_>>(), events);
            // Retargeting is metadata-only; payload address is untouched.
            let retargeted = view
                .iter()
                .last()
                .unwrap()
                .with_port(EventPortIndex::new(7));
            assert_eq!(retargeted.port(), EventPortIndex::new(7));
            assert!(matches!(retargeted.message(), MidiMessage::SysEx(bytes)
                if core::ptr::eq(bytes.as_ptr(), payload.as_ptr())));
        }
    }
    assert_eq!(EventFlags::default(), EventFlags::new(false, false));
    assert_eq!(MidiEventLimits::default(), MidiEventLimits::new(0, 0));
    let limits = MidiEventLimits::new(4, 12);
    assert_eq!(limits.max_events(), 4);
    assert_eq!(limits.max_sysex_bytes(), 12);
    let empty = MidiEvents::empty_for_block(0);
    assert!(empty.is_empty());
    assert_eq!(empty.frame_count(), 0);
    assert_eq!(empty.sysex_bytes(), 0);
    assert_eq!(empty.iter().next(), None);
}

struct CountSource<'a> {
    events: &'a [MidiEvent<'a>],
    count: usize,
}
impl MidiEventSource for CountSource<'_> {
    fn len(&self) -> usize {
        self.count
    }
    fn get(&self, index: usize) -> Option<MidiEvent<'_>> {
        self.events.get(index).copied()
    }
}

#[test]
fn admission_rejects_counts_source_holes_times_and_aggregate_bytes() {
    let events = [
        MidiEvent::new(0, PORT, MidiMessage::SysEx(&[1, 2])),
        MidiEvent::new(1, PORT, MidiMessage::SysEx(&[3, 4])),
    ];
    assert_eq!(
        MidiEvents::new(&events, 2, 1, 4).unwrap_err(),
        MidiEventsError::TooManyEvents {
            actual: 2,
            maximum: 1
        }
    );
    assert_eq!(
        MidiEvents::new(&events, 2, 2, 3).unwrap_err(),
        MidiEventsError::TooManySysExBytes {
            actual: 4,
            maximum: 3
        }
    );
    for count in [1, 3] {
        assert_eq!(
            MidiEvents::new(
                &CountSource {
                    events: &events,
                    count
                },
                2,
                3,
                4
            )
            .unwrap_err(),
            MidiEventsError::SourceCountMismatch
        );
    }
    assert_eq!(
        MidiEvents::new(&events, 1, 2, 4).unwrap_err(),
        MidiEventsError::OffsetOutOfRange {
            index: 1,
            offset: 1,
            frame_count: 1
        }
    );
    let decreasing = [events[1], events[0]];
    assert_eq!(
        MidiEvents::new(&decreasing, 2, 2, 4).unwrap_err(),
        MidiEventsError::NonMonotonicOffsets {
            index: 1,
            previous: 1,
            actual: 0
        }
    );
    assert!(MidiEvents::new(&[MidiEvent::new(0, PORT, MidiMessage::SysEx(&[]))], 1, 1, 0).is_ok());
    assert!(MidiEvents::new(&[events[0]], 0, 1, 4).is_err());
}

#[test]
fn aggregate_length_overflow_is_checked_without_copying_large_payloads() {
    struct Repeated<'a>(&'a [u8]);
    impl MidiEventSource for Repeated<'_> {
        fn len(&self) -> usize {
            4096
        }
        fn get(&self, index: usize) -> Option<MidiEvent<'_>> {
            (index < self.len()).then_some(MidiEvent::new(0, PORT, MidiMessage::SysEx(self.0)))
        }
    }
    // 4096 borrowed views of ONE MiB sum to u32::MAX + 1; no 4 GiB fixture.
    let bytes = vec![0; 1024 * 1024];
    assert_eq!(
        MidiEvents::new(&Repeated(&bytes), 1, 4096, u32::MAX).unwrap_err(),
        MidiEventsError::SysExBytesOverflow
    );
}

#[test]
fn input_requires_a_concrete_known_input_port_with_the_messages_dialect() {
    for message in [
        MidiMessage::Short([0; 3]),
        MidiMessage::SysEx(&[]),
        MidiMessage::Ump([u32::MAX, 0, 1, 0x1234_abcd]),
    ] {
        let events = [MidiEvent::new(0, PORT, message)];
        let view = MidiEvents::new(&events, 1, 1, 0).unwrap();
        assert_eq!(
            view.validate_ports(&[]),
            Err(MidiEventPortError::UnknownPort(PORT))
        );
        for direction in [EventPortDirection::Input, EventPortDirection::Output] {
            for dialect in [
                EventDialect::Notes,
                EventDialect::Midi1,
                EventDialect::Midi2,
            ] {
                let ports = [EventPortDescriptor::owned(
                    "midi",
                    "MIDI",
                    direction,
                    vec![dialect],
                )];
                let expected = if direction != EventPortDirection::Input {
                    Err(MidiEventPortError::PortIsNotInput(PORT))
                } else if dialect != message.dialect() {
                    Err(MidiEventPortError::PortDoesNotSupportDialect {
                        port: PORT,
                        dialect: message.dialect(),
                    })
                } else {
                    Ok(())
                };
                assert_eq!(view.validate_ports(&ports), expected);
            }
        }
        let ports = [EventPortDescriptor::new(
            EventPortKey::new("in"),
            "Input",
            EventPortDirection::Input,
            &[EventDialect::Midi1, EventDialect::Midi2],
        )];
        assert_eq!(view.validate_ports(&ports), Ok(()));
    }
}

#[test]
fn ump_preserves_all_four_words_and_shares_counts_without_spending_sysex_bytes() {
    let words = [0x2090_3cff, 0x89ab_cdef, u32::MAX, 0x1234_5678];
    // Even unused words of a one-word message remain opaque storage.
    let event =
        MidiEvent::new(0, PORT, MidiMessage::Ump(words)).with_flags(EventFlags::new(true, true));
    let events = [
        event,
        MidiEvent::new(0, PORT, MidiMessage::Short([0xff; 3])),
    ];
    let view = MidiEvents::new(&events, 1, 2, 0).unwrap();
    assert_eq!(view.sysex_bytes(), 0);
    assert_eq!(view.iter().next(), Some(event));
    assert_eq!(
        event.with_port(EventPortIndex::new(9)).message(),
        MidiMessage::Ump(words)
    );
    assert_eq!(event.message().dialect(), EventDialect::Midi2);
    assert_eq!(
        MidiEvents::new(&events, 1, 1, 0).unwrap_err(),
        MidiEventsError::TooManyEvents {
            actual: 2,
            maximum: 1
        }
    );
}
