//! Activation-bounded normalization retaining the host's mixed-list order.

use chassis_runtime::{
    automation::ParameterEvent, events::NoteEvent, midi::MidiEventLimits, process::InputEventIndex,
};
use clack_plugin::{events::io::InputEvents, prelude::PluginError};

use crate::{
    event_ports::ClapEventPorts,
    midi::{self, MidiInputRecord},
    parameters::{ClapStateProjection, normalize_input_event},
};

pub(crate) struct InputScratch {
    pub(crate) parameters: Vec<ParameterEvent>,
    pub(crate) notes: Vec<NoteEvent>,
    pub(crate) midi: Vec<MidiInputRecord>,
    pub(crate) midi_limits: MidiEventLimits,
    pub(crate) order: Vec<InputEventIndex>,
    pub(crate) maximum_parameters: u32,
    pub(crate) maximum_notes: u32,
    maximum_input: u32,
}

fn reserved<T>(maximum: u32) -> Result<Vec<T>, PluginError> {
    let count = usize::try_from(maximum)
        .map_err(|_| PluginError::Message("CLAP event bound is not representable"))?;
    let mut events = Vec::new();
    events.try_reserve_exact(count).map_err(PluginError::from)?;
    Ok(events)
}

impl InputScratch {
    pub(crate) fn new(
        maximum_parameters: u32,
        maximum_notes: u32,
        maximum_input: u32,
        midi_limits: MidiEventLimits,
    ) -> Result<Self, PluginError> {
        Ok(Self {
            parameters: reserved(maximum_parameters)?,
            notes: reserved(maximum_notes)?,
            midi: reserved(midi_limits.max_events())?,
            midi_limits,
            order: reserved(maximum_input)?,
            maximum_parameters,
            maximum_notes,
            maximum_input,
        })
    }

    pub(crate) fn normalize(
        &mut self,
        parameters: &ClapStateProjection,
        notes: &ClapEventPorts,
        input: &InputEvents<'_>,
        frame_count: u32,
    ) -> Result<(), &'static str> {
        self.parameters.clear();
        self.notes.clear();
        self.midi.clear();
        let mut sysex_bytes = 0;
        self.order.clear();
        let count = input.len();
        if count > self.maximum_input {
            return Err("CLAP input event count exceeds the activation bound");
        }
        let mut previous = None;
        for position in 0..count {
            let event = input.get(position).ok_or("Missing CLAP input event")?;
            let time = event.header().as_raw().time;
            if time >= frame_count || previous.is_some_and(|previous| time < previous) {
                return Err("Invalid CLAP input event timing");
            }
            previous = Some(time);
            if let Some(event) = normalize_input_event(parameters, event)? {
                let index = u32::try_from(self.parameters.len())
                    .map_err(|_| "CLAP parameter index is not representable")?;
                if index >= self.maximum_parameters {
                    return Err("CLAP parameter event count exceeds the activation bound");
                }
                self.parameters.push(event);
                self.order.push(InputEventIndex::Parameter(index));
            } else if let Some(event) = crate::notes::normalize(notes, event)? {
                let index = u32::try_from(self.notes.len())
                    .map_err(|_| "CLAP note index is not representable")?;
                if index >= self.maximum_notes {
                    return Err("CLAP note event count exceeds the activation bound");
                }
                self.notes.push(event);
                self.order.push(InputEventIndex::Note(index));
            } else if let Some((event, bytes)) = midi::normalize(
                notes,
                event,
                position,
                self.midi_limits.max_sysex_bytes() - sysex_bytes,
            )? {
                let index = u32::try_from(self.midi.len())
                    .map_err(|_| "CLAP MIDI index is not representable")?;
                if index >= self.midi_limits.max_events() {
                    return Err("CLAP MIDI event count exceeds the activation bound");
                }
                // normalize checked bytes against the remaining aggregate before
                // constructing any payload loan; this addition cannot overflow.
                sysex_bytes += bytes;
                self.midi.push(event);
                self.order.push(InputEventIndex::Midi(index));
            }
        }
        Ok(())
    }
}
