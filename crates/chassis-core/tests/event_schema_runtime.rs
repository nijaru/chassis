//! External event-schema authority tests through public Chassis APIs.

use std::{convert::Infallible, num::NonZeroU32};

use chassis_core::{
    audio::AudioIoConfiguration,
    automation::ParameterEvents,
    buffer::ChannelBuffer,
    events::{
        EventDialect, EventPortDescriptor, EventPortDirection, EventPortIndex, EventPortKey,
        EventPortSchemaError, NormalizedValue, NoteAddress, NoteEvent, NoteEventKind,
        NoteEventPortError, NoteEvents,
    },
    process::{
        ActivationConfig, ProcessBlock, ProcessBufferSource, ProcessConfig, ProcessContext,
        ProcessMode, TransportSnapshot,
    },
    runtime::{
        ActivateError, Component, InstanceProcessError, InstanceRuntime, InstanceRuntimeError,
        Process, Processor,
    },
};

const NOTES: &[EventDialect] = &[EventDialect::Notes];
const INPUT_KEY: EventPortKey = EventPortKey::new("notes.in");
static INPUT_PORTS: &[EventPortDescriptor] = &[EventPortDescriptor::new(
    INPUT_KEY,
    "Notes In",
    EventPortDirection::Input,
    NOTES,
)];
static OUTPUT_PORTS: &[EventPortDescriptor] = &[EventPortDescriptor::new(
    EventPortKey::new("notes.out"),
    "Notes Out",
    EventPortDirection::Output,
    NOTES,
)];
static DUPLICATE_PORTS: &[EventPortDescriptor] = &[
    EventPortDescriptor::new(INPUT_KEY, "First", EventPortDirection::Input, NOTES),
    EventPortDescriptor::new(INPUT_KEY, "Second", EventPortDirection::Input, NOTES),
];

struct EventComponent {
    ports: &'static [EventPortDescriptor],
}

impl Component for EventComponent {
    type Processor = EventProcessor;
    type ActivationError = Infallible;

    fn schema(
        &self,
    ) -> Result<chassis_core::schema::ComponentSchema, chassis_core::schema::ComponentSchemaError>
    {
        chassis_core::schema::ComponentSchema::unidentified(vec![], self.ports.to_vec(), vec![])
    }

    fn activate(
        &self,
        _config: &ActivationConfig<'_>,
    ) -> Result<Self::Processor, Self::ActivationError> {
        Ok(EventProcessor)
    }
}

struct EventProcessor;

impl Processor for EventProcessor {}

impl Process<f32> for EventProcessor {
    fn process<B>(&mut self, _block: &mut ProcessBlock<'_, '_, '_, f32, B>)
    where
        B: ProcessBufferSource<f32> + ?Sized,
    {
    }
}

fn process_config() -> ProcessConfig {
    ProcessConfig::new(
        48_000.0,
        NonZeroU32::new(1),
        NonZeroU32::new(64).expect("maximum is non-zero"),
        0,
    )
    .expect("test process configuration is valid")
    .with_max_note_events(8)
}

fn velocity() -> NormalizedValue {
    NormalizedValue::new(0.75).expect("test velocity is normalized")
}

#[test]
fn invalid_event_schema_is_rejected_before_instance_publication() {
    let component = EventComponent {
        ports: DUPLICATE_PORTS,
    };
    let result = InstanceRuntime::<EventProcessor>::for_component(&component);

    assert!(matches!(
        result,
        Err(InstanceRuntimeError::InvalidEventPorts(
            EventPortSchemaError::DuplicateKey(key)
        )) if key == INPUT_KEY
    ));
}

#[test]
fn activation_rejects_component_event_schema_drift() {
    let original = EventComponent { ports: INPUT_PORTS };
    let changed = EventComponent {
        ports: OUTPUT_PORTS,
    };
    let mut runtime =
        InstanceRuntime::<EventProcessor>::for_component(&original).expect("schema is valid");

    assert!(matches!(
        runtime.activate(&changed, process_config(), AudioIoConfiguration::new(&[])),
        Err(ActivateError::EventPortSchemaMismatch)
    ));
}

#[test]
fn process_rejects_unknown_note_port_before_product_dsp() {
    let component = EventComponent { ports: INPUT_PORTS };
    let mut runtime =
        InstanceRuntime::<EventProcessor>::for_component(&component).expect("schema is valid");
    assert_eq!(
        runtime.event_port_index(&INPUT_KEY),
        Some(EventPortIndex::new(0))
    );
    runtime
        .activate(&component, process_config(), AudioIoConfiguration::new(&[]))
        .expect("component activates");

    let raw_notes = [NoteEvent::new(
        0,
        NoteEventKind::On {
            address: NoteAddress::new(EventPortIndex::new(1), None, None, None),
            velocity: velocity(),
        },
    )];
    let notes = NoteEvents::new(&raw_notes, 1, 8).expect("note event shape is valid");
    let context = ProcessContext::with_note_events(
        ProcessMode::Realtime,
        TransportSnapshot::unknown(),
        ParameterEvents::empty_for_block(1),
        notes,
    );
    let mut buffers: [ChannelBuffer<'_, f32>; 0] = [];

    assert_eq!(
        runtime.process(1, context, &mut buffers),
        Err(InstanceProcessError::InvalidNoteEvents(
            NoteEventPortError::UnknownPort(EventPortIndex::new(1))
        ))
    );
}
