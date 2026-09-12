//! External semantic-note fixture through the public Chassis runtime.

use std::{
    convert::Infallible,
    num::NonZeroU32,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};

use chassis_core::{
    audio::{AudioIoConfiguration, AudioPortDescriptor},
    automation::ParameterEvents,
    buffer::ChannelBuffer,
    events::{EventPortIndex, NormalizedValue, NoteAddress, NoteEvent, NoteEventKind, NoteEvents},
    process::{
        ActivationConfig, ProcessBlock, ProcessBufferSource, ProcessConfig, ProcessContext,
        ProcessMode, TransportSnapshot,
    },
    runtime::{Component, InstanceRuntime, Process, Processor},
};

struct NoteProbe {
    observed_note_ons: Arc<AtomicU32>,
}

impl Component for NoteProbe {
    type Processor = NoteProbeProcessor;
    type ActivationError = Infallible;

    fn audio_ports(&self) -> &[AudioPortDescriptor] {
        &[]
    }

    fn activate(
        &self,
        _config: &ActivationConfig<'_>,
    ) -> Result<Self::Processor, Self::ActivationError> {
        Ok(NoteProbeProcessor {
            observed_note_ons: Arc::clone(&self.observed_note_ons),
        })
    }
}

struct NoteProbeProcessor {
    observed_note_ons: Arc<AtomicU32>,
}

impl Processor for NoteProbeProcessor {}

impl Process<f32> for NoteProbeProcessor {
    fn process<B>(&mut self, block: &mut ProcessBlock<'_, '_, '_, f32, B>)
    where
        B: ProcessBufferSource<f32> + ?Sized,
    {
        let note_ons = block
            .note_events()
            .iter()
            .filter(|event| matches!(event.kind(), NoteEventKind::On { .. }))
            .count();
        self.observed_note_ons.store(
            u32::try_from(note_ons).expect("event bound fits u32"),
            Ordering::Release,
        );
    }
}

fn process_config() -> ProcessConfig {
    ProcessConfig::new(
        48_000.0,
        NonZeroU32::new(1),
        NonZeroU32::new(64).expect("maximum is non-zero"),
        0,
    )
    .expect("note probe process configuration is valid")
    .with_max_note_events(8)
}

fn note_address(key: u8) -> NoteAddress {
    NoteAddress::new(
        EventPortIndex::new(0),
        None,
        None,
        Some(chassis_core::events::NoteKey::new(key).expect("test key is valid")),
    )
}

fn velocity(value: f64) -> NormalizedValue {
    NormalizedValue::new(value).expect("test velocity is normalized")
}

#[test]
fn zero_audio_processor_receives_bounded_semantic_note_events() {
    let observed_note_ons = Arc::new(AtomicU32::new(0));
    let component = NoteProbe {
        observed_note_ons: Arc::clone(&observed_note_ons),
    };
    let mut runtime: InstanceRuntime<NoteProbeProcessor> =
        InstanceRuntime::for_component(&component).expect("note probe schema is valid");
    runtime
        .activate(&component, process_config(), AudioIoConfiguration::new(&[]))
        .expect("zero-audio note probe activates");

    let raw_notes = [
        NoteEvent::new(
            2,
            NoteEventKind::On {
                address: note_address(60),
                velocity: velocity(0.8),
            },
        ),
        NoteEvent::new(
            9,
            NoteEventKind::On {
                address: note_address(64),
                velocity: velocity(0.7),
            },
        ),
        NoteEvent::new(
            31,
            NoteEventKind::Off {
                address: note_address(60),
                velocity: velocity(0.2),
            },
        ),
    ];
    let note_events = NoteEvents::new(&raw_notes, 32, 8).expect("test note events are valid");
    let context = ProcessContext::with_note_events(
        ProcessMode::Realtime,
        TransportSnapshot::unknown(),
        ParameterEvents::empty_for_block(32),
        note_events,
    );
    let mut buffers: [ChannelBuffer<'_, f32>; 0] = [];

    runtime
        .process(32, context, &mut buffers)
        .expect("zero-audio semantic note callback succeeds");

    assert_eq!(observed_note_ons.load(Ordering::Acquire), 2);
}
