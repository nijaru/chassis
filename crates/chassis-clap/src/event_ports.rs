//! Stable CLAP event-port metadata and direction/dialect index projections.

#[cfg(test)]
mod tests;

use chassis_runtime::events::{
    EventDialect, EventOutputError, EventPortDescriptor, EventPortDirection, EventPortIndex,
    EventPortKey,
};
use clack_extensions::note_ports::{NoteDialect, NoteDialects, NotePortInfo};
use clack_plugin::utils::ClapId;
use core::fmt;

/// Explicit stable CLAP identity for one component event port.
///
/// IDs are unique within a direction; they are never dense process indices.
/// Dialects come from the schema; the preferred supported dialect is explicit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClapNotePort {
    /// Stable component event-port key.
    pub key: EventPortKey,
    /// Stable CLAP ID; `u32::MAX` is reserved.
    pub id: u32,
    /// Preferred supported wire dialect; never an implicit conversion policy.
    pub preferred: EventDialect,
}

impl ClapNotePort {
    /// Declare one stable CLAP projection.
    #[must_use]
    pub const fn new(key: EventPortKey, id: u32, preferred: EventDialect) -> Self {
        Self { key, id, preferred }
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

fn dialect(value: EventDialect) -> Result<NoteDialect, NoteMappingError> {
    match value {
        EventDialect::Notes => Ok(NoteDialect::Clap),
        EventDialect::Midi1 => Ok(NoteDialect::Midi),
        EventDialect::Ump => Ok(NoteDialect::Midi2),
        _ => Err(NoteMappingError("Unsupported CLAP event dialect")),
    }
}

struct Binding {
    id: u32,
    name: String,
    index: EventPortIndex,
    supported: NoteDialects,
    preferred: NoteDialect,
}

/// Setup metadata and precomputed numeric callback maps, owned once per instance.
pub(crate) struct ClapEventPorts {
    inputs: Vec<Binding>,
    outputs: Vec<Binding>,
    input_indices: Vec<Option<u16>>,
    output_indices: Vec<Option<u16>>,
}

impl ClapEventPorts {
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
            let mut supported = NoteDialects::empty();
            for &value in descriptor.dialects.iter() {
                supported |= dialect(value)?.into();
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
            let preferred = dialect(mapped.preferred)?;
            if !supported.supports(preferred) {
                return Err(NoteMappingError(
                    "Preferred CLAP dialect is not supported by the port",
                ));
            }
            let binding = Binding {
                supported,
                preferred,
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
            if ports.len() > 65_536 {
                return Err(NoteMappingError("Too many CLAP event ports"));
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
                let host_index = u16::try_from(position)
                    .map_err(|_| NoteMappingError("CLAP event-port index is not representable"))?;
                if port.supported.contains(NoteDialects::CLAP) && i16::try_from(host_index).is_err()
                {
                    return Err(NoteMappingError(
                        "Native CLAP port index exceeds signed range",
                    ));
                }
                reverse[index] = Some(host_index);
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

    pub(crate) fn has_input_dialect(&self, value: EventDialect) -> bool {
        dialect(value).is_ok_and(|value| {
            self.inputs
                .iter()
                .any(|port| port.supported.supports(value))
        })
    }

    pub(crate) fn input_port(
        &self,
        index: u16,
        value: EventDialect,
    ) -> Result<EventPortIndex, &'static str> {
        let port = self
            .inputs
            .get(usize::from(index))
            .ok_or("Unknown CLAP event input port")?;
        if !port
            .supported
            .supports(dialect(value).map_err(|_| "Unsupported CLAP event dialect")?)
        {
            return Err("Unsupported dialect on CLAP event input port");
        }
        Ok(port.index)
    }

    pub(crate) fn mapped_port(
        &self,
        index: EventPortIndex,
        input: bool,
        value: EventDialect,
    ) -> Result<u16, EventOutputError> {
        let (indices, ports) = if input {
            (&self.input_indices, &self.inputs)
        } else {
            (&self.output_indices, &self.outputs)
        };
        let index = indices
            .get(usize::try_from(index.get()).map_err(|_| EventOutputError::Rejected)?)
            .copied()
            .flatten()
            .ok_or(EventOutputError::Rejected)?;
        if !ports[usize::from(index)]
            .supported
            .supports(dialect(value).map_err(|_| EventOutputError::Rejected)?)
        {
            return Err(EventOutputError::Rejected);
        }
        Ok(index)
    }

    pub(crate) fn count(&self, is_input: bool) -> u32 {
        let count = if is_input {
            self.inputs.len()
        } else {
            self.outputs.len()
        };
        u32::try_from(count).expect("setup bounds event-port counts to 65536")
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
            supported_dialects: port.supported,
            preferred_dialect: Some(port.preferred),
        })
    }
}
