//! Checked native-note admission and wire output.

use crate::event_ports::ClapEventPorts;
use chassis_runtime::events::{
    EventDialect, EventOutputError, NormalizedValue, NoteAddress, NoteChannel, NoteEvent,
    NoteEventKind, NoteExpression, NoteExpressionKind, NoteId, NoteKey,
};
use clack_plugin::events::{
    Event, Pckn, UnknownEvent,
    event_types::{
        NoteChokeEvent, NoteEndEvent, NoteExpressionEvent, NoteExpressionType, NoteOffEvent,
        NoteOnEvent,
    },
    io::OutputEvents,
};

fn address(
    configuration: &ClapEventPorts,
    port: i16,
    id: i32,
    channel: i16,
    key: i16,
    concrete: bool,
) -> Result<NoteAddress, &'static str> {
    let port = match port {
        -1 if !concrete && configuration.has_input_dialect(EventDialect::Notes) => None,
        0.. => Some(configuration.input_port(
            u16::try_from(port).map_err(|_| "Invalid CLAP note input port")?,
            EventDialect::Notes,
        )?),
        _ => return Err("Invalid CLAP note input port"),
    };
    let note_id = match id {
        -1 => None,
        0.. => Some(NoteId::new(
            u32::try_from(id).map_err(|_| "Invalid CLAP note ID")?,
        )),
        _ => return Err("Invalid CLAP note ID"),
    };
    let channel = match channel {
        -1 if !concrete => None,
        0..=15 => NoteChannel::new(u8::try_from(channel).map_err(|_| "Invalid CLAP note channel")?),
        _ => return Err("Invalid CLAP note channel"),
    };
    let key = match key {
        -1 if !concrete => None,
        0..=127 => NoteKey::new(u8::try_from(key).map_err(|_| "Invalid CLAP note key")?),
        _ => return Err("Invalid CLAP note key"),
    };
    Ok(NoteAddress::with_optional_port(port, note_id, channel, key))
}

pub(crate) fn normalize(
    configuration: &ClapEventPorts,
    event: &UnknownEvent,
) -> Result<Option<NoteEvent>, &'static str> {
    let header = event.header().as_raw();
    if header.space_id != 0 {
        return Ok(None);
    }
    let kind = match header.type_ {
        NoteOnEvent::TYPE_ID | NoteOffEvent::TYPE_ID | NoteChokeEvent::TYPE_ID => {
            // Checked downcasts validate exact space/type/size before raw field access.
            let raw = match header.type_ {
                NoteOnEvent::TYPE_ID => event.as_event::<NoteOnEvent>().map(NoteOnEvent::as_raw),
                NoteOffEvent::TYPE_ID => event.as_event::<NoteOffEvent>().map(NoteOffEvent::as_raw),
                _ => event
                    .as_event::<NoteChokeEvent>()
                    .map(NoteChokeEvent::as_raw),
            }
            .ok_or("Malformed CLAP native note event")?;
            let address = address(
                configuration,
                raw.port_index,
                raw.note_id,
                raw.channel,
                raw.key,
                header.type_ == NoteOnEvent::TYPE_ID,
            )?;
            match header.type_ {
                NoteOnEvent::TYPE_ID => NoteEventKind::On {
                    address,
                    velocity: NormalizedValue::new(raw.velocity)
                        .ok_or("Invalid CLAP note-on velocity")?,
                },
                NoteOffEvent::TYPE_ID => NoteEventKind::Off {
                    address,
                    velocity: NormalizedValue::new(raw.velocity)
                        .ok_or("Invalid CLAP note-off velocity")?,
                },
                _ => NoteEventKind::Choke { address },
            }
        }
        NoteExpressionEvent::TYPE_ID => {
            let raw = event
                .as_event::<NoteExpressionEvent>()
                .ok_or("Malformed CLAP note expression event")?
                .as_raw();
            let address = address(
                configuration,
                raw.port_index,
                raw.note_id,
                raw.channel,
                raw.key,
                false,
            )?;
            let kind = match raw.expression_id {
                0 => NoteExpressionKind::Volume,
                1 => NoteExpressionKind::Pan,
                2 => NoteExpressionKind::Tuning,
                3 => NoteExpressionKind::Vibrato,
                4 => NoteExpressionKind::Expression,
                5 => NoteExpressionKind::Brightness,
                6 => NoteExpressionKind::Pressure,
                _ => return Err("Unsupported CLAP note expression"),
            };
            let expression =
                NoteExpression::new(kind, raw.value).ok_or("Invalid CLAP note expression value")?;
            NoteEventKind::Expression {
                address,
                expression,
            }
        }
        NoteEndEvent::TYPE_ID => return Err("CLAP note-end events are plugin-to-host only"),
        clap_sys::events::CLAP_EVENT_MIDI2 => return Err("CLAP MIDI 2 is unsupported"),
        _ => return Ok(None),
    };
    Ok(Some(NoteEvent::new(header.time, kind)))
}

fn pckn(
    configuration: &ClapEventPorts,
    address: NoteAddress,
    input: bool,
    concrete: bool,
) -> Result<Pckn, EventOutputError> {
    let port = match address.port() {
        Some(port) => i16::try_from(configuration.mapped_port(port, input, EventDialect::Notes)?)
            .map_err(|_| EventOutputError::Rejected)?,
        None if !concrete => -1,
        None => return Err(EventOutputError::Rejected),
    };
    let id = address.note_id().map_or(Ok(-1), |id| {
        i32::try_from(id.get()).map_err(|_| EventOutputError::Rejected)
    })?;
    let channel = address.channel().map_or(-1, |value| i16::from(value.get()));
    let key = address.key().map_or(-1, |value| i16::from(value.get()));
    if concrete && (channel < 0 || key < 0) {
        return Err(EventOutputError::Rejected);
    }
    Ok(Pckn::from_raw(port, channel, key, id))
}

pub(crate) fn push(
    configuration: &ClapEventPorts,
    output: &mut OutputEvents<'_>,
    event: NoteEvent,
) -> Result<(), EventOutputError> {
    let address = event.kind().address();
    let pckn = pckn(
        configuration,
        address,
        matches!(event.kind(), NoteEventKind::End { .. }),
        matches!(
            event.kind(),
            NoteEventKind::On { .. } | NoteEventKind::End { .. }
        ),
    )?;
    let time = event.offset();
    let result = match event.kind() {
        NoteEventKind::On { velocity, .. } => {
            output.try_push(NoteOnEvent::new(time, pckn, velocity.get()))
        }
        NoteEventKind::Off { velocity, .. } => {
            output.try_push(NoteOffEvent::new(time, pckn, velocity.get()))
        }
        NoteEventKind::Choke { .. } => output.try_push(NoteChokeEvent::new(time, pckn)),
        NoteEventKind::End { .. } => output.try_push(NoteEndEvent::new(time, pckn)),
        NoteEventKind::Expression { expression, .. } => {
            let kind = match expression.kind() {
                NoteExpressionKind::Volume => NoteExpressionType::Volume,
                NoteExpressionKind::Pan => NoteExpressionType::Pan,
                NoteExpressionKind::Tuning => NoteExpressionType::Tuning,
                NoteExpressionKind::Vibrato => NoteExpressionType::Vibrato,
                NoteExpressionKind::Expression => NoteExpressionType::Expression,
                NoteExpressionKind::Brightness => NoteExpressionType::Brightness,
                NoteExpressionKind::Pressure => NoteExpressionType::Pressure,
            };
            output.try_push(NoteExpressionEvent::new(
                time,
                pckn,
                kind,
                expression.value(),
            ))
        }
    };
    result.map_err(|_| EventOutputError::Rejected)
}
