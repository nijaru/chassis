//! Deterministic monophonic native-note qualification instrument, not a product synth.

use chassis_clap::{
    ClapAudioPort, ClapComponent, ClapNotePort, F32AndF64, SingleComponentEntry, clack_export_entry,
};
use chassis_runtime::{
    audio::{
        AudioIoConfigurationSpec, AudioIoPolicy, AudioPortDescriptor, AudioPortIndex,
        ChannelLayout, ConfiguredAudioPort, PortDirection, PortKey, PortRole,
    },
    automation::{ParameterEventChange, ParameterEventValue},
    events::{
        EventDialect, EventPortDescriptor, EventPortDirection, EventPortKey, NoteAddress,
        NoteEvent, NoteEventKind, NoteExpression, NoteExpressionKind,
    },
    parameters::{ParameterDescriptor, ParameterIndex, ParameterStore},
    process::{ActivationConfig, InputEventRef, ProcessBlock},
    runtime::{Component, Process, ProcessError, Processor, TailLength},
    schema::{ComponentId, ComponentSchema, ComponentSchemaError, StateSchemaVersion},
    state::StateEntry,
};
use core::{
    convert::Infallible,
    f64::consts::{FRAC_PI_2, TAU},
};

const OUTPUT: PortKey = PortKey::new("audio.out");
const PRIMARY: EventPortKey = EventPortKey::new("notes.primary");
const SECONDARY: EventPortKey = EventPortKey::new("notes.secondary");
const MAX_NOTES: u32 = 1024;

/// Exported qualification component with deliberately nonconventional topology.
#[derive(Default)]
pub struct NoteProbe;

impl Component for NoteProbe {
    type Processor = NoteProbeProcessor;
    type ActivationError = Infallible;

    fn schema(&self) -> Result<ComponentSchema, ComponentSchemaError> {
        ComponentSchema::new(
            ComponentId::new("org.nijaru.chassis.note-probe").expect("static semantic ID"),
            StateSchemaVersion::new(1),
            vec![AudioPortDescriptor::new(
                OUTPUT,
                "Main Output",
                PortDirection::Output,
                PortRole::Main,
                false,
            )],
            AudioIoPolicy::enumerated(vec![AudioIoConfigurationSpec::new(vec![
                ConfiguredAudioPort::new(OUTPUT, ChannelLayout::Stereo),
            ])]),
            vec![
                EventPortDescriptor::new(
                    PRIMARY,
                    "Primary Notes",
                    EventPortDirection::Input,
                    &[EventDialect::Notes],
                ),
                EventPortDescriptor::new(
                    SECONDARY,
                    "Secondary Notes",
                    EventPortDirection::Input,
                    &[EventDialect::Notes],
                ),
            ],
            vec![
                ParameterDescriptor::float("strike", "Strike", 0.0, 1.0, 1.0)
                    .expect("static float domain")
                    .with_global_modulation()
                    .expect("strike is float"),
            ],
        )
    }

    fn activate(
        &self,
        config: &ActivationConfig<'_>,
        parameters: &ParameterStore,
        _custom: &[StateEntry],
    ) -> Result<Self::Processor, Self::ActivationError> {
        Ok(NoteProbeProcessor {
            output: config
                .schema()
                .audio_port_index(&OUTPUT)
                .expect("declared output"),
            strike: parameters.index("strike").expect("declared strike"),
            sample_rate: config.process().sample_rate(),
            audio: vec![
                [0.0; 2];
                usize::try_from(config.process().max_frames().get())
                    .expect("frame bound fits platform")
            ],
            ended: Vec::with_capacity(usize::try_from(MAX_NOTES).expect("constant fits") + 1),
            boundary_expressions: Vec::with_capacity(
                usize::try_from(MAX_NOTES).expect("constant fits"),
            ),
            voice: None,
        })
    }
}

impl ClapComponent for NoteProbe {
    const CLAP_DESCRIPTOR: chassis_clap::ClapDescriptor = chassis_clap::ClapDescriptor {
        features: &[c"instrument", c"stereo"],
        vendor: "Nijaru",
        version: env!("CARGO_PKG_VERSION"),
        description: "Instrument probe for Chassis native note and expression qualification",
        ..chassis_clap::ClapDescriptor::new("com.nijaru.chassis.note-probe", "Chassis Note Probe")
    };
    const CLAP_AUDIO_PORTS: &'static [ClapAudioPort] =
        &[ClapAudioPort::new(OUTPUT, 71, ChannelLayout::Stereo, None)];
    // Deliberately reverse declaration order: backend and schema indices differ.
    const CLAP_NOTE_PORTS: &'static [ClapNotePort] = &[
        ClapNotePort::new(SECONDARY, 10, EventDialect::Notes),
        ClapNotePort::new(PRIMARY, 90, EventDialect::Notes),
    ];
    const CLAP_PARAMETER_IDS: &'static [(&'static str, u32)] = &[("strike", 7007)];
    const CLAP_MAX_PARAMETER_EVENTS: u32 = 512;
    const CLAP_MAX_NOTE_EVENTS: u32 = MAX_NOTES;
    const CLAP_MAX_OUTPUT_NOTE_EVENTS: u32 = MAX_NOTES + 1;
}

struct Voice {
    address: NoteAddress,
    level: f64,
    phase: f64,
    vibrato_phase: f64,
    volume: f64,
    pan: f64,
    tuning: f64,
    vibrato: f64,
    expression: f64,
    brightness: f64,
    pressure: f64,
}

impl Voice {
    fn expression(&mut self, expression: NoteExpression) {
        let value = expression.value();
        match expression.kind() {
            NoteExpressionKind::Volume => self.volume = value,
            NoteExpressionKind::Pan => self.pan = value,
            NoteExpressionKind::Tuning => self.tuning = value,
            NoteExpressionKind::Vibrato => self.vibrato = value,
            NoteExpressionKind::Expression => self.expression = value,
            NoteExpressionKind::Brightness => self.brightness = value,
            NoteExpressionKind::Pressure => self.pressure = value,
        }
    }

    fn sample(&mut self, sample_rate: f64) -> [f64; 2] {
        let key = self
            .address
            .key()
            .expect("native note-on key is concrete")
            .get();
        let pitch = f64::from(key) + self.tuning + self.vibrato * self.vibrato_phase.sin();
        let frequency = (440.0 * ((pitch - 69.0) / 12.0).exp2()).min(sample_rate * 0.49);
        let wave =
            (1.0 - self.brightness) * self.phase.sin() + self.brightness * (2.0 * self.phase).sin();
        let value =
            0.1 * self.level * self.volume * self.expression * (0.5 + 0.5 * self.pressure) * wave;
        self.phase = (self.phase + (frequency / sample_rate) * TAU).rem_euclid(TAU);
        self.vibrato_phase = (self.vibrato_phase
            + (6.0_f64.min(sample_rate * 0.49) / sample_rate) * TAU)
            .rem_euclid(TAU);
        [value * (1.0 - self.pan), value * self.pan]
    }
}

/// Active monophonic resources; all scratch is prepared at activation.
pub struct NoteProbeProcessor {
    output: AudioPortIndex,
    strike: ParameterIndex,
    sample_rate: f64,
    audio: Vec<[f64; 2]>,
    ended: Vec<NoteAddress>,
    boundary_expressions: Vec<(NoteAddress, NoteExpression)>,
    voice: Option<Voice>,
}

fn matches(target: NoteAddress, voice: NoteAddress) -> bool {
    target.port().is_none_or(|v| voice.port() == Some(v))
        && target.channel().is_none_or(|v| voice.channel() == Some(v))
        && target.key().is_none_or(|v| voice.key() == Some(v))
        && target.note_id().is_none_or(|v| voice.note_id() == Some(v))
}

impl NoteProbeProcessor {
    fn end_voice(&mut self) -> Result<(), ProcessError> {
        if let Some(voice) = self.voice.take() {
            // Keep the broadest original tuple: a no-ID completion also
            // covers ended ID-specific voices at the same port/channel/key.
            if self
                .ended
                .iter()
                .any(|target| matches(*target, voice.address))
            {
                return Ok(());
            }
            self.ended.retain(|target| !matches(voice.address, *target));
            if self.ended.len() == self.ended.capacity() {
                return Err(ProcessError::Message(
                    "Note probe voice-end capacity exceeded",
                ));
            }
            self.ended.push(voice.address);
        }
        Ok(())
    }

    fn note(&mut self, event: NoteEvent, strike: f64) -> Result<(), ProcessError> {
        match event.kind() {
            NoteEventKind::On { address, velocity } => {
                if address.port().is_none()
                    || address.channel().is_none()
                    || address.key().is_none()
                {
                    return Err(ProcessError::Message(
                        "Note probe requires concrete note-on addressing",
                    ));
                }
                self.end_voice()?;
                let mut voice = Voice {
                    address,
                    level: strike * velocity.get(),
                    phase: FRAC_PI_2,
                    vibrato_phase: 0.0,
                    volume: 1.0,
                    pan: 0.5,
                    tuning: 0.0,
                    vibrato: 0.0,
                    expression: 1.0,
                    brightness: 0.0,
                    pressure: 0.0,
                };
                // CLAP requires expressions at the onset sample to affect all
                // generated samples, including expressions preceding the onset.
                for &(target, expression) in &self.boundary_expressions {
                    if matches(target, address) {
                        voice.expression(expression);
                    }
                }
                self.voice = Some(voice);
            }
            NoteEventKind::Off { address, .. }
            | NoteEventKind::Choke { address }
            | NoteEventKind::End { address } => {
                if self
                    .voice
                    .as_ref()
                    .is_some_and(|voice| matches(address, voice.address))
                {
                    self.end_voice()?;
                }
            }
            NoteEventKind::Expression {
                address,
                expression,
            } => {
                if self.boundary_expressions.len() == self.boundary_expressions.capacity() {
                    return Err(ProcessError::Message(
                        "Note probe expression capacity exceeded",
                    ));
                }
                self.boundary_expressions.push((address, expression));
                if let Some(voice) = &mut self.voice
                    && matches(address, voice.address)
                {
                    voice.expression(expression);
                }
            }
        }
        Ok(())
    }

    fn report_ends<S>(&mut self, block: &mut ProcessBlock<'_, S>) -> Result<(), ProcessError> {
        // NOTE_END time is ignored by CLAP. Retain feedback that would end a
        // live matching retrigger, including across blocks. Canonicalization in
        // end_voice leaves at most one such tuple for this monophonic client.
        let offset = block.frame_count().saturating_sub(1);
        let mut pending = 0;
        for index in 0..self.ended.len() {
            let address = self.ended[index];
            if self
                .voice
                .as_ref()
                .is_some_and(|voice| matches(address, voice.address))
            {
                self.ended[pending] = address;
                pending += 1;
            } else {
                block
                    .try_push_note_event(NoteEvent::new(offset, NoteEventKind::End { address }))?;
            }
        }
        self.ended.truncate(pending);
        Ok(())
    }

    fn render<S>(
        &mut self,
        block: &mut ProcessBlock<'_, S>,
        convert: impl Fn(f64) -> S,
    ) -> Result<(), ProcessError> {
        let Some(ParameterEventValue::Float(mut base)) =
            block.parameters().get_event_value(self.strike)
        else {
            return Err(ProcessError::Message("Invalid note probe strike"));
        };
        let mut amount = block
            .modulation(self.strike)
            .expect("strike has held modulation");
        // Strike is intentionally captured at onset so equal-offset mixed-list
        // ordering is observable. This exported CLAP client accepts sets, not ramps.
        let context = block.context();
        let mut events = context
            .ordered_events()
            .ok_or(ProcessError::Message(
                "Note probe requires source input order",
            ))?
            .peekable();
        for offset in 0..block.frame_count() {
            self.boundary_expressions.clear();
            while events.peek().is_some_and(|event| event.offset() == offset) {
                match events.next().expect("peeked event exists") {
                    InputEventRef::Parameter(event) => match event.change() {
                        ParameterEventChange::Set(ParameterEventValue::Float(value)) => {
                            base = value;
                        }
                        ParameterEventChange::Modulate { amount: value } => amount = value,
                        _ => {
                            return Err(ProcessError::Message("Unsupported note probe trajectory"));
                        }
                    },
                    InputEventRef::Midi(_) => {
                        return Err(ProcessError::Message(
                            "MIDI is not declared by this instrument",
                        ));
                    }
                    InputEventRef::Note(event) => {
                        self.note(*event, (base + amount).clamp(0.0, 1.0))?;
                    }
                }
            }
            self.audio[usize::try_from(offset).expect("activated offset fits")] = self
                .voice
                .as_mut()
                .map_or([0.0; 2], |voice| voice.sample(self.sample_rate));
        }
        self.report_ends(block)?;
        block.for_each_channel(|mut channel| {
            let endpoint = channel.output_endpoint().expect("output-only instrument");
            debug_assert_eq!(endpoint.port_index(), self.output);
            let index = usize::try_from(endpoint.channel()).expect("stereo index fits");
            for (sample, frame) in channel
                .output_mut()
                .expect("output channel")
                .iter_mut()
                .zip(&self.audio)
            {
                *sample = convert(frame[index]);
            }
        });
        Ok(())
    }
}

impl Processor for NoteProbeProcessor {
    fn reset(&mut self) {
        self.voice = None;
        self.ended.clear();
        self.boundary_expressions.clear();
    }
    fn tail(&self) -> TailLength {
        if self.voice.is_some() {
            TailLength::Infinite
        } else {
            TailLength::FiniteSamples(0)
        }
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "bounded audio conversion to negotiated f32"
)]
fn to_f32(value: f64) -> f32 {
    let value = value as f32;
    if value.is_subnormal() { 0.0 } else { value }
}

fn to_f64(value: f64) -> f64 {
    if value.is_subnormal() { 0.0 } else { value }
}

impl Process<f32> for NoteProbeProcessor {
    fn process(&mut self, block: &mut ProcessBlock<'_, f32>) -> Result<(), ProcessError> {
        self.render(block, to_f32)
    }
}
impl Process<f64> for NoteProbeProcessor {
    fn process(&mut self, block: &mut ProcessBlock<'_, f64>) -> Result<(), ProcessError> {
        self.render(block, to_f64)
    }
}

clack_export_entry!(SingleComponentEntry<NoteProbe, F32AndF64>);
