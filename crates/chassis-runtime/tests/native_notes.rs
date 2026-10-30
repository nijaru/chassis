//! Native note semantics through the public runtime, without a format adapter.

use chassis_runtime::{
    audio::{AudioIoConfiguration, AudioIoPolicy},
    automation::{ParameterEvent, ParameterEventValue, ParameterEvents},
    buffer::ChannelBuffer,
    events::{
        EventDialect, EventFlags, EventOutputError, EventPortDescriptor, EventPortDirection,
        EventPortIndex, EventPortKey, EventSink, NormalizedValue, NoteAddress, NoteEvent,
        NoteEventKind, NoteEvents, NoteExpression, NoteExpressionKind, OutputEvent,
    },
    midi::{MidiEvent, MidiEventLimits, MidiEventPortError, MidiEvents, MidiMessage},
    parameters::{ParameterDescriptor, ParameterIndex, ParameterStore},
    process::{
        ActivationConfig, ChannelBufferSlice, InputEventIndex, InputEventOrderError, InputEventRef,
        ProcessBlock, ProcessBufferSource, ProcessConfig, ProcessContext, ProcessMode,
        TransportSnapshot,
    },
    runtime::{
        Component, ComponentStateError, InstanceProcessError, InstanceRuntime, Process,
        ProcessError, Processor, TailLength,
    },
    schema::{ComponentId, ComponentSchema, ComponentSchemaError, StateSchemaVersion},
    state::{StateDocument, StateEntry, StateValue},
};
use std::{
    convert::Infallible,
    num::NonZeroU32,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};

const GAIN: ParameterIndex = ParameterIndex::new(0);
const INPUT: EventPortIndex = EventPortIndex::new(0);
const OUTPUT: EventPortIndex = EventPortIndex::new(1);

fn address(port: Option<EventPortIndex>) -> NoteAddress {
    NoteAddress::with_optional_port(port, None, None, None)
}
fn on(port: Option<EventPortIndex>, offset: u32) -> NoteEvent {
    NoteEvent::new(
        offset,
        NoteEventKind::On {
            address: address(port),
            velocity: NormalizedValue::new(0.5).unwrap(),
        },
    )
}
fn end(port: Option<EventPortIndex>, offset: u32) -> NoteEvent {
    NoteEvent::new(
        offset,
        NoteEventKind::End {
            address: address(port),
        },
    )
}
fn context<'a>(parameters: &'a [ParameterEvent], notes: &'a [NoteEvent]) -> ProcessContext<'a> {
    ProcessContext::with_note_events(
        ProcessMode::Realtime,
        TransportSnapshot::unknown(),
        ParameterEvents::new(parameters, 8, 16).unwrap(),
        NoteEvents::new(notes, 8, 16).unwrap(),
    )
}
fn config(attempts: u32) -> ProcessConfig {
    ProcessConfig::new(48_000.0, None, NonZeroU32::new(8).unwrap(), 8)
        .unwrap()
        .with_max_note_events(8)
        .with_max_output_note_events(attempts)
        .with_midi_input_limits(MidiEventLimits::new(8, 16))
        .with_midi_output_limits(MidiEventLimits::new(attempts, 4))
}

fn note_script(
    script: Vec<(NoteEvent, Result<(), EventOutputError>)>,
) -> Vec<(OutputEvent<'static>, Result<(), EventOutputError>)> {
    script
        .into_iter()
        .map(|(event, result)| (OutputEvent::Note(event), result))
        .collect()
}

#[derive(Default)]
struct Observed {
    calls: AtomicU32,
    gain_at_on: AtomicU32,
    typed_notes: AtomicU32,
    ordered_notes: AtomicU32,
    typed_midi: AtomicU32,
    ordered_midi: AtomicU32,
    parameters: AtomicU32,
    tail_queries: AtomicU32,
}
#[derive(Default)]
struct Probe {
    observed: Arc<Observed>,
    script: Vec<(OutputEvent<'static>, Result<(), EventOutputError>)>,
    fail_fast: bool,
    product_error: bool,
    input_only: bool,
}
impl Component for Probe {
    type Processor = ProbeProcessor;
    type ActivationError = Infallible;
    fn schema(&self) -> Result<ComponentSchema, ComponentSchemaError> {
        let mut ports = vec![EventPortDescriptor::new(
            EventPortKey::new("in"),
            "Input",
            EventPortDirection::Input,
            &[
                EventDialect::Notes,
                EventDialect::Midi1,
                EventDialect::Midi2,
            ],
        )];
        if !self.input_only {
            ports.extend([
                EventPortDescriptor::new(
                    EventPortKey::new("out"),
                    "Output",
                    EventPortDirection::Output,
                    &[
                        EventDialect::Notes,
                        EventDialect::Midi1,
                        EventDialect::Midi2,
                    ],
                ),
                EventPortDescriptor::new(
                    EventPortKey::new("midi"),
                    "MIDI",
                    EventPortDirection::Output,
                    &[EventDialect::Midi1],
                ),
            ]);
        }
        ComponentSchema::new(
            ComponentId::new("test.native-notes").unwrap(),
            StateSchemaVersion::new(1),
            vec![],
            AudioIoPolicy::any_structurally_valid(),
            ports,
            vec![
                ParameterDescriptor::float("gain", "Gain", 0.0, 1.0, 0.0)
                    .unwrap()
                    .with_global_modulation()
                    .unwrap(),
            ],
        )
    }
    fn default_custom_state(&self) -> Vec<StateEntry> {
        vec![StateEntry::new(
            "state/resource",
            StateValue::Boolean(false),
        )]
    }
    fn validate_state(
        &self,
        _: &ParameterStore,
        custom: &[StateEntry],
    ) -> Result<(), ComponentStateError> {
        match custom {
            [entry]
                if entry.key() == "state/resource"
                    && matches!(entry.value(), StateValue::Boolean(_)) =>
            {
                Ok(())
            }
            _ => Err(ComponentStateError::UnsupportedCustomState),
        }
    }
    fn activate(
        &self,
        _: &ActivationConfig<'_>,
        _: &ParameterStore,
        _: &[StateEntry],
    ) -> Result<ProbeProcessor, Infallible> {
        Ok(ProbeProcessor {
            probe: Probe {
                observed: Arc::clone(&self.observed),
                script: self.script.clone(),
                fail_fast: self.fail_fast,
                product_error: self.product_error,
                input_only: self.input_only,
            },
            processed: false,
        })
    }
}
struct ProbeProcessor {
    probe: Probe,
    processed: bool,
}
impl Processor for ProbeProcessor {
    fn tail(&self) -> TailLength {
        self.probe
            .observed
            .tail_queries
            .fetch_add(1, Ordering::Relaxed);
        TailLength::FiniteSamples(if self.processed { 2 } else { 1 })
    }
}
impl Process<f32> for ProbeProcessor {
    fn process<B: ProcessBufferSource<f32> + ?Sized>(
        &mut self,
        block: &mut ProcessBlock<'_, '_, '_, f32, B>,
    ) -> Result<(), ProcessError> {
        let observed = &self.probe.observed;
        observed.calls.fetch_add(1, Ordering::Relaxed);
        observed.typed_notes.store(
            u32::try_from(block.note_events().len()).unwrap(),
            Ordering::Relaxed,
        );
        observed.typed_midi.store(
            u32::try_from(block.midi_events().len()).unwrap(),
            Ordering::Relaxed,
        );
        observed.parameters.store(
            u32::try_from(block.parameter_events().len()).unwrap(),
            Ordering::Relaxed,
        );
        let mut gain = block.parameters().get_event_value(GAIN).unwrap();
        if let Some(events) = block.ordered_events() {
            for event in events {
                match event {
                    InputEventRef::Parameter(event) => {
                        if let chassis_runtime::automation::ParameterEventChange::Set(value) =
                            event.change()
                        {
                            gain = value;
                        }
                    }
                    InputEventRef::Midi(_) => {
                        observed.ordered_midi.fetch_add(1, Ordering::Relaxed);
                    }
                    InputEventRef::Note(event) => {
                        observed.ordered_notes.fetch_add(1, Ordering::Relaxed);
                        if matches!(event.kind(), NoteEventKind::On { .. }) {
                            observed.gain_at_on.store(
                                u32::from(gain == ParameterEventValue::Float(1.0)),
                                Ordering::Relaxed,
                            );
                        }
                    }
                }
            }
        }
        self.processed = true;
        for &(event, expected) in &self.probe.script {
            let result = match event {
                OutputEvent::Note(event) => block.try_push_note_event(event),
                OutputEvent::Midi(event) => block.try_push_midi_event(event),
            };
            if self.probe.fail_fast {
                result?;
            } else {
                assert_eq!(result, expected);
            }
        }
        if self.probe.product_error {
            return Err(ProcessError::Message("injected DSP failure"));
        }
        Ok(())
    }
}
fn active(probe: &Probe, attempts: u32) -> InstanceRuntime<ProbeProcessor> {
    let mut runtime = InstanceRuntime::for_component(probe).unwrap();
    runtime
        .activate(probe, config(attempts), AudioIoConfiguration::new(&[]))
        .unwrap();
    runtime
}
fn render(
    runtime: &mut InstanceRuntime<ProbeProcessor>,
    context: ProcessContext<'_>,
    sink: Option<&mut dyn EventSink>,
) -> Result<(), InstanceProcessError> {
    let mut buffers: [ChannelBuffer<'_, f32>; 0] = [];
    let mut source = ChannelBufferSlice::new(&mut buffers);
    match sink {
        Some(sink) => runtime.process_source_with_output(8, context, &mut source, sink),
        None => runtime.process_source(8, context, &mut source),
    }
}
#[derive(Default)]
struct Sink {
    accepted: Vec<NoteEvent>,
    reject: bool,
    attempts: u32,
}
impl EventSink for Sink {
    fn try_push(&mut self, event: OutputEvent<'_>) -> Result<(), EventOutputError> {
        let OutputEvent::Note(event) = event else {
            panic!("native note fixture")
        };
        self.attempts += 1;
        if self.reject {
            return Err(EventOutputError::Rejected);
        }
        // Capacity is prepared by each test, never grown on the callback.
        assert!(self.accepted.len() < self.accepted.capacity());
        self.accepted.push(event);
        Ok(())
    }
}

#[test]
fn source_order_distinguishes_parameter_then_note_from_note_then_parameter_at_same_time() {
    use InputEventIndex::{Note, Parameter};
    let parameters = [ParameterEvent::set(
        2,
        GAIN,
        ParameterEventValue::Float(1.0),
    )];
    let notes = [on(Some(INPUT), 2)];
    for (order, expected) in [([Parameter(0), Note(0)], 1), ([Note(0), Parameter(0)], 0)] {
        let probe = Probe::default();
        let mut runtime = active(&probe, 0);
        let context = context(&parameters, &notes)
            .with_input_order(&order)
            .unwrap();
        assert_eq!(
            context
                .ordered_events()
                .unwrap()
                .map(InputEventRef::offset)
                .collect::<Vec<_>>(),
            [2, 2]
        );
        render(&mut runtime, context, None).unwrap();
        assert_eq!(probe.observed.gain_at_on.load(Ordering::Relaxed), expected);
    }
    assert!(context(&parameters, &notes).ordered_events().is_none());
    assert!(
        context(&[], &[])
            .without_parameter_events()
            .ordered_events()
            .is_none()
    );
}

#[test]
fn bad_index_duplicate_missing_family_reordering_and_decreasing_mixed_offsets_are_rejected() {
    use InputEventIndex::{Note, Parameter};
    let parameters = [
        ParameterEvent::set(1, GAIN, ParameterEventValue::Float(1.0)),
        ParameterEvent::modulation(4, GAIN, 0.5),
    ];
    let notes = [on(Some(INPUT), 3)];
    let context = context(&parameters, &notes);
    for order in [
        vec![Parameter(9), Note(0), Parameter(1)],
        vec![Parameter(0), Note(0), Parameter(0)],
        vec![Parameter(0), Note(0)],
        vec![Parameter(1), Note(0), Parameter(0)],
        vec![Note(0), Parameter(0), Parameter(1)],
        vec![Parameter(0), Parameter(1), Note(0)],
        vec![Parameter(0), Note(1), Parameter(1)],
        vec![Parameter(0), Parameter(1), Parameter(2)],
    ] {
        assert!(context.with_input_order(&order).is_err(), "{order:?}");
    }
    assert_eq!(
        context
            .with_input_order(&[Note(0), Parameter(0), Parameter(1)])
            .unwrap_err(),
        InputEventOrderError::NonMonotonicOffsets
    );
    assert!(
        context
            .with_input_order(&[Parameter(0), Note(0), Parameter(1)])
            .is_ok()
    );
    let mismatched = ProcessContext::with_note_events(
        ProcessMode::Realtime,
        TransportSnapshot::unknown(),
        ParameterEvents::empty_for_block(4),
        NoteEvents::empty_for_block(8),
    );
    assert_eq!(
        mismatched.with_input_order(&[]).unwrap_err(),
        InputEventOrderError::FrameCountMismatch
    );
}

#[test]
fn custom_resource_gate_withholds_parameters_but_preserves_typed_and_ordered_notes_and_midi() {
    use InputEventIndex::{Midi, Note, Parameter};
    let probe = Probe::default();
    let mut runtime = active(&probe, 0);
    // Complete replacement, with a custom-resource change that activates the gate.
    let mut replacement = StateDocument::new("test.native-notes", 1).unwrap();
    replacement
        .insert(StateEntry::new("parameter/gain", StateValue::Float(0.75)))
        .unwrap();
    replacement
        .insert(StateEntry::new("state/resource", StateValue::Boolean(true)))
        .unwrap();
    runtime.apply_state(&probe, &replacement).unwrap();
    let parameters = [ParameterEvent::set(
        0,
        GAIN,
        ParameterEventValue::Float(1.0),
    )];
    let payload = [1, 0xff, 3];
    let midi = [
        MidiEvent::new(0, INPUT, MidiMessage::SysEx(&payload)),
        MidiEvent::new(3, INPUT, MidiMessage::Ump([0x4090_3c00, u32::MAX, 2, 3])),
    ];
    let notes = [on(Some(INPUT), 0), end(Some(INPUT), 3)];
    let order = [Midi(0), Parameter(0), Note(0), Midi(1), Note(1)];
    let context = ProcessContext::with_events(
        ProcessMode::Realtime,
        TransportSnapshot::unknown(),
        ParameterEvents::new(&parameters, 8, 8).unwrap(),
        NoteEvents::new(&notes, 8, 8).unwrap(),
        MidiEvents::new(&midi, 8, 8, 16).unwrap(),
    )
    .with_input_order(&order)
    .unwrap();
    let hidden = context.without_parameter_events();
    assert!(hidden.parameter_events().is_empty());
    assert_eq!(
        hidden.ordered_events().unwrap().collect::<Vec<_>>(),
        [
            InputEventRef::Midi(midi[0]),
            InputEventRef::Note(&notes[0]),
            InputEventRef::Midi(midi[1]),
            InputEventRef::Note(&notes[1])
        ]
    );
    render(&mut runtime, context, None).unwrap();
    assert_eq!(probe.observed.parameters.load(Ordering::Relaxed), 0);
    assert_eq!(probe.observed.typed_notes.load(Ordering::Relaxed), 2);
    assert_eq!(probe.observed.ordered_notes.load(Ordering::Relaxed), 2);
    assert_eq!(probe.observed.typed_midi.load(Ordering::Relaxed), 2);
    assert_eq!(probe.observed.ordered_midi.load(Ordering::Relaxed), 2);
    assert_eq!(
        hidden.midi_events().iter().next().unwrap().message(),
        MidiMessage::SysEx(&payload)
    );
    assert!(
        matches!(hidden.midi_events().iter().next().unwrap().message(),
        MidiMessage::SysEx(bytes) if core::ptr::eq(bytes.as_ptr(), payload.as_ptr()))
    );
    assert_eq!(probe.observed.gain_at_on.load(Ordering::Relaxed), 0);
    assert_eq!(runtime.state_document().unwrap(), replacement);

    // Withholding never bypasses original count or schema validation before DSP.
    for parameters in [
        vec![ParameterEvent::modulation(0, GAIN, 0.0); 9],
        vec![ParameterEvent::modulation(0, ParameterIndex::new(99), 0.0)],
    ] {
        assert!(
            render(
                &mut runtime,
                self::context(&parameters, &[]).without_parameter_events(),
                None
            )
            .is_err()
        );
    }
    assert_eq!(probe.observed.calls.load(Ordering::Relaxed), 1);
}

#[test]
fn expression_domains_are_finite_and_values_are_held_sets_with_full_wildcards() {
    use NoteExpressionKind::*;
    for (kind, low, high) in [
        (Volume, f64::MIN_POSITIVE, 4.0),
        (Tuning, -120.0, 120.0),
        (Pan, 0.0, 1.0),
        (Vibrato, 0.0, 1.0),
        (Expression, 0.0, 1.0),
        (Brightness, 0.0, 1.0),
        (Pressure, 0.0, 1.0),
    ] {
        for value in [low, high] {
            let expression = NoteExpression::new(kind, value).unwrap();
            assert_eq!(expression.kind(), kind);
            assert_eq!(expression.value().to_bits(), value.to_bits());
        }
        for invalid in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            high + 0.01,
            if kind == Volume { 0.0 } else { low - 0.01 },
        ] {
            assert!(
                NoteExpression::new(kind, invalid).is_none(),
                "{kind:?}: {invalid}"
            );
        }
    }
    let target = address(None);
    let expressions = [0.0, 1.0, 0.5].map(|value| {
        NoteEvent::new(
            0,
            NoteEventKind::Expression {
                address: target,
                expression: NoteExpression::new(Pan, value).unwrap(),
            },
        )
    });
    let mut held_pan = 0.5;
    for event in NoteEvents::new(&expressions, 8, 3).unwrap().iter() {
        let NoteEventKind::Expression {
            address,
            expression,
        } = event.kind()
        else {
            unreachable!()
        };
        assert_eq!(address, target);
        held_pan = expression.value();
    }
    assert_eq!(held_pan.to_bits(), 0.5_f64.to_bits());
}

#[test]
fn output_validates_direction_dialect_offset_order_and_attempt_budget() {
    use EventOutputError::*;
    let valid = on(Some(OUTPUT), 2);
    let input_end = end(Some(INPUT), 3);
    let expression = NoteEvent::new(
        3,
        NoteEventKind::Expression {
            address: address(Some(OUTPUT)),
            expression: NoteExpression::new(NoteExpressionKind::Volume, 2.0).unwrap(),
        },
    );
    let script = vec![
        (on(Some(INPUT), 0), Err(TargetRejected)),
        (end(Some(OUTPUT), 0), Err(TargetRejected)),
        (on(Some(EventPortIndex::new(2)), 0), Err(TargetRejected)),
        (on(Some(EventPortIndex::new(99)), 0), Err(TargetRejected)),
        (on(Some(OUTPUT), 8), Err(OffsetOutOfRange)),
        (valid, Ok(())),
        (on(Some(OUTPUT), 1), Err(NonMonotonicOffsets)),
        (input_end, Ok(())),
        (expression, Ok(())),
        (on(None, 3), Ok(())),
        (end(None, 3), Ok(())),
        (valid, Err(LimitExceeded)),
    ];
    let probe = Probe {
        script: note_script(script),
        ..Probe::default()
    };
    let mut runtime = active(&probe, 11);
    let mut sink = Sink {
        accepted: Vec::with_capacity(5),
        ..Sink::default()
    };
    render(&mut runtime, context(&[], &[]), Some(&mut sink)).unwrap();
    assert_eq!(
        sink.accepted,
        [valid, input_end, expression, on(None, 3), end(None, 3)]
    );
    assert_eq!(sink.attempts, 5);
    // A new block resets both offset and write-attempt bounds.
    sink.accepted.clear();
    render(&mut runtime, context(&[], &[]), Some(&mut sink)).unwrap();
    assert_eq!(sink.attempts, 10);
}

#[test]
fn end_feedback_needs_only_an_input_port_and_missing_sink_is_explicit() {
    let event = end(Some(INPUT), 0);
    let probe = Probe {
        script: note_script(vec![
            (on(None, 0), Err(EventOutputError::TargetRejected)),
            (event, Ok(())),
        ]),
        input_only: true,
        ..Probe::default()
    };
    let mut runtime = active(&probe, 2);
    let mut sink = Sink {
        accepted: Vec::with_capacity(1),
        ..Sink::default()
    };
    render(&mut runtime, context(&[], &[]), Some(&mut sink)).unwrap();
    assert_eq!(sink.accepted, [event]);
    for (attempts, expected) in [
        (1, EventOutputError::NoSink),
        (0, EventOutputError::LimitExceeded),
    ] {
        let probe = Probe {
            script: note_script(vec![
                (event, Err(expected)),
                (event, Err(EventOutputError::LimitExceeded)),
            ]),
            ..Probe::default()
        };
        render(&mut active(&probe, attempts), context(&[], &[]), None).unwrap();
    }
    assert_eq!(
        ProcessConfig::new(48_000.0, None, NonZeroU32::new(8).unwrap(), 0)
            .unwrap()
            .max_output_note_events(),
        0,
    );
}

#[test]
fn sink_backpressure_consumes_attempt_budget_and_product_owns_bounded_retry() {
    let event = on(Some(OUTPUT), 0);
    let probe = Probe {
        script: note_script(vec![
            (event, Err(EventOutputError::Rejected)),
            (event, Err(EventOutputError::Rejected)),
            (event, Err(EventOutputError::LimitExceeded)),
        ]),
        ..Probe::default()
    };
    let mut sink = Sink {
        reject: true,
        ..Sink::default()
    };
    render(&mut active(&probe, 2), context(&[], &[]), Some(&mut sink)).unwrap();
    assert_eq!(sink.attempts, 2);
}

fn midi_context(events: MidiEvents<'_>) -> ProcessContext<'_> {
    ProcessContext::with_events(
        ProcessMode::Realtime,
        TransportSnapshot::unknown(),
        ParameterEvents::empty_for_block(8),
        NoteEvents::empty_for_block(8),
        events,
    )
}

#[test]
fn three_family_order_requires_exact_coverage_family_order_and_monotonic_times() {
    use InputEventIndex::{Midi, Note, Parameter};
    let parameters = [ParameterEvent::set(
        1,
        GAIN,
        ParameterEventValue::Float(1.0),
    )];
    let notes = [on(Some(INPUT), 1)];
    let midi = [
        MidiEvent::new(1, INPUT, MidiMessage::SysEx(&[1, 2])),
        MidiEvent::new(2, INPUT, MidiMessage::Ump([1, 2, 3, u32::MAX])),
    ];
    let context = ProcessContext::with_events(
        ProcessMode::Realtime,
        TransportSnapshot::unknown(),
        ParameterEvents::new(&parameters, 8, 1).unwrap(),
        NoteEvents::new(&notes, 8, 1).unwrap(),
        MidiEvents::new(&midi, 8, 2, 2).unwrap(),
    );
    assert!(context.ordered_events().is_none());
    let order = [Midi(0), Note(0), Parameter(0), Midi(1)];
    assert_eq!(
        context
            .with_input_order(&order)
            .unwrap()
            .ordered_events()
            .unwrap()
            .collect::<Vec<_>>(),
        [
            InputEventRef::Midi(midi[0]),
            InputEventRef::Note(&notes[0]),
            InputEventRef::Parameter(&parameters[0]),
            InputEventRef::Midi(midi[1])
        ]
    );
    for order in [
        vec![Midi(0), Note(0), Parameter(0)],               // missing
        vec![Midi(0), Note(0), Parameter(0), Midi(0)],      // duplicate
        vec![Midi(1), Note(0), Parameter(0), Midi(0)],      // family reorder
        vec![Midi(0), Midi(1), Note(0), Parameter(0)],      // decreasing mixed time
        vec![Midi(0), Note(0), Parameter(0), Midi(99)],     // unknown
        vec![Midi(0), Parameter(0), Parameter(1), Midi(1)], // note omitted
    ] {
        assert!(
            context
                .without_parameter_events()
                .with_input_order(&order)
                .is_err()
        );
    }
    assert_eq!(
        midi_context(MidiEvents::empty_for_block(7))
            .with_input_order(&[])
            .unwrap_err(),
        InputEventOrderError::FrameCountMismatch
    );
}

#[test]
fn activation_rechecks_actual_midi_counts_bytes_frames_and_ports_before_dsp() {
    use chassis_runtime::process::ProcessBlockError;
    let probe = Probe::default();
    let mut runtime = active(&probe, 0);
    let count = [MidiEvent::new(0, INPUT, MidiMessage::Short([0; 3])); 9];
    let bytes = [MidiEvent::new(0, INPUT, MidiMessage::SysEx(&[0; 17]))];
    for (events, expected) in [
        (
            MidiEvents::new(&count, 8, 9, 0).unwrap(),
            ProcessBlockError::MidiEventCountTooLarge {
                actual: 9,
                maximum: 8,
            },
        ),
        (
            MidiEvents::new(&bytes, 8, 1, 17).unwrap(),
            ProcessBlockError::MidiSysExBytesTooLarge {
                actual: 17,
                maximum: 16,
            },
        ),
        (
            MidiEvents::empty_for_block(7),
            ProcessBlockError::MidiEventFrameCountMismatch {
                expected: 8,
                actual: 7,
            },
        ),
    ] {
        assert_eq!(
            render(
                &mut runtime,
                midi_context(events).without_parameter_events(),
                None
            ),
            Err(InstanceProcessError::InvalidBlock(expected))
        );
    }
    for (port, expected) in [
        (OUTPUT, MidiEventPortError::PortIsNotInput(OUTPUT)),
        (
            EventPortIndex::new(99),
            MidiEventPortError::UnknownPort(EventPortIndex::new(99)),
        ),
    ] {
        let events = [MidiEvent::new(0, port, MidiMessage::SysEx(&[]))];
        assert_eq!(
            render(
                &mut runtime,
                midi_context(MidiEvents::new(&events, 8, 1, 0).unwrap()),
                None
            ),
            Err(InstanceProcessError::InvalidMidiEvents(expected))
        );
    }
    assert_eq!(probe.observed.calls.load(Ordering::Relaxed), 0);
}

#[derive(Default)]
struct MixedSink {
    delivered: [Option<(u32, bool)>; 8],
    count: usize,
    attempts: usize,
    reject_first: bool,
    expected_midi: Option<MidiEvent<'static>>,
}
impl EventSink for MixedSink {
    fn try_push(&mut self, event: OutputEvent<'_>) -> Result<(), EventOutputError> {
        self.attempts += 1;
        if self.reject_first && self.attempts == 1 {
            return Err(EventOutputError::Rejected);
        }
        if let (OutputEvent::Midi(event), Some(expected)) = (event, self.expected_midi) {
            assert_eq!(event, expected);
        }
        self.delivered[self.count] = Some((event.offset(), matches!(event, OutputEvent::Midi(_))));
        self.count += 1;
        Ok(())
    }
}

#[test]
fn output_shares_one_order_across_families_in_both_directions_and_preserves_opaque_payloads() {
    use EventOutputError::*;
    for message in [
        MidiMessage::Short([0x90, 0xff, 0]),
        MidiMessage::SysEx(&[]),
        MidiMessage::SysEx(&[1, 0xff, 3, 4]),
        MidiMessage::Ump([0x2090_3c00, u32::MAX, 0x1234_5678, 0x90ab_cdef]),
    ] {
        let midi = MidiEvent::new(3, OUTPUT, message).with_flags(EventFlags::new(true, true));
        let probe = Probe {
            script: vec![
                (OutputEvent::Note(on(Some(OUTPUT), 2)), Ok(())),
                (
                    OutputEvent::Midi(MidiEvent::new(1, OUTPUT, MidiMessage::Short([0; 3]))),
                    Err(NonMonotonicOffsets),
                ),
                (OutputEvent::Midi(midi), Ok(())),
                (
                    OutputEvent::Note(on(Some(OUTPUT), 1)),
                    Err(NonMonotonicOffsets),
                ),
            ],
            ..Probe::default()
        };
        let mut sink = MixedSink {
            expected_midi: Some(midi),
            ..MixedSink::default()
        };
        render(&mut active(&probe, 4), context(&[], &[]), Some(&mut sink)).unwrap();
        assert_eq!(
            &sink.delivered[..sink.count],
            &[Some((2, false)), Some((3, true))]
        );
    }
    // A successful MIDI write establishes ordering for a subsequent note write.
    let midi = MidiEvent::new(3, OUTPUT, MidiMessage::SysEx(&[1, 0xff, 3, 4]))
        .with_flags(EventFlags::new(true, true));
    let probe = Probe {
        script: vec![
            (OutputEvent::Midi(midi), Ok(())),
            (
                OutputEvent::Note(on(Some(OUTPUT), 2)),
                Err(NonMonotonicOffsets),
            ),
            (OutputEvent::Note(on(Some(OUTPUT), 3)), Ok(())),
        ],
        ..Probe::default()
    };
    let mut sink = MixedSink {
        expected_midi: Some(midi),
        ..MixedSink::default()
    };
    render(&mut active(&probe, 3), context(&[], &[]), Some(&mut sink)).unwrap();
    assert_eq!(
        &sink.delivered[..sink.count],
        &[Some((3, true)), Some((3, false))]
    );
}

#[test]
fn midi_output_checks_the_messages_dialect_without_conversion() {
    let probe = Probe {
        script: vec![
            (
                OutputEvent::Midi(MidiEvent::new(
                    0,
                    EventPortIndex::new(2),
                    MidiMessage::Ump([0; 4]),
                )),
                Err(EventOutputError::TargetRejected),
            ), // MIDI 1-only output
            (
                OutputEvent::Midi(MidiEvent::new(0, INPUT, MidiMessage::Ump([0; 4]))),
                Err(EventOutputError::TargetRejected),
            ), // correct dialect, wrong direction
            (
                OutputEvent::Midi(MidiEvent::new(0, OUTPUT, MidiMessage::Ump([u32::MAX; 4]))),
                Ok(()),
            ),
        ],
        ..Probe::default()
    };
    let mut sink = MixedSink::default();
    render(&mut active(&probe, 3), context(&[], &[]), Some(&mut sink)).unwrap();
    assert_eq!(sink.count, 1);
}

#[test]
fn midi_attempts_include_invalid_and_byte_rejections_backpressure_does_not_refund_bytes() {
    use EventOutputError::*;
    let short = MidiMessage::Short([0x01, 0xff, 0x90]);
    let out = EventPortIndex::new(2);
    let probe = Probe {
        script: vec![
            (
                OutputEvent::Midi(MidiEvent::new(8, out, short)),
                Err(OffsetOutOfRange),
            ),
            (
                OutputEvent::Midi(MidiEvent::new(0, EventPortIndex::new(99), short)),
                Err(TargetRejected),
            ),
            (
                OutputEvent::Midi(MidiEvent::new(0, INPUT, short)),
                Err(TargetRejected),
            ),
            (
                OutputEvent::Midi(MidiEvent::new(7, out, MidiMessage::SysEx(&[1, 2, 3, 4]))),
                Err(Rejected),
            ),
            // Rejected offset 7 did not advance the shared ordering cursor.
            (
                OutputEvent::Midi(MidiEvent::new(0, out, MidiMessage::SysEx(&[]))),
                Ok(()),
            ),
            (
                OutputEvent::Midi(MidiEvent::new(0, out, MidiMessage::SysEx(&[1]))),
                Err(SysExBytesExceeded),
            ),
            (OutputEvent::Midi(MidiEvent::new(0, out, short)), Ok(())),
            (
                OutputEvent::Midi(MidiEvent::new(0, out, short)),
                Err(LimitExceeded),
            ),
            // MIDI attempts do not consume the note budget.
            (OutputEvent::Note(on(Some(OUTPUT), 0)), Ok(())),
        ],
        ..Probe::default()
    };
    let mut sink = MixedSink {
        reject_first: true,
        ..MixedSink::default()
    };
    render(&mut active(&probe, 7), context(&[], &[]), Some(&mut sink)).unwrap();
    assert_eq!(sink.attempts, 4);
    assert_eq!(sink.count, 3);
    for (attempts, expected) in [(0, LimitExceeded), (1, NoSink)] {
        let probe = Probe {
            script: vec![
                (
                    OutputEvent::Midi(MidiEvent::new(0, out, short)),
                    Err(expected),
                ),
                (
                    OutputEvent::Midi(MidiEvent::new(0, out, short)),
                    Err(LimitExceeded),
                ),
            ],
            ..Probe::default()
        };
        render(&mut active(&probe, attempts), context(&[], &[]), None).unwrap();
    }
}

#[test]
fn product_error_retains_neither_tail_nor_base_or_modulation_endpoints() {
    for failure in 0..4 {
        let essential_output = failure != 0;
        let probe = Probe {
            script: match failure {
                1 => note_script(vec![(end(Some(INPUT), 0), Ok(()))]),
                2 => vec![(
                    OutputEvent::Midi(MidiEvent::new(0, OUTPUT, MidiMessage::SysEx(&[1, 2, 3, 4]))),
                    Ok(()),
                )],
                3 => vec![(
                    OutputEvent::Midi(MidiEvent::new(0, OUTPUT, MidiMessage::Ump([0; 4]))),
                    Ok(()),
                )],
                _ => vec![],
            },
            fail_fast: essential_output,
            product_error: !essential_output,
            ..Probe::default()
        };
        let mut runtime = active(&probe, 1);
        let before = runtime.state_document().unwrap();
        let parameters = [
            ParameterEvent::set(0, GAIN, ParameterEventValue::Float(1.0)),
            ParameterEvent::modulation(0, GAIN, 0.5),
        ];
        let mut sink = MixedSink {
            reject_first: true,
            ..MixedSink::default()
        };
        let expected = if essential_output {
            ProcessError::EventOutput(EventOutputError::Rejected)
        } else {
            ProcessError::Message("injected DSP failure")
        };
        assert_eq!(
            render(&mut runtime, context(&parameters, &[]), Some(&mut sink)),
            Err(InstanceProcessError::Processor(expected))
        );
        assert_eq!(runtime.state_document().unwrap(), before);
        assert_eq!(runtime.modulation(GAIN), Some(0.0));
        assert_eq!(runtime.active_tail(), Some(TailLength::FiniteSamples(1)));
        assert_eq!(
            probe.observed.tail_queries.load(Ordering::Relaxed),
            1,
            "no post-error tail query"
        );
    }
}
