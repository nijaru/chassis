#![allow(unsafe_code)]
//! Actual exported gain fixture: timed bypass, state, both precisions, callback bounds.

use clack_extensions::{
    params::{ParamInfoBuffer, ParamInfoFlags, PluginParams},
    state::PluginState,
};
use clack_host::{
    events::event_types::{ParamModEvent, ParamValueEvent},
    prelude::*,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    io::Cursor,
    sync::atomic::{AtomicU64, Ordering},
};

#[path = "../../../examples/clap-conformance/src/lib.rs"]
mod exported;

thread_local! { static COUNT: Cell<bool> = const { Cell::new(false) }; }
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static DEALLOCATIONS: AtomicU64 = AtomicU64::new(0);
struct Allocator;
// SAFETY: allocation arguments are forwarded unchanged to System.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNT.try_with(Cell::get).unwrap_or(false) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: unchanged layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if COUNT.try_with(Cell::get).unwrap_or(false) {
            DEALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: unchanged allocation pair.
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static GLOBAL: Allocator = Allocator;

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

fn set(events: &mut EventBuffer, offset: u32, id: u32, value: f64) {
    events.push(&ParamValueEvent::new(
        offset,
        ClapId::new(id),
        Pckn::match_all(),
        value,
    ));
}
fn mixed_controls() -> EventBuffer {
    let mut events = EventBuffer::with_capacity(513);
    for offset in 0..8 {
        set(&mut events, offset, 7002, 0.25);
        events.push(&ParamModEvent::new(
            offset,
            ClapId::new(7002),
            Pckn::match_all(),
            0.25,
        ));
        for slot in 0..62 {
            let bypass = if slot == 61 {
                offset % 2 != 0
            } else {
                slot % 2 != 0
            };
            set(&mut events, offset, 7003, f64::from(u8::from(bypass)));
        }
    }
    events
}

// One test path specialized only at the two CLAP sample buffer constructors.
macro_rules! renderer {
    ($name:ident, $sample:ty, $input:ident, $output:ident) => {
        fn $name(
            processor: &mut StartedPluginAudioProcessor<Host>,
            events: &EventBuffer,
            amplitude: f64,
        ) -> Result<[f64; 8], ()> {
            // Test inputs intentionally cross the f64 -> f32 boundary.
            #[allow(clippy::cast_possible_truncation)]
            let mut inputs = [[amplitude as $sample; 8]; 4];
            let mut outputs = [[-99.0 as $sample; 8]; 2];
            let mut input_ports = AudioPorts::with_capacity(4, 2);
            let mut output_ports = AudioPorts::with_capacity(2, 1);
            let (main, side) = inputs.split_at_mut(2);
            let input =
                input_ports.with_input_buffers([main, side].map(|channels| AudioPortBuffer {
                    channels: AudioPortBufferType::$input(
                        channels.iter_mut().map(InputChannel::variable),
                    ),
                    latency: 0,
                }));
            let mut output = output_ports.with_output_buffers([AudioPortBuffer {
                channels: AudioPortBufferType::$output(
                    outputs.iter_mut().map(|samples| &mut samples[..]),
                ),
                latency: 0,
            }]);
            COUNT.set(true);
            let result = processor.process(
                &input,
                &mut output,
                &events.as_input(),
                &mut OutputEvents::void(),
                None,
                None,
            );
            COUNT.set(false);
            let outputs = outputs.map(|channel| channel.map(f64::from));
            assert_eq!(outputs[0].map(f64::to_bits), outputs[1].map(f64::to_bits));
            if result.is_err() {
                assert_eq!(
                    outputs[0].map(f64::to_bits),
                    [-99.0; 8].map(f64::to_bits),
                    "rejected input must not reach DSP"
                );
                return Err(());
            }
            Ok(outputs[0])
        }
    };
}
renderer!(render_f32, f32, f32_input_only, f32_output_only);
renderer!(render_f64, f64, f64_input_only, f64_output_only);

fn assert_metadata(plugin: &mut PluginInstance<Host>, params: PluginParams) {
    let handle = plugin.plugin_handle();
    assert_eq!(params.count(&handle), 3);
    for index in 0..3 {
        let mut storage = ParamInfoBuffer::new();
        let info = params.get_info(&handle, index, &mut storage).unwrap();
        assert_eq!(
            info.flags.contains(ParamInfoFlags::IS_BYPASS),
            info.id == ClapId::new(7003)
        );
        if info.id == ClapId::new(7003) {
            assert!(info.flags.contains(ParamInfoFlags::IS_STEPPED));
            assert!(!info.flags.contains(ParamInfoFlags::IS_MODULATABLE));
            assert_eq!(
                [info.min_value, info.max_value, info.default_value].map(f64::to_bits),
                [0.0, 1.0, 0.0].map(f64::to_bits)
            );
        }
    }
}

#[test]
fn readonly_output_tables_preserve_host_ownership() {
    use clap_sys::{
        events::{clap_event_header, clap_output_events},
        ext::params::{CLAP_EXT_PARAMS, clap_plugin_params},
    };
    struct Sink {
        table: Cell<*const clap_output_events>,
        calls: Cell<u32>,
        identity_matches: Cell<bool>,
    }
    unsafe extern "C" fn push(
        table: *const clap_output_events,
        _: *const clap_event_header,
    ) -> bool {
        // SAFETY: the test keeps the immutable table and Cell-backed context
        // alive across every synchronous call.
        let sink = unsafe { &*((*table).ctx.cast::<Sink>()) };
        sink.identity_matches
            .set(sink.identity_matches.get() && sink.table.get() == table);
        sink.calls.set(sink.calls.get() + 1);
        sink.calls.get() % 2 == 1
    }
    let sink = Sink {
        table: Cell::new(std::ptr::null()),
        calls: Cell::new(0),
        identity_matches: Cell::new(true),
    };
    let table = clap_output_events {
        ctx: std::ptr::from_ref(&sink).cast_mut().cast(),
        try_push: Some(push),
    };
    sink.table.set(std::ptr::from_ref(&table));
    let event = ParamValueEvent::new(0, ClapId::new(7003), Pckn::match_all(), 1.0);
    // SAFETY: valid original callback/context and borrowed event throughout.
    unsafe {
        OutputEvents::with_raw(&table, |output| {
            assert!(output.try_push(event).is_ok());
            assert!(output.try_push(event).is_err());
            // Safe replacement must affect only the local descriptor.
            *output = OutputEvents::void();
        });
    }
    assert_eq!(sink.calls.get(), 2);
    assert!(sink.identity_matches.get());
    assert_eq!(table.ctx, std::ptr::from_ref(&sink).cast_mut().cast());

    let mut plugin = plugin();
    let raw = plugin.plugin_handle().as_raw_ptr();
    // SAFETY: live initialized plugin and standard params identifier/layout.
    let params = unsafe {
        &*(((*raw).get_extension.unwrap())(raw, CLAP_EXT_PARAMS.as_ptr())
            .cast::<clap_plugin_params>())
    };
    let flush = params.flush.unwrap();
    let mut input = EventBuffer::new();
    input.push(&event);
    // Unlike Clack's host convenience API, pass a genuinely shared descriptor.
    // SAFETY: legal inactive main-thread flush; tables/context outlive the call.
    unsafe { flush(raw, input.as_input().as_raw(), std::ptr::from_ref(&table)) };
    let processor = activate(&mut plugin);
    input.clear();
    set(&mut input, 0, 7003, 0.0);
    // SAFETY: legal active audio-domain flush, exclusive with processing.
    unsafe { flush(raw, input.as_input().as_raw(), std::ptr::from_ref(&table)) };
    plugin.deactivate(processor);
    assert_eq!(
        sink.calls.get(),
        2,
        "gain fixture emits no parameter events"
    );
    assert_eq!(table.ctx, std::ptr::from_ref(&sink).cast_mut().cast());
}

fn plugin() -> PluginInstance<Host> {
    // SAFETY: this is the real fixture's static entry and valid Clack vtable.
    let entry = unsafe { PluginEntry::load_from_raw(&exported::clap_entry, c"") }.unwrap();
    PluginInstance::<Host>::new(
        |()| Host,
        |_| Host,
        &entry,
        c"com.nijaru.chassis.conformance",
        &HostInfo::new("bypass", "", "", "").unwrap(),
    )
    .unwrap()
}

fn activate(plugin: &mut PluginInstance<Host>) -> StoppedPluginAudioProcessor<Host> {
    plugin
        .activate(
            |_, _| Host,
            PluginAudioConfiguration {
                sample_rate: 48_000.0,
                min_frames_count: 1,
                max_frames_count: 8,
            },
        )
        .unwrap()
}

#[test]
fn exported_bypass_is_timed_persistent_and_allocation_free_at_mixed_event_bound() {
    let mut plugin = plugin();
    let handle = plugin.plugin_handle();
    let params = handle.get_extension::<PluginParams>().unwrap();
    let state = handle.get_extension::<PluginState>().unwrap();
    assert_metadata(&mut plugin, params);
    let mut flush = EventBuffer::new();
    set(&mut flush, 0, 7003, 1.0);
    params.flush(
        &mut plugin.inactive_plugin_handle().unwrap(),
        &flush.as_input(),
        &mut OutputEvents::void(),
    );
    let mut processor = activate(&mut plugin).start_processing().unwrap();
    let empty = EventBuffer::new();
    assert_eq!(
        render_f32(&mut processor, &empty, 1.0)
            .unwrap()
            .map(f64::to_bits),
        [1.0; 8].map(f64::to_bits)
    );
    // Bypass retains the fixture's subnormal cleanup in both precisions.
    let zero = [0.0; 8].map(f64::to_bits);
    assert_eq!(
        render_f32(&mut processor, &empty, f64::from(f32::MIN_POSITIVE) / 2.0)
            .unwrap()
            .map(f64::to_bits),
        zero
    );
    assert_eq!(
        render_f64(&mut processor, &empty, f64::MIN_POSITIVE / 2.0)
            .unwrap()
            .map(f64::to_bits),
        zero
    );
    let mut mixed = mixed_controls();
    for _ in 0..1_000 {
        for render in [render_f32, render_f64] {
            assert_eq!(
                render(&mut processor, &mixed, 1.0)
                    .unwrap()
                    .map(f64::to_bits),
                [0.5, 1.0, 0.5, 1.0, 0.5, 1.0, 0.5, 1.0].map(f64::to_bits)
            );
        }
    }
    let mut saved = Vec::new();
    state.save(&plugin.plugin_handle(), &mut saved).unwrap();
    processor.reset();
    assert_eq!(
        render_f64(&mut processor, &empty, 1.0)
            .unwrap()
            .map(f64::to_bits),
        [1.0; 8].map(f64::to_bits)
    );
    flush.clear();
    set(&mut flush, 0, 7003, 0.99); // CLAP stepped conversion truncates toward zero.
    set(&mut flush, 0, 7002, 0.75);
    flush.push(&ParamModEvent::new(
        0,
        ClapId::new(7002),
        Pckn::match_all(),
        -0.25,
    ));
    params.flush_active(
        &mut processor.plugin_handle(),
        &flush.as_input(),
        &mut OutputEvents::void(),
    );
    for render in [render_f32, render_f64] {
        assert_eq!(
            render(&mut processor, &empty, 1.0)
                .unwrap()
                .map(f64::to_bits),
            [0.5; 8].map(f64::to_bits)
        );
    }
    state
        .load(&plugin.plugin_handle(), &mut Cursor::new(saved))
        .unwrap();
    assert_eq!(
        render_f64(&mut processor, &empty, 1.0)
            .unwrap()
            .map(f64::to_bits),
        [1.0; 8].map(f64::to_bits)
    );
    set(&mut mixed, 7, 7003, 0.0);
    assert!(
        render_f64(&mut processor, &mixed, 1.0).is_err(),
        "513 recognized controls exceed the shared 512-event bound"
    );
    assert_eq!(
        params
            .get_value(&plugin.plugin_handle(), ClapId::new(7003))
            .map(f64::to_bits),
        Some(1.0_f64.to_bits())
    );
    plugin.deactivate(processor.stop_processing());
    assert_eq!(ALLOCATIONS.load(Ordering::Relaxed), 0);
    assert_eq!(DEALLOCATIONS.load(Ordering::Relaxed), 0);
}
