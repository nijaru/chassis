//! External instrument fixture through the public Chassis runtime.
//!
//! This proves the neutral component model can represent an event-driven source
//! with no audio inputs and output-only process buffers, rather than relying on
//! conventional effect assumptions.

use std::{convert::Infallible, num::NonZeroU32};

use chassis_runtime::transport::TransportSnapshot;
use chassis_runtime::{
    audio::{
        AudioIoConfiguration, AudioIoConfigurationSpec, AudioIoPolicy, AudioPortDescriptor,
        AudioPortDirection, AudioPortKey, AudioPortRole, ChannelLayout, ConfiguredAudioPort,
    },
    automation::ParameterEvents,
    buffer::{ChannelBuffer, OutputEndpoint},
    events::{
        EventDialect, EventPortDescriptor, EventPortDirection, EventPortIndex, EventPortKey,
        NormalizedValue, NoteAddress, NoteEvent, NoteEventKind, NoteEvents,
    },
    process::{ActivationConfig, ProcessBlock, ProcessConfig, ProcessContext, ProcessMode},
    runtime::{Component, InstanceRuntime, Process, Processor},
    schema::ComponentSchema,
};

const AUDIO_OUTPUT: AudioPortKey = AudioPortKey::new("audio.main.out");
const NOTE_INPUT: EventPortKey = EventPortKey::new("notes.in");
const NOTE_DIALECTS: &[EventDialect] = &[EventDialect::Notes];

static AUDIO_PORTS: &[AudioPortDescriptor] = &[AudioPortDescriptor::new(
    AUDIO_OUTPUT,
    "Main Output",
    AudioPortDirection::Output,
    AudioPortRole::Main,
    false,
)];

static EVENT_PORTS: &[EventPortDescriptor] = &[EventPortDescriptor::new(
    NOTE_INPUT,
    "Notes",
    EventPortDirection::Input,
    NOTE_DIALECTS,
)];

fn instrument_audio_policy() -> AudioIoPolicy {
    AudioIoPolicy::enumerated(vec![AudioIoConfigurationSpec::new(vec![
        ConfiguredAudioPort::new(AUDIO_OUTPUT, ChannelLayout::Stereo),
    ])])
}

struct Instrument;

impl Component for Instrument {
    type Processor = InstrumentProcessor;
    type ActivationError = Infallible;

    fn schema(&self) -> Result<ComponentSchema, chassis_runtime::schema::ComponentSchemaError> {
        ComponentSchema::unidentified(
            AUDIO_PORTS.to_vec(),
            instrument_audio_policy(),
            EVENT_PORTS.to_vec(),
            vec![],
        )
    }

    fn activate(
        &self,
        config: &ActivationConfig<'_>,
        _parameters: &chassis_runtime::parameters::ParameterStore,
        _custom_state: &[chassis_runtime::state::StateEntry],
    ) -> Result<Self::Processor, Self::ActivationError> {
        Ok(InstrumentProcessor {
            output: config
                .schema()
                .audio_port_index(&AUDIO_OUTPUT)
                .expect("instrument output resolves before activation"),
            notes: config
                .schema()
                .event_port_index(&NOTE_INPUT)
                .expect("instrument note input resolves before activation"),
            gate: false,
        })
    }
}

struct InstrumentProcessor {
    output: chassis_runtime::audio::AudioPortIndex,
    notes: EventPortIndex,
    gate: bool,
}

impl Processor for InstrumentProcessor {}

impl Process<f32> for InstrumentProcessor {
    fn process(
        &mut self,
        block: &mut ProcessBlock<'_, f32>,
    ) -> Result<(), chassis_runtime::runtime::ProcessError> {
        let note_events = block.note_events();
        let initial_gate = self.gate;

        block.for_each_channel(|mut channel| {
            if channel
                .output_endpoint()
                .is_none_or(|endpoint| endpoint.port_index() != self.output)
            {
                return;
            }

            let samples = channel
                .output_mut()
                .expect("instrument channels are output-only");
            let mut gate = initial_gate;
            let mut events = note_events.for_port(self.notes).peekable();

            for (offset, sample) in samples.iter_mut().enumerate() {
                let offset = u32::try_from(offset).expect("test block offset fits u32");
                while events.peek().is_some_and(|event| event.offset() == offset) {
                    let event = events.next().expect("peeked event exists");
                    apply_gate_event(&mut gate, event.kind());
                }
                *sample = if gate { 0.25 } else { 0.0 };
            }
        });

        for event in note_events.for_port(self.notes) {
            apply_gate_event(&mut self.gate, event.kind());
        }
        Ok(())
    }
}

fn apply_gate_event(gate: &mut bool, event: NoteEventKind) {
    match event {
        NoteEventKind::Expression { .. } => {}
        NoteEventKind::On { .. } => *gate = true,
        NoteEventKind::Off { .. } | NoteEventKind::Choke { .. } | NoteEventKind::End { .. } => {
            *gate = false;
        }
    }
}

fn process_config() -> ProcessConfig {
    ProcessConfig::new(
        48_000.0,
        NonZeroU32::new(1),
        NonZeroU32::new(64).expect("maximum is non-zero"),
        0,
    )
    .expect("instrument process configuration is valid")
    .with_max_note_events(8)
}

fn note_address(port: EventPortIndex, key: u8) -> NoteAddress {
    NoteAddress::new(
        port,
        None,
        None,
        Some(chassis_runtime::events::NoteKey::new(key).expect("test key is valid")),
    )
}

fn velocity(value: f64) -> NormalizedValue {
    NormalizedValue::new(value).expect("test velocity is normalized")
}

fn assert_samples(actual: &[f32], expected: &[f32]) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        assert!((*actual - *expected).abs() <= f32::EPSILON);
    }
}

#[test]
fn note_input_drives_stereo_output_without_audio_input() {
    let component = Instrument;
    let mut runtime: InstanceRuntime<InstrumentProcessor> =
        InstanceRuntime::for_component(&component).expect("instrument schema is valid");

    assert_eq!(runtime.audio_ports().len(), 1);
    assert_eq!(
        runtime.audio_ports()[0].direction,
        AudioPortDirection::Output
    );
    let output = runtime
        .audio_port_index(&AUDIO_OUTPUT)
        .expect("instrument output has a dense runtime index");
    let note_port = runtime
        .event_port_index(&NOTE_INPUT)
        .expect("instrument note input has a dense runtime index");

    let audio = [ConfiguredAudioPort::new(
        AUDIO_OUTPUT,
        ChannelLayout::Stereo,
    )];
    runtime
        .activate(
            &component,
            process_config(),
            AudioIoConfiguration::new(&audio),
        )
        .expect("instrument activates with output-only stereo I/O");

    let notes = [
        NoteEvent::new(
            2,
            NoteEventKind::On {
                address: note_address(note_port, 60),
                velocity: velocity(0.8),
            },
        ),
        NoteEvent::new(
            6,
            NoteEventKind::Off {
                address: note_address(note_port, 60),
                velocity: velocity(0.2),
            },
        ),
    ];
    let note_events = NoteEvents::new(&notes, 8, 8).expect("instrument notes are valid");
    let context = ProcessContext::with_note_events(
        ProcessMode::Realtime,
        &TransportSnapshot::UNKNOWN,
        ParameterEvents::empty_for_block(8),
        note_events,
    );

    let mut left = [-1.0_f32; 8];
    let mut right = [-1.0_f32; 8];
    let mut buffers = [
        ChannelBuffer::output_only(OutputEndpoint::new(output, 0), &mut left, 8)
            .expect("left output buffer is valid"),
        ChannelBuffer::output_only(OutputEndpoint::new(output, 1), &mut right, 8)
            .expect("right output buffer is valid"),
    ];

    runtime
        .process(8, context, &mut buffers)
        .expect("instrument output-only process block succeeds");

    let expected = [0.0, 0.0, 0.25, 0.25, 0.25, 0.25, 0.0, 0.0];
    assert_samples(&left, &expected);
    assert_samples(&right, &expected);
}
