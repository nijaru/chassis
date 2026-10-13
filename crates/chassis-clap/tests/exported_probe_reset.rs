#![allow(unsafe_code)]
//! Exercise the actual exported fixture, not a duplicate delay processor.

use clack_host::prelude::*;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

#[path = "../../../examples/clap-delayed-probe/src/lib.rs"]
mod exported_probe;

struct CountingAllocator;
static COUNT: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static DEALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

// SAFETY: all allocator operations are forwarded unchanged to System.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNT.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: layout is forwarded unchanged.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if COUNT.load(Ordering::Relaxed) {
            DEALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: pointer and layout are forwarded unchanged.
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

struct Host;
impl SharedHandler<'_> for Host {
    fn request_restart(&self) {}
    fn request_process(&self) {}
    fn request_callback(&self) {}
}
impl MainThreadHandler<'_> for Host {}
impl AudioProcessorHandler<'_> for Host {}
impl HostHandlers for Host {
    type Shared<'a> = Host;
    type MainThread<'a> = Host;
    type AudioProcessor<'a> = Host;
}

fn process(processor: &mut StartedPluginAudioProcessor<Host>, input: [f32; 17]) -> [[f32; 17]; 2] {
    let mut inputs = [input; 2];
    let mut sidechain = [[0.0; 17]; 2];
    let mut outputs = [[0.0; 17]; 2];
    let mut input_ports = AudioPorts::with_capacity(4, 2);
    let mut output_ports = AudioPorts::with_capacity(2, 1);
    let input_audio = input_ports.with_input_buffers([
        AudioPortBuffer {
            channels: AudioPortBufferType::f32_input_only(
                inputs.iter_mut().map(InputChannel::variable),
            ),
            latency: 0,
        },
        AudioPortBuffer {
            channels: AudioPortBufferType::f32_input_only(
                sidechain.iter_mut().map(InputChannel::variable),
            ),
            latency: 0,
        },
    ]);
    let mut output_audio = output_ports.with_output_buffers([AudioPortBuffer {
        channels: AudioPortBufferType::f32_output_only(
            outputs.iter_mut().map(|channel| &mut channel[..]),
        ),
        latency: 0,
    }]);
    processor
        .process(
            &input_audio,
            &mut output_audio,
            &InputEvents::empty(),
            &mut OutputEvents::void(),
            None,
            None,
        )
        .unwrap();
    outputs
}

#[test]
fn exported_reset_clears_history_and_preserves_activation_latency_without_allocation() {
    // SAFETY: this is the fixture's static exported entry with valid Clack callbacks.
    let entry = unsafe { PluginEntry::load_from_raw(&exported_probe::clap_entry, c"") }.unwrap();
    let mut plugin = PluginInstance::<Host>::new(
        |()| Host,
        |_| Host,
        &entry,
        c"com.nijaru.chassis.delayed-probe",
        &HostInfo::new("reset-test", "", "", "").unwrap(),
    )
    .unwrap();
    let processor = plugin
        .activate(
            |_, _| Host,
            PluginAudioConfiguration {
                sample_rate: 48_000.0,
                min_frames_count: 1,
                max_frames_count: 17,
            },
        )
        .unwrap();
    let mut processor = processor.start_processing().unwrap();
    // A non-ring-aligned block leaves both nonzero history and nonzero positions.
    assert_eq!(process(&mut processor, [1.0; 17]), [[0.0; 17]; 2]);
    COUNT.store(true, Ordering::Relaxed);
    processor.reset();
    COUNT.store(false, Ordering::Relaxed);
    assert_eq!(ALLOCATIONS.load(Ordering::Relaxed), 0);
    assert_eq!(DEALLOCATIONS.load(Ordering::Relaxed), 0);

    let mut impulse = [0.0; 17];
    impulse[0] = 1.0;
    let mut rendered = [Vec::new(), Vec::new()];
    for block in 0..5 {
        let output = process(&mut processor, if block == 0 { impulse } else { [0.0; 17] });
        for (channel, samples) in rendered.iter_mut().zip(output) {
            channel.extend(samples);
        }
    }
    for channel in rendered {
        for (offset, sample) in channel.iter().enumerate() {
            assert_eq!(
                sample.to_bits(),
                if offset == 64 {
                    1.0_f32.to_bits()
                } else {
                    0.0_f32.to_bits()
                },
                "sample {offset}"
            );
        }
    }
    plugin.deactivate(processor.stop_processing());
}
