//! Runtime-owned metadata fixture through the public Chassis runtime.
//!
//! This proves dynamically discovered/hosted components can own their audio and
//! event metadata while active processing still consumes only dense identities.

use std::{convert::Infallible, num::NonZeroU32};

use chassis_core::{
    audio::{
        AudioIoConfiguration, AudioIoConfigurationSpec, AudioIoPolicy, AudioPortDescriptor,
        AudioPortIndex, ChannelLayout, ConfiguredAudioPort, PortDirection, PortKey, PortRole,
    },
    automation::ParameterEvents,
    buffer::{ChannelBuffer, OutputEndpoint},
    events::{
        EventDialect, EventPortDescriptor, EventPortDirection, EventPortIndex, EventPortKey,
        NormalizedValue, NoteAddress, NoteEvent, NoteEventKind, NoteEvents,
    },
    process::{
        ActivationConfig, ProcessBlock, ProcessBufferSource, ProcessChannel, ProcessConfig,
        ProcessContext, ProcessMode, TransportSnapshot,
    },
    runtime::{Component, InstanceRuntime, Process, Processor},
    schema::ComponentSchema,
};

struct HostedComponent {
    audio_key: String,
    audio_name: String,
    event_key: String,
    event_name: String,
}

impl HostedComponent {
    fn discovered() -> Self {
        Self {
            audio_key: String::from("hosted.audio.out"),
            audio_name: String::from("Discovered Output"),
            event_key: String::from("hosted.notes.in"),
            event_name: String::from("Discovered Notes"),
        }
    }

    fn audio_key(&self) -> PortKey {
        PortKey::owned(self.audio_key.clone())
    }

    fn event_key(&self) -> EventPortKey {
        EventPortKey::owned(self.event_key.clone())
    }
}

impl Component for HostedComponent {
    type Processor = HostedProcessor;
    type ActivationError = Infallible;

    fn schema(&self) -> Result<ComponentSchema, chassis_core::schema::ComponentSchemaError> {
        let audio_key = self.audio_key();
        let audio_ports = vec![AudioPortDescriptor::owned(
            self.audio_key.clone(),
            self.audio_name.clone(),
            PortDirection::Output,
            PortRole::Main,
            false,
        )];
        let policy = AudioIoPolicy::enumerated(vec![AudioIoConfigurationSpec::new(vec![
            ConfiguredAudioPort::new(audio_key, ChannelLayout::Stereo),
        ])]);
        let event_ports = vec![EventPortDescriptor::owned(
            self.event_key.clone(),
            self.event_name.clone(),
            EventPortDirection::Input,
            vec![EventDialect::Notes],
        )];
        ComponentSchema::unidentified(audio_ports, policy, event_ports, vec![])
    }

    fn activate(
        &self,
        config: &ActivationConfig<'_>,
    ) -> Result<Self::Processor, Self::ActivationError> {
        Ok(HostedProcessor {
            output: config
                .schema()
                .audio_port_index(&self.audio_key())
                .expect("runtime-owned audio key resolves before activation"),
            notes: config
                .schema()
                .event_port_index(&self.event_key())
                .expect("runtime-owned event key resolves before activation"),
        })
    }
}

struct HostedProcessor {
    output: AudioPortIndex,
    notes: EventPortIndex,
}

impl Processor for HostedProcessor {}

impl Process<f32> for HostedProcessor {
    fn process<B>(&mut self, block: &mut ProcessBlock<'_, '_, '_, f32, B>)
    where
        B: ProcessBufferSource<f32> + ?Sized,
    {
        let note_on = block
            .note_events()
            .for_port(self.notes)
            .any(|event| matches!(event.kind(), NoteEventKind::On { .. }));
        let value = if note_on { 0.5 } else { 0.0 };

        for mut channel in block.channels() {
            if channel
                .output_endpoint()
                .is_none_or(|endpoint| endpoint.port_index() != self.output)
            {
                continue;
            }
            channel
                .output_mut()
                .expect("hosted fixture receives output-only channels")
                .fill(value);
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
    .expect("hosted process configuration is valid")
    .with_max_note_events(8)
}

fn velocity(value: f64) -> NormalizedValue {
    NormalizedValue::new(value).expect("test velocity is normalized")
}

fn assert_samples(actual: &[f32], expected: f32) {
    for sample in actual {
        assert!((*sample - expected).abs() <= f32::EPSILON);
    }
}

#[test]
fn runtime_owned_metadata_resolves_to_dense_process_identities() {
    let component = HostedComponent::discovered();
    let mut runtime: InstanceRuntime<HostedProcessor> =
        InstanceRuntime::for_component(&component).expect("runtime-owned schema is valid");

    assert_eq!(runtime.audio_ports()[0].key.as_str(), "hosted.audio.out");
    assert_eq!(runtime.audio_ports()[0].name.as_ref(), "Discovered Output");
    assert_eq!(runtime.event_ports()[0].key.as_str(), "hosted.notes.in");
    assert_eq!(runtime.event_ports()[0].name.as_ref(), "Discovered Notes");

    let output = runtime
        .audio_port_index(&PortKey::owned(String::from("hosted.audio.out")))
        .expect("owned audio key resolves by semantic identity");
    let note_port = runtime
        .event_port_index(&EventPortKey::owned(String::from("hosted.notes.in")))
        .expect("owned event key resolves by semantic identity");

    let audio = [ConfiguredAudioPort::new(
        PortKey::owned(String::from("hosted.audio.out")),
        ChannelLayout::Stereo,
    )];
    runtime
        .activate(
            &component,
            process_config(),
            AudioIoConfiguration::new(&audio),
        )
        .expect("runtime-owned component activates");

    let notes = [NoteEvent::new(
        0,
        NoteEventKind::On {
            address: NoteAddress::new(
                note_port,
                None,
                None,
                Some(chassis_core::events::NoteKey::new(64).expect("test note is valid")),
            ),
            velocity: velocity(0.8),
        },
    )];
    let note_events = NoteEvents::new(&notes, 4, 8).expect("hosted note events are valid");
    let context = ProcessContext::with_note_events(
        ProcessMode::Realtime,
        TransportSnapshot::unknown(),
        ParameterEvents::empty_for_block(4),
        note_events,
    );

    let mut left = [-1.0_f32; 4];
    let mut right = [-1.0_f32; 4];
    let mut buffers = [
        ChannelBuffer::output_only(OutputEndpoint::new(output, 0), &mut left, 4)
            .expect("hosted left output is valid"),
        ChannelBuffer::output_only(OutputEndpoint::new(output, 1), &mut right, 4)
            .expect("hosted right output is valid"),
    ];
    runtime
        .process(4, context, &mut buffers)
        .expect("hosted process block succeeds");

    assert_samples(&left, 0.5);
    assert_samples(&right, 0.5);
}
