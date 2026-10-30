//! Opaque MIDI projection. Host payload loans never enter persistent scratch.

#[cfg(test)]
mod tests;

use chassis_runtime::{
    events::{EventDialect, EventFlags, EventOutputError, EventPortIndex},
    midi::{MidiEvent, MidiEventSource, MidiMessage},
};
use clack_plugin::events::{
    Event, EventFlags as ClapFlags, UnknownEvent,
    event_types::{Midi2Event, MidiEvent as ClapMidi, MidiSysExEvent},
    io::{InputEvents, OutputEvents},
};

use crate::event_ports::ClapEventPorts;

#[derive(Clone, Copy)]
enum MessageRecord {
    Short([u8; 3]),
    Ump([u32; 4]),
    SysEx { input_index: u32, size: u32 },
}

/// No borrowed payloads or host pointers survive the delivering callback.
#[derive(Clone, Copy)]
pub(crate) struct MidiInputRecord {
    offset: u32,
    port: EventPortIndex,
    flags: EventFlags,
    message: MessageRecord,
}

fn payload_size(event: &MidiSysExEvent, maximum: u32) -> Result<u32, &'static str> {
    let raw = event.as_raw();
    let size = usize::try_from(raw.size).map_err(|_| "SysEx size is not representable")?;
    if raw.size > maximum || size > isize::MAX as usize {
        return Err("CLAP SysEx byte count exceeds the activation bound");
    }
    if size != 0 && raw.buffer.is_null() {
        return Err("CLAP SysEx payload is null");
    }
    Ok(raw.size)
}

pub(crate) fn normalize(
    ports: &ClapEventPorts,
    event: &UnknownEvent,
    input_index: u32,
    remaining_bytes: u32,
) -> Result<Option<(MidiInputRecord, u32)>, &'static str> {
    let header = event.header().as_raw();
    if header.space_id != 0
        || !matches!(
            header.type_,
            ClapMidi::TYPE_ID | MidiSysExEvent::TYPE_ID | Midi2Event::TYPE_ID
        )
    {
        return Ok(None);
    }
    let known = ClapFlags::IS_LIVE | ClapFlags::DONT_RECORD;
    if header.flags & !known.bits() != 0 {
        return Err("Unsupported CLAP MIDI event flags");
    }
    let flags = EventFlags::new(
        header.flags & ClapFlags::IS_LIVE.bits() != 0,
        header.flags & ClapFlags::DONT_RECORD.bits() != 0,
    );
    let (port, message, bytes, dialect) = if header.type_ == ClapMidi::TYPE_ID {
        let raw = event
            .as_event::<ClapMidi>()
            .ok_or("Malformed CLAP MIDI event")?
            .as_raw();
        (
            raw.port_index,
            MessageRecord::Short(raw.data),
            0,
            EventDialect::Midi1,
        )
    } else if header.type_ == Midi2Event::TYPE_ID {
        let event = event
            .as_event::<Midi2Event>()
            .ok_or("Malformed CLAP MIDI 2 event")?;
        (
            event.port_index(),
            MessageRecord::Ump(event.data()),
            0,
            EventDialect::Ump,
        )
    } else {
        let event = event
            .as_event::<MidiSysExEvent>()
            .ok_or("Malformed CLAP SysEx event")?;
        let size = payload_size(event, remaining_bytes)?;
        (
            event.as_raw().port_index,
            MessageRecord::SysEx { input_index, size },
            size,
            EventDialect::Midi1,
        )
    };
    Ok(Some((
        MidiInputRecord {
            offset: header.time,
            port: ports.input_port(port, dialect)?,
            flags,
            message,
        },
        bytes,
    )))
}

/// Constructed only within process, borrowing both its host list and POD scratch.
pub(crate) struct ClapMidiInput<'a, 'host> {
    pub(crate) input: &'a InputEvents<'host>,
    pub(crate) records: &'a [MidiInputRecord],
}

// The returned loan is anchored to the checked event borrow, not Clack's
// unconstrained data<'a>() lifetime. A conforming CLAP provider must keep the
// declared payload initialized, immutable and readable throughout process;
// scalar/null checks cannot establish arbitrary C pointer provenance or extent.
fn payload(event: &MidiSysExEvent) -> Option<&[u8]> {
    let size = payload_size(event, u32::MAX).ok()?;
    if size == 0 {
        return Some(&[]);
    }
    // SAFETY: only reached for callback-owned input after checked downcast,
    // representability and positive-null guards. CLAP supplies the live payload
    // allocation for this callback; the result cannot outlive this event borrow.
    Some(unsafe { event.data() })
}

impl MidiEventSource for ClapMidiInput<'_, '_> {
    fn len(&self) -> usize {
        self.records.len()
    }

    fn get(&self, index: usize) -> Option<MidiEvent<'_>> {
        let record = self.records.get(index)?;
        let message = match record.message {
            MessageRecord::Short(bytes) => MidiMessage::Short(bytes),
            MessageRecord::Ump(words) => MidiMessage::Ump(words),
            MessageRecord::SysEx { input_index, size } => {
                if input_index >= self.input.len() {
                    return None;
                }
                let event = self.input.get(input_index)?.as_event::<MidiSysExEvent>()?;
                if event.as_raw().size != size {
                    return None;
                }
                MidiMessage::SysEx(payload(event)?)
            }
        };
        Some(MidiEvent::new(record.offset, record.port, message).with_flags(record.flags))
    }
}

pub(crate) fn push(
    ports: &ClapEventPorts,
    output: &mut OutputEvents<'_>,
    event: MidiEvent<'_>,
) -> Result<(), EventOutputError> {
    let port = ports.mapped_port(event.port(), false, event.message().dialect())?;
    let mut flags = ClapFlags::empty();
    flags.set(ClapFlags::IS_LIVE, event.flags().is_live());
    flags.set(ClapFlags::DONT_RECORD, event.flags().do_not_record());
    let result = match event.message() {
        MidiMessage::Short(bytes) => {
            output.try_push(ClapMidi::new(event.offset(), port, bytes).with_flags(flags))
        }
        MidiMessage::Ump(words) => {
            output.try_push(Midi2Event::new(event.offset(), port, words).with_flags(flags))
        }
        MidiMessage::SysEx(bytes) => {
            u32::try_from(bytes.len()).map_err(|_| EventOutputError::SysExBytesExceeded)?;
            // SAFETY: the borrowed bytes remain live through this immediate
            // try_push. A retaining host must copy descriptor AND payload before
            // returning; neither the wrapper nor its pointer escapes this push.
            let raw = unsafe { MidiSysExEvent::new(event.offset(), port, bytes) }.with_flags(flags);
            output.try_push(raw)
        }
    };
    result.map_err(|_| EventOutputError::Rejected)
}
