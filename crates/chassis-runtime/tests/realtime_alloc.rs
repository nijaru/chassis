#![allow(unsafe_code)]

//! Test-only heap instrumentation for the post-activation core process path.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    convert::Infallible,
    num::NonZeroU32,
    sync::atomic::{AtomicU64, Ordering},
};

use chassis_runtime::{
    audio::{AudioPortIndex, DEFAULT_EFFECT_CONFIGURATION},
    automation::{ParameterEvent, ParameterEventValue, ParameterEvents},
    buffer::{ChannelBuffer, InputEndpoint, OutputEndpoint},
    events::{
        EventDialect, EventFlags, EventOutputError, EventPortDescriptor, EventPortDirection,
        EventPortIndex, EventPortKey, EventSink, NormalizedValue, NoteAddress, NoteEvent,
        NoteEventKind, NoteEvents, NoteExpression, NoteExpressionKind, OutputEvent,
    },
    midi::{MidiEvent, MidiEventLimits, MidiEvents, MidiMessage},
    parameters::{
        ChoiceId, ChoiceIndex, ChoiceOption, ParameterDescriptor, ParameterEventValueError,
        ParameterIndex, ParameterValue, ParameterValueError,
    },
    process::{
        ActivationConfig, ChannelBufferSlice, InputEventIndex, InputEventRef, ProcessBlock,
        ProcessBufferSource, ProcessChannel, ProcessConfig, ProcessContext, ProcessMode,
        TransportSnapshot,
    },
    runtime::{Component, InstanceProcessError, InstanceRuntime, Process, ProcessError, Processor},
    state::{StateDocument, StateLimits},
};

#[path = "support/resource_state.rs"]
mod resource_state;

const MAIN_INPUT_INDEX: AudioPortIndex = AudioPortIndex::new(0);
const MAIN_OUTPUT_INDEX: AudioPortIndex = AudioPortIndex::new(1);

struct CountingAllocator;

thread_local! {
    static COUNT_THIS_THREAD: Cell<bool> = const { Cell::new(false) };
}

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static DEALLOCATIONS: AtomicU64 = AtomicU64::new(0);

fn counting_this_thread() -> bool {
    COUNT_THIS_THREAD.try_with(Cell::get).unwrap_or(false)
}

// This allocator is test instrumentation only. It delegates every operation to
// `System` unchanged and increments atomics only for the measured callback thread.
// The production format-independent runtime continues to forbid unsafe code.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if counting_this_thread() {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: `layout` comes from `GlobalAlloc::alloc`; forwarding it to the
        // system allocator preserves the required allocation contract.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if counting_this_thread() {
            DEALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: `ptr` and `layout` are the exact pair supplied by the caller
        // for a prior allocation from this allocator, which delegates to System.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if counting_this_thread() {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: same forwarding argument as `alloc`; System implements the
        // requested zero-initialized allocation contract.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if counting_this_thread() {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            DEALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: the caller provides a valid allocation/layout pair and the
        // requested size; forwarding unchanged preserves System's contract.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

struct Effect;
struct GainProcessor;

impl Component for Effect {
    type Processor = GainProcessor;
    type ActivationError = Infallible;

    fn schema(
        &self,
    ) -> Result<
        chassis_runtime::schema::ComponentSchema,
        chassis_runtime::schema::ComponentSchemaError,
    > {
        chassis_runtime::schema::ComponentSchema::stereo_effect_with_state(
            chassis_runtime::schema::ComponentId::new("org.nijaru.chassis.allocation-test")
                .unwrap(),
            chassis_runtime::schema::StateSchemaVersion::new(1),
            vec![
                ParameterDescriptor::float("gain", "Gain", 0.0, 1.0, 0.5).unwrap(),
                ParameterDescriptor::integer("steps", "Steps", 0, 4, 0).unwrap(),
                ParameterDescriptor::boolean("bypass", "Bypass", false).unwrap(),
                ParameterDescriptor::choice(
                    "mode",
                    "Mode",
                    vec![
                        ChoiceOption::new("clean", "Clean").unwrap(),
                        ChoiceOption::new("warm", "Warm").unwrap(),
                    ],
                    "clean",
                )
                .unwrap(),
            ],
        )
    }

    fn activate(
        &self,
        _config: &ActivationConfig<'_>,
        _parameters: &chassis_runtime::parameters::ParameterStore,
        _custom_state: &[chassis_runtime::state::StateEntry],
    ) -> Result<Self::Processor, Self::ActivationError> {
        Ok(GainProcessor)
    }
}

impl Processor for GainProcessor {}

impl Process<f32> for GainProcessor {
    fn process<B>(
        &mut self,
        block: &mut ProcessBlock<'_, '_, '_, f32, B>,
    ) -> Result<(), chassis_runtime::runtime::ProcessError>
    where
        B: ProcessBufferSource<f32> + ?Sized,
    {
        for mut channel in block.channels() {
            if let Ok(samples) = channel.copy_input_to_output() {
                for sample in samples {
                    *sample *= 0.5;
                }
            }
        }
        Ok(())
    }
}

fn process_config() -> ProcessConfig {
    ProcessConfig::new(
        48_000.0,
        Some(NonZeroU32::new(1).expect("one is non-zero")),
        NonZeroU32::new(512).expect("512 is non-zero"),
        64,
    )
    .expect("test process configuration is valid")
}

fn process_context(events: &[ParameterEvent]) -> ProcessContext<'_> {
    ProcessContext::new(
        ProcessMode::Realtime,
        TransportSnapshot::unknown(),
        ParameterEvents::new(events, 512, 64).expect("bounded events are valid"),
    )
}

#[test]
fn repeated_post_activation_process_calls_do_not_allocate_or_deallocate() {
    let component = Effect;
    let mut runtime = InstanceRuntime::for_component(&component).expect("runtime schema is valid");
    runtime
        .activate(&component, process_config(), DEFAULT_EFFECT_CONFIGURATION)
        .expect("activation succeeds");

    // Transfer a separately allocated control ID into the store. Completion
    // must never drop that allocation's last owner on the callback thread.
    runtime
        .parameters_mut()
        .set(
            "mode",
            ParameterValue::Choice(ChoiceId::new("warm").expect("control choice is valid")),
        )
        .expect("control edit succeeds");

    let saved_control =
        StateDocument::decode(&runtime.encode_state(StateLimits::default()).unwrap()).unwrap();

    let mut left = [1.0_f32; 512];
    let mut right = [1.0_f32; 512];
    let mode_parameter = ParameterIndex::new(3);
    let event_batches = ["clean", "warm"].map(|id| {
        let mode = runtime
            .parameters()
            .descriptor(mode_parameter)
            .unwrap()
            .choice_index(id)
            .unwrap();
        [
            ParameterEvent::set(0, ParameterIndex::new(1), ParameterEventValue::Integer(3)),
            ParameterEvent::set(
                0,
                ParameterIndex::new(2),
                ParameterEventValue::Boolean(true),
            ),
            ParameterEvent::set(0, ParameterIndex::new(3), ParameterEventValue::Choice(mode)),
            ParameterEvent::linear(511, ParameterIndex::new(0), 0.75),
        ]
    });

    ALLOCATIONS.store(0, Ordering::Relaxed);
    DEALLOCATIONS.store(0, Ordering::Relaxed);
    COUNT_THIS_THREAD.with(|counting| counting.set(true));

    for block in 0..1_000 {
        if block == 500 {
            // State decoding/publication is non-realtime. The following
            // callback must also replace its decoded choice without freeing it.
            COUNT_THIS_THREAD.with(|counting| counting.set(false));
            runtime.apply_state(&component, &saved_control).unwrap();
            COUNT_THIS_THREAD.with(|counting| counting.set(true));
        }
        // Dense synchronization and invalid payload rejection must also remain
        // allocation-free, including after a decoded stable-ID state load.
        let mode = runtime
            .parameters()
            .get_event_value(mode_parameter)
            .unwrap();
        runtime
            .parameters_mut()
            .set_event_value(mode_parameter, mode)
            .unwrap();
        assert_eq!(
            runtime.parameters_mut().set_event_value(
                mode_parameter,
                ParameterEventValue::Choice(ChoiceIndex::new(2)),
            ),
            Err(ParameterEventValueError::InvalidValue(
                ParameterValueError::UnknownChoice
            )),
        );
        let mut buffers = [
            ChannelBuffer::in_place(
                InputEndpoint::new(MAIN_INPUT_INDEX, 0),
                OutputEndpoint::new(MAIN_OUTPUT_INDEX, 0),
                &mut left,
                512,
            )
            .expect("left buffer is valid"),
            ChannelBuffer::in_place(
                InputEndpoint::new(MAIN_INPUT_INDEX, 1),
                OutputEndpoint::new(MAIN_OUTPUT_INDEX, 1),
                &mut right,
                512,
            )
            .expect("right buffer is valid"),
        ];
        runtime
            .process(
                512,
                process_context(&event_batches[block % 2]),
                &mut buffers,
            )
            .expect("process callback is valid");
        assert_eq!(
            runtime.parameters().get_event_value(mode_parameter),
            Some(ParameterEventValue::Choice(ChoiceIndex::new(
                u32::try_from(block % 2).unwrap()
            ))),
        );
    }

    COUNT_THIS_THREAD.with(|counting| counting.set(false));

    assert_eq!(ALLOCATIONS.load(Ordering::Relaxed), 0);
    assert_eq!(DEALLOCATIONS.load(Ordering::Relaxed), 0);

    runtime.deactivate().expect("deactivation succeeds");
    pending_resource_callbacks_do_not_allocate_or_reclaim();
    mixed_event_callbacks_do_not_allocate_or_reclaim();
}

fn pending_resource_callbacks_do_not_allocate_or_reclaim() {
    // A retained parameter snapshot owns allocations and choice IDs. Neither
    // frozen callbacks nor ignored automation may reclaim it or touch pending state.
    let component = resource_state::TableComponent::default();
    let mut runtime = InstanceRuntime::for_component(&component).unwrap();
    runtime
        .activate(&component, process_config(), DEFAULT_EFFECT_CONFIGURATION)
        .unwrap();
    let replacement = resource_state::document(1.0, &[3, 7]);
    runtime.apply_state(&component, &replacement).unwrap();
    let automation = [
        ParameterEvent::set(0, ParameterIndex::new(0), ParameterEventValue::Float(0.0)),
        ParameterEvent::set(
            0,
            ParameterIndex::new(1),
            ParameterEventValue::Choice(ChoiceIndex::new(0)),
        ),
        ParameterEvent::linear(511, ParameterIndex::new(0), 1.0),
    ];
    let mut left = [0.0_f64; 512];
    let mut right = [0.0_f64; 512];
    ALLOCATIONS.store(0, Ordering::Relaxed);
    DEALLOCATIONS.store(0, Ordering::Relaxed);
    COUNT_THIS_THREAD.with(|counting| counting.set(true));
    for _ in 0..1000 {
        let mut buffers = [
            ChannelBuffer::in_place(
                InputEndpoint::new(MAIN_INPUT_INDEX, 0),
                OutputEndpoint::new(MAIN_OUTPUT_INDEX, 0),
                &mut left,
                512,
            )
            .unwrap(),
            ChannelBuffer::in_place(
                InputEndpoint::new(MAIN_INPUT_INDEX, 1),
                OutputEndpoint::new(MAIN_OUTPUT_INDEX, 1),
                &mut right,
                512,
            )
            .unwrap(),
        ];
        runtime
            .process(
                512,
                ProcessContext::new(
                    ProcessMode::Realtime,
                    TransportSnapshot::unknown(),
                    ParameterEvents::new(&automation, 512, 8).unwrap(),
                ),
                &mut buffers,
            )
            .unwrap();
        runtime.reset().unwrap();
    }
    COUNT_THIS_THREAD.with(|counting| counting.set(false));
    assert_eq!(ALLOCATIONS.load(Ordering::Relaxed), 0);
    assert_eq!(DEALLOCATIONS.load(Ordering::Relaxed), 0);
    assert!(
        left.into_iter()
            .chain(right)
            .all(|value| value.to_bits() == 2.0_f64.to_bits())
    );
    assert_eq!(component.incompatible_controls.load(Ordering::Relaxed), 0);
    assert_eq!(runtime.state_document().unwrap(), replacement);
    runtime.deactivate().unwrap();
}

struct EventEcho;
impl Component for EventEcho {
    type Processor = Self;
    type ActivationError = Infallible;
    fn schema(
        &self,
    ) -> Result<
        chassis_runtime::schema::ComponentSchema,
        chassis_runtime::schema::ComponentSchemaError,
    > {
        chassis_runtime::schema::ComponentSchema::unidentified(
            vec![],
            chassis_runtime::audio::AudioIoPolicy::any_structurally_valid(),
            vec![
                EventPortDescriptor::new(
                    EventPortKey::new("notes.in"),
                    "Input",
                    EventPortDirection::Input,
                    &[EventDialect::Notes, EventDialect::Midi1],
                ),
                EventPortDescriptor::new(
                    EventPortKey::new("notes.out"),
                    "Output",
                    EventPortDirection::Output,
                    &[EventDialect::Notes, EventDialect::Midi1],
                ),
            ],
            vec![ParameterDescriptor::float("gain", "Gain", 0.0, 1.0, 0.5).unwrap()],
        )
    }
    fn activate(
        &self,
        _: &ActivationConfig<'_>,
        _: &chassis_runtime::parameters::ParameterStore,
        _: &[chassis_runtime::state::StateEntry],
    ) -> Result<Self, Infallible> {
        Ok(Self)
    }
}
impl Processor for EventEcho {}
impl Process<f32> for EventEcho {
    fn process<B: ProcessBufferSource<f32> + ?Sized>(
        &mut self,
        block: &mut ProcessBlock<'_, '_, '_, f32, B>,
    ) -> Result<(), ProcessError> {
        // The iterator borrows input storage, not the block/sink. Products can
        // emit directly while traversing real source order.
        for event in block.ordered_events().unwrap() {
            if let InputEventRef::Note(event) = event {
                let address = event.kind().address();
                let kind = if let NoteEventKind::Expression { expression, .. } = event.kind() {
                    NoteEventKind::Expression {
                        address: NoteAddress::new(
                            EventPortIndex::new(1),
                            address.note_id(),
                            address.channel(),
                            address.key(),
                        ),
                        expression,
                    }
                } else {
                    NoteEventKind::End { address }
                };
                block.try_push_note_event(NoteEvent::new(event.offset(), kind))?;
            } else if let InputEventRef::Midi(event) = event {
                block.try_push_midi_event(event.with_port(EventPortIndex::new(1)))?;
            }
        }
        Ok(())
    }
}

struct CountingEventSink {
    reject: bool,
    accepted: u32,
}
impl EventSink for CountingEventSink {
    fn try_push(&mut self, _: OutputEvent<'_>) -> Result<(), EventOutputError> {
        if self.reject {
            return Err(EventOutputError::Rejected);
        }
        self.accepted += 1;
        Ok(())
    }
}

fn mixed_event_callbacks_do_not_allocate_or_reclaim() {
    let mut runtime = InstanceRuntime::for_component(&EventEcho).unwrap();
    runtime
        .activate(
            &EventEcho,
            process_config()
                .with_max_note_events(2)
                .with_max_output_note_events(2)
                .with_midi_input_limits(MidiEventLimits::new(2, 128))
                .with_midi_output_limits(MidiEventLimits::new(2, 128)),
            chassis_runtime::audio::AudioIoConfiguration::new(&[]),
        )
        .unwrap();
    let parameters = [ParameterEvent::set(
        0,
        ParameterIndex::new(0),
        ParameterEventValue::Float(1.0),
    )];
    let address = NoteAddress::new(EventPortIndex::new(0), None, None, None);
    let notes = [
        NoteEvent::new(
            0,
            NoteEventKind::On {
                address,
                velocity: NormalizedValue::new(0.5).unwrap(),
            },
        ),
        NoteEvent::new(
            1,
            NoteEventKind::Expression {
                address,
                expression: NoteExpression::new(NoteExpressionKind::Tuning, 12.0).unwrap(),
            },
        ),
    ];
    let payload = [0x80; 128];
    let midi = [
        MidiEvent::new(0, EventPortIndex::new(0), MidiMessage::Short([0x90, 60, 0]))
            .with_flags(EventFlags::new(true, true)),
        MidiEvent::new(1, EventPortIndex::new(0), MidiMessage::SysEx(&payload)),
    ];
    let order = [
        InputEventIndex::Parameter(0),
        InputEventIndex::Midi(0),
        InputEventIndex::Note(0),
        InputEventIndex::Note(1),
        InputEventIndex::Midi(1),
    ];
    let mut sink = CountingEventSink {
        reject: false,
        accepted: 0,
    };
    let mut buffers: [ChannelBuffer<'_, f32>; 0] = [];
    let mut source = ChannelBufferSlice::new(&mut buffers);
    ALLOCATIONS.store(0, Ordering::Relaxed);
    DEALLOCATIONS.store(0, Ordering::Relaxed);
    COUNT_THIS_THREAD.with(|counting| counting.set(true));
    for block in 0..1000 {
        let context = ProcessContext::with_events(
            ProcessMode::Realtime,
            TransportSnapshot::unknown(),
            ParameterEvents::new(&parameters, 512, 1).unwrap(),
            NoteEvents::new(&notes, 512, 2).unwrap(),
            MidiEvents::new(&midi, 512, 2, 128).unwrap(),
        )
        .with_input_order(&order)
        .unwrap();
        let context = if block % 2 == 0 {
            context.without_parameter_events()
        } else {
            context
        };
        runtime
            .process_source_with_output(512, context, &mut source, &mut sink)
            .unwrap();
    }
    assert_eq!(sink.accepted, 4000);
    sink.reject = true;
    // Essential-output failure uses the same allocation-free error path. Do
    // not continue DSP after this failed run.
    let context = ProcessContext::with_events(
        ProcessMode::Realtime,
        TransportSnapshot::unknown(),
        ParameterEvents::new(&parameters, 512, 1).unwrap(),
        NoteEvents::new(&notes, 512, 2).unwrap(),
        MidiEvents::new(&midi, 512, 2, 128).unwrap(),
    )
    .with_input_order(&order)
    .unwrap();
    assert_eq!(
        runtime.process_source_with_output(512, context, &mut source, &mut sink),
        Err(InstanceProcessError::Processor(ProcessError::EventOutput(
            EventOutputError::Rejected
        )))
    );
    COUNT_THIS_THREAD.with(|counting| counting.set(false));
    assert_eq!(ALLOCATIONS.load(Ordering::Relaxed), 0);
    assert_eq!(DEALLOCATIONS.load(Ordering::Relaxed), 0);
}
