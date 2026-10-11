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
    parameters::{ChoiceId, ChoiceOption, ParameterDescriptor, ParameterIndex, ParameterValue},
    process::{
        ActivationConfig, ProcessBlock, ProcessBufferSource, ProcessChannel, ProcessConfig,
        ProcessContext, ProcessMode, TransportSnapshot,
    },
    runtime::{Component, InstanceRuntime, Process, Processor},
    state::{StateDocument, StateLimits},
};

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
// The production Chassis crates continue to forbid unsafe code.
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
    ) -> Result<Self::Processor, Self::ActivationError> {
        Ok(GainProcessor)
    }
}

impl Processor for GainProcessor {}

impl Process<f32> for GainProcessor {
    fn process<B>(&mut self, block: &mut ProcessBlock<'_, '_, '_, f32, B>)
    where
        B: ProcessBufferSource<f32> + ?Sized,
    {
        for mut channel in block.channels() {
            if let Ok(samples) = channel.make_in_place() {
                for sample in samples {
                    *sample *= 0.5;
                }
            }
        }
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

fn process_context<'a>(events: &'a [ParameterEvent<'a>]) -> ProcessContext<'a> {
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
    let event_batches = ["clean", "warm"].map(|mode| {
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
            runtime
                .apply_state(&saved_control, |_, _| Ok::<_, Infallible>(()))
                .unwrap();
            COUNT_THIS_THREAD.with(|counting| counting.set(true));
        }
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
    }

    COUNT_THIS_THREAD.with(|counting| counting.set(false));

    assert_eq!(ALLOCATIONS.load(Ordering::Relaxed), 0);
    assert_eq!(DEALLOCATIONS.load(Ordering::Relaxed), 0);

    runtime.deactivate().expect("deactivation succeeds");
}
