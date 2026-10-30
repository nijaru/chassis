//! Native-note port projection and checked process-time translation.

#[cfg(test)]
mod tests;

use chassis_runtime::events::{
    EventDialect, EventPortDescriptor, EventPortDirection, EventPortIndex, EventPortKey,
    NormalizedValue, NoteAddress, NoteChannel, NoteEvent, NoteEventKind, NoteEventSink,
    NoteExpression, NoteExpressionKind, NoteId, NoteKey, NoteOutputError,
};
use clack_extensions::note_ports::{NoteDialect, NoteDialects, NotePortInfo};
use clack_plugin::{
    events::{
        Event, Pckn, UnknownEvent,
        event_types::{
            NoteChokeEvent, NoteEndEvent, NoteExpressionEvent, NoteExpressionType, NoteOffEvent,
            NoteOnEvent,
        },
        io::OutputEvents,
    },
    utils::ClapId,
};
use core::fmt;

/// Explicit stable CLAP identity for one component event port.
///
/// IDs are unique within a direction; they are never dense process indices.
/// Ports currently support the complete native note/expression dialect only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClapNotePort {
    /// Stable component event-port key.
    pub key: EventPortKey,
    /// Stable CLAP ID; `u32::MAX` is reserved.
    pub id: u32,
}

impl ClapNotePort {
    /// Declare one stable CLAP projection.
    #[must_use]
    pub const fn new(key: EventPortKey, id: u32) -> Self {
        Self { key, id }
    }
}

#[derive(Debug)]
pub(crate) struct NoteMappingError(&'static str);

impl fmt::Display for NoteMappingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for NoteMappingError {}

struct Binding {
    id: u32,
    name: String,
    index: EventPortIndex,
}

/// Setup metadata and precomputed numeric callback maps, owned once per instance.
pub(crate) struct ClapNoteConfiguration {
    inputs: Vec<Binding>,
    outputs: Vec<Binding>,
    input_indices: Vec<Option<i16>>,
    output_indices: Vec<Option<i16>>,
}

impl ClapNoteConfiguration {
    pub(crate) fn new(
        descriptors: &[EventPortDescriptor],
        mapping: &[ClapNotePort],
    ) -> Result<Self, NoteMappingError> {
        if descriptors.len() != mapping.len() {
            return Err(NoteMappingError(
                "CLAP note mapping must cover every event port exactly once",
            ));
        }
        let mut inputs = Vec::new();
        let mut outputs = Vec::new();
        for (position, descriptor) in descriptors.iter().enumerate() {
            if descriptor.dialects.as_ref() != [EventDialect::Notes] {
                return Err(NoteMappingError(
                    "CLAP event ports support only the native note/expression dialect",
                ));
            }
            if descriptor.name.len() > 255 || descriptor.name.as_bytes().contains(&0) {
                return Err(NoteMappingError("CLAP note-port name is not representable"));
            }
            let mut matches = mapping.iter().filter(|port| port.key == descriptor.key);
            let mapped = matches
                .next()
                .ok_or(NoteMappingError("Missing CLAP note-port mapping"))?;
            if matches.next().is_some() {
                return Err(NoteMappingError("Duplicate CLAP note-port mapping"));
            }
            if mapped.id == u32::MAX {
                return Err(NoteMappingError("Invalid CLAP note-port ID"));
            }
            let binding = Binding {
                id: mapped.id,
                name: descriptor.name.to_string(),
                index: EventPortIndex::new(
                    u32::try_from(position)
                        .map_err(|_| NoteMappingError("Event-port index is not representable"))?,
                ),
            };
            match descriptor.direction {
                EventPortDirection::Input => inputs.push(binding),
                EventPortDirection::Output => outputs.push(binding),
            }
        }
        let mut input_indices = vec![None; descriptors.len()];
        let mut output_indices = vec![None; descriptors.len()];
        for (ports, reverse) in [
            (&mut inputs, &mut input_indices),
            (&mut outputs, &mut output_indices),
        ] {
            if ports.len() > 32_768 {
                return Err(NoteMappingError("Too many native CLAP note ports"));
            }
            // Dense host indices are setup projections of stable IDs, not identity.
            ports.sort_unstable_by_key(|port| port.id);
            if ports.windows(2).any(|pair| pair[0].id == pair[1].id) {
                return Err(NoteMappingError(
                    "Duplicate CLAP note-port ID in one direction",
                ));
            }
            for (position, port) in ports.iter().enumerate() {
                let index = usize::try_from(port.index.get())
                    .map_err(|_| NoteMappingError("Event-port index is not representable"))?;
                reverse[index] =
                    Some(i16::try_from(position).map_err(|_| {
                        NoteMappingError("CLAP note-port index is not representable")
                    })?);
            }
        }
        Ok(Self {
            inputs,
            outputs,
            input_indices,
            output_indices,
        })
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.inputs.is_empty() && self.outputs.is_empty()
    }

    pub(crate) fn has_inputs(&self) -> bool {
        !self.inputs.is_empty()
    }

    pub(crate) fn count(&self, is_input: bool) -> u32 {
        let count = if is_input {
            self.inputs.len()
        } else {
            self.outputs.len()
        };
        u32::try_from(count).expect("setup bounds native port counts to 32768")
    }

    pub(crate) fn info(&self, index: u32, is_input: bool) -> Option<NotePortInfo<'_>> {
        let ports = if is_input {
            &self.inputs
        } else {
            &self.outputs
        };
        let port = ports.get(usize::try_from(index).ok()?)?;
        Some(NotePortInfo {
            id: ClapId::new(port.id),
            name: port.name.as_bytes(),
            supported_dialects: NoteDialects::CLAP,
            preferred_dialect: Some(NoteDialect::Clap),
        })
    }

    fn address(
        &self,
        port: i16,
        id: i32,
        channel: i16,
        key: i16,
        concrete: bool,
    ) -> Result<NoteAddress, &'static str> {
        let port = match port {
            -1 if !concrete && self.has_inputs() => None,
            0.. => Some(
                self.inputs
                    .get(usize::try_from(port).map_err(|_| "Invalid CLAP note port")?)
                    .ok_or("Unknown CLAP note input port")?
                    .index,
            ),
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
            0..=15 => {
                NoteChannel::new(u8::try_from(channel).map_err(|_| "Invalid CLAP note channel")?)
            }
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
        &self,
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
                    NoteOnEvent::TYPE_ID => {
                        event.as_event::<NoteOnEvent>().map(NoteOnEvent::as_raw)
                    }
                    NoteOffEvent::TYPE_ID => {
                        event.as_event::<NoteOffEvent>().map(NoteOffEvent::as_raw)
                    }
                    _ => event
                        .as_event::<NoteChokeEvent>()
                        .map(NoteChokeEvent::as_raw),
                }
                .ok_or("Malformed CLAP native note event")?;
                let address = self.address(
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
                let address =
                    self.address(raw.port_index, raw.note_id, raw.channel, raw.key, false)?;
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
                let expression = NoteExpression::new(kind, raw.value)
                    .ok_or("Invalid CLAP note expression value")?;
                NoteEventKind::Expression {
                    address,
                    expression,
                }
            }
            NoteEndEvent::TYPE_ID => return Err("CLAP note-end events are plugin-to-host only"),
            // Do not dereference payloads for dialects this deployment does not claim.
            clap_sys::events::CLAP_EVENT_MIDI
            | clap_sys::events::CLAP_EVENT_MIDI_SYSEX
            | clap_sys::events::CLAP_EVENT_MIDI2 => {
                return Err("CLAP MIDI dialects are unsupported");
            }
            _ => return Ok(None),
        };
        Ok(Some(NoteEvent::new(header.time, kind)))
    }

    fn pckn(
        &self,
        address: NoteAddress,
        input: bool,
        concrete: bool,
    ) -> Result<Pckn, NoteOutputError> {
        let indices = if input {
            &self.input_indices
        } else {
            &self.output_indices
        };
        let port = match address.port() {
            Some(port) => indices
                .get(usize::try_from(port.get()).map_err(|_| NoteOutputError::Rejected)?)
                .copied()
                .flatten()
                .ok_or(NoteOutputError::Rejected)?,
            None if !concrete => -1,
            None => return Err(NoteOutputError::Rejected),
        };
        let id = address.note_id().map_or(Ok(-1), |id| {
            i32::try_from(id.get()).map_err(|_| NoteOutputError::Rejected)
        })?;
        let channel = address.channel().map_or(-1, |value| i16::from(value.get()));
        let key = address.key().map_or(-1, |value| i16::from(value.get()));
        if concrete && (channel < 0 || key < 0) {
            return Err(NoteOutputError::Rejected);
        }
        Ok(Pckn::from_raw(port, channel, key, id))
    }
}

pub(crate) struct ClapNoteOutput<'a, 'b> {
    pub(crate) configuration: &'a ClapNoteConfiguration,
    pub(crate) output: &'a mut OutputEvents<'b>,
}

impl NoteEventSink for ClapNoteOutput<'_, '_> {
    fn try_push(&mut self, event: NoteEvent) -> Result<(), NoteOutputError> {
        let address = event.kind().address();
        let pckn = self.configuration.pckn(
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
                self.output
                    .try_push(NoteOnEvent::new(time, pckn, velocity.get()))
            }
            NoteEventKind::Off { velocity, .. } => {
                self.output
                    .try_push(NoteOffEvent::new(time, pckn, velocity.get()))
            }
            NoteEventKind::Choke { .. } => self.output.try_push(NoteChokeEvent::new(time, pckn)),
            NoteEventKind::End { .. } => self.output.try_push(NoteEndEvent::new(time, pckn)),
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
                self.output.try_push(NoteExpressionEvent::new(
                    time,
                    pckn,
                    kind,
                    expression.value(),
                ))
            }
        };
        result.map_err(|_| NoteOutputError::Rejected)
    }
}
