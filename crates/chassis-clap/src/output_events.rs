//! One CLAP output queue shared by every supported event family.

use chassis_runtime::events::{EventOutputError, EventSink, OutputEvent};
use clack_plugin::events::io::OutputEvents;

use crate::{event_ports::ClapEventPorts, midi, notes};

pub(crate) struct ClapEventOutput<'a, 'host> {
    pub(crate) configuration: &'a ClapEventPorts,
    pub(crate) output: &'a mut OutputEvents<'host>,
}

impl EventSink for ClapEventOutput<'_, '_> {
    fn try_push(&mut self, event: OutputEvent<'_>) -> Result<(), EventOutputError> {
        match event {
            OutputEvent::Note(event) => notes::push(self.configuration, self.output, event),
            OutputEvent::Midi(event) => midi::push(self.configuration, self.output, event),
        }
    }
}
