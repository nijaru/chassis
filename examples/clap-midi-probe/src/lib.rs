//! Opaque MIDI 1/SysEx and MIDI 2/UMP router with no audio or retained payloads.
#![deny(unsafe_code)]

use chassis_clap::{
    ClapAudioPort, ClapComponent, ClapNotePort, SingleComponentEntry, clack_export_entry,
};
use chassis_runtime::{
    audio::{AudioIoConfigurationSpec, AudioIoPolicy},
    automation::{ParameterEventChange, ParameterEventValue},
    events::{EventDialect, EventPortDescriptor, EventPortDirection, EventPortIndex, EventPortKey},
    parameters::{ParameterDescriptor, ParameterIndex, ParameterStore},
    process::{ActivationConfig, InputEventRef, ProcessBlock},
    runtime::{Component, Process, ProcessError, Processor},
    schema::{ComponentId, ComponentSchema, ComponentSchemaError, StateSchemaVersion},
    state::StateEntry,
};
use core::convert::Infallible;

const PRIMARY_IN: EventPortKey = EventPortKey::new("midi.primary.in");
const SECONDARY_IN: EventPortKey = EventPortKey::new("midi.secondary.in");
const UMP_IN: EventPortKey = EventPortKey::new("ump.in");
const UMP_OUT: EventPortKey = EventPortKey::new("ump.out");
const PRIMARY_OUT: EventPortKey = EventPortKey::new("midi.primary.out");
const SECONDARY_OUT: EventPortKey = EventPortKey::new("midi.secondary.out");

/// Exported event-only component exercising independently mapped port directions.
#[derive(Default)]
pub struct MidiProbe;

impl Component for MidiProbe {
    type Processor = MidiProbeProcessor;
    type ActivationError = Infallible;

    fn schema(&self) -> Result<ComponentSchema, ComponentSchemaError> {
        ComponentSchema::new(
            ComponentId::new("org.nijaru.chassis.midi-probe").expect("static semantic ID"),
            StateSchemaVersion::new(1),
            vec![],
            AudioIoPolicy::enumerated(vec![AudioIoConfigurationSpec::new(vec![])]),
            // Schema indices differ from direction-local host indices.
            vec![
                EventPortDescriptor::new(
                    PRIMARY_IN,
                    "Primary MIDI Input",
                    EventPortDirection::Input,
                    &[EventDialect::Midi1, EventDialect::Midi2],
                ),
                EventPortDescriptor::new(
                    SECONDARY_OUT,
                    "Secondary MIDI Output",
                    EventPortDirection::Output,
                    &[EventDialect::Midi1],
                ),
                EventPortDescriptor::new(
                    SECONDARY_IN,
                    "Secondary MIDI Input",
                    EventPortDirection::Input,
                    &[EventDialect::Midi1],
                ),
                EventPortDescriptor::new(
                    PRIMARY_OUT,
                    "Primary MIDI Output",
                    EventPortDirection::Output,
                    &[EventDialect::Midi1, EventDialect::Midi2],
                ),
                EventPortDescriptor::new(
                    UMP_OUT,
                    "UMP Output",
                    EventPortDirection::Output,
                    &[EventDialect::Midi2],
                ),
                EventPortDescriptor::new(
                    UMP_IN,
                    "UMP Input",
                    EventPortDirection::Input,
                    &[EventDialect::Midi2],
                ),
            ],
            vec![
                ParameterDescriptor::boolean("enabled", "Enabled", true)
                    .expect("static Boolean parameter"),
            ],
        )
    }

    fn activate(
        &self,
        config: &ActivationConfig<'_>,
        parameters: &ParameterStore,
        _custom: &[StateEntry],
    ) -> Result<Self::Processor, Self::ActivationError> {
        let port = |key| {
            config
                .schema()
                .event_port_index(key)
                .expect("declared MIDI port")
        };
        Ok(MidiProbeProcessor {
            routes: [
                (port(&PRIMARY_IN), port(&PRIMARY_OUT)),
                (port(&SECONDARY_IN), port(&SECONDARY_OUT)),
                (port(&UMP_IN), port(&UMP_OUT)),
            ],
            enabled: parameters
                .index("enabled")
                .expect("declared enabled parameter"),
        })
    }
}

impl ClapComponent for MidiProbe {
    const CLAP_ID: &'static str = "com.nijaru.chassis.midi-probe";
    const CLAP_NAME: &'static str = "Chassis MIDI Probe";
    const CLAP_FEATURES: &'static [&'static std::ffi::CStr] = &[c"note-effect"];
    const CLAP_AUDIO_PORTS: &'static [ClapAudioPort] = &[];
    const CLAP_NOTE_PORTS: &'static [ClapNotePort] = &[
        ClapNotePort::new(PRIMARY_IN, 90, EventDialect::Midi2),
        ClapNotePort::new(SECONDARY_OUT, 80, EventDialect::Midi1),
        ClapNotePort::new(SECONDARY_IN, 10, EventDialect::Midi1),
        ClapNotePort::new(PRIMARY_OUT, 20, EventDialect::Midi2),
        ClapNotePort::new(UMP_IN, 120, EventDialect::Midi2),
        ClapNotePort::new(UMP_OUT, 100, EventDialect::Midi2),
    ];
    const CLAP_PARAMETER_IDS: &'static [(&'static str, u32)] = &[("enabled", 7008)];
    const CLAP_MAX_PARAMETER_EVENTS: u32 = 512;
    const CLAP_MAX_INPUT_EVENTS: u32 = 2048;
    const CLAP_MAX_MIDI_EVENTS: u32 = 1024;
    const CLAP_MAX_INPUT_SYSEX_BYTES: u32 = 64 * 1024;
    const CLAP_MAX_OUTPUT_MIDI_EVENTS: u32 = 1024;
    const CLAP_MAX_OUTPUT_SYSEX_BYTES: u32 = 64 * 1024;
}

/// Activation-cached dense routes; no payloads or process history are owned here.
pub struct MidiProbeProcessor {
    routes: [(EventPortIndex, EventPortIndex); 3],
    enabled: ParameterIndex,
}

// Default reset is a no-op and default tail is zero: there is no DSP history.
impl Processor for MidiProbeProcessor {}

impl Process<f32> for MidiProbeProcessor {
    fn process(&mut self, block: &mut ProcessBlock<'_, f32>) -> Result<(), ProcessError> {
        let Some(ParameterEventValue::Boolean(mut enabled)) =
            block.parameters().get_event_value(self.enabled)
        else {
            return Err(ProcessError::Message(
                "Invalid MIDI probe enabled parameter",
            ));
        };
        // Copy the validated borrowed view before mutating the output sink.
        // Fold the real mixed list: later equal-time controls cannot undo output.
        let context = block.context();
        let events = context.ordered_events().ok_or(ProcessError::Message(
            "MIDI probe requires source input order",
        ))?;
        for event in events {
            match event {
                InputEventRef::Parameter(event) => {
                    let ParameterEventChange::Set(ParameterEventValue::Boolean(value)) =
                        event.change()
                    else {
                        return Err(ProcessError::Message("Unsupported MIDI probe trajectory"));
                    };
                    enabled = value;
                }
                InputEventRef::Midi(event) => {
                    let output = self
                        .routes
                        .iter()
                        .find_map(|&(input, output)| (input == event.port()).then_some(output))
                        .ok_or(ProcessError::Message("Unknown MIDI probe route"))?;
                    if enabled {
                        // Essential output: rejection aborts endpoint completion.
                        block.try_push_midi_event(event.with_port(output))?;
                    }
                }
                InputEventRef::Note(_) => {
                    return Err(ProcessError::Message(
                        "Native notes are not declared by this router",
                    ));
                }
            }
        }
        Ok(())
    }
}

clack_export_entry!(SingleComponentEntry<MidiProbe>);
