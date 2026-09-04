#![allow(unsafe_code)]

//! Test-only heap instrumentation for the post-activation core process path.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    convert::Infallible,
    num::NonZeroU32,
    sync::atomic::{AtomicU64, Ordering},
};

use chassis_core::{
    audio::{DEFAULT_EFFECT_CONFIGURATION, MAIN_INPUT, MAIN_OUTPUT},
    automation::ParameterEvents,
    buffer::{ChannelBuffer, InputEndpoint, OutputEndpoint},
    process::{
        ActivationConfig, ProcessBlock, ProcessBufferSource, ProcessChannel, ProcessConfig,
        ProcessContext, ProcessMode, TransportSnapshot,
    },
    runtime::{Component, InstanceRuntime, Process, Processor},
};

struct CountingAllocator;

thread_local! {
    static COUNT_THIS_THREAD: Cell<bool> = const { Cell::new(false) };
}

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static DEALLOCATIONS: AtomicU64 = AtomicU64::new(0);

fn counting_this_thread() -> bool {
    COUNT_THIS_THREAD
        .try_with(Cell::get)
        .unwrap_or(false)
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

fn process_context() -> ProcessContext<'static> {
    ProcessContext::new(
        ProcessMode::Realtime,
        TransportSnapshot::unknown(),
        ParameterEvents::empty_for_block(512),
    )
}

#[test]
fn repeated_post_activation_process_calls_do_not_allocate_or_deallocate() {
    let component = Effect;
    let mut runtime = InstanceRuntime::for_component(&component).expect("runtime schema is valid");
    runtime
        .activate(&component, process_config(), DEFAULT_EFFECT_CONFIGURATION)
        .expect("activation succeeds");

    let mut left = [1.0_f32; 512];
    let mut right = [1.0_f32; 512];

    ALLOCATIONS.store(0, Ordering::Relaxed);
    DEALLOCATIONS.store(0, Ordering::Relaxed);
    COUNT_THIS_THREAD.with(|counting| counting.set(true));

    for _ in 0..1_000 {
        let mut buffers = [
            ChannelBuffer::in_place(
                InputEndpoint::new(MAIN_INPUT, 0),
                OutputEndpoint::new(MAIN_OUTPUT, 0),
                &mut left,
                512,
            )
            .expect("left buffer is valid"),
            ChannelBuffer::in_place(
                InputEndpoint::new(MAIN_INPUT, 1),
                OutputEndpoint::new(MAIN_OUTPUT, 1),
                &mut right,
                512,
            )
            .expect("right buffer is valid"),
        ];
        runtime
            .process(512, process_context(), &mut buffers)
            .expect("process callback is valid");
    }

    COUNT_THIS_THREAD.with(|counting| counting.set(false));

    assert_eq!(ALLOCATIONS.load(Ordering::Relaxed), 0);
    assert_eq!(DEALLOCATIONS.load(Ordering::Relaxed), 0);

    runtime.deactivate().expect("deactivation succeeds");
}
