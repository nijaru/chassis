//! Actual-entry global modulation, lifetime, and strict recognized-input admission.
use super::*;
use clack_extensions::{
    params::{ParamInfoBuffer, ParamInfoFlags},
    state::PluginState,
};
use clack_host::events::UnknownEvent;
use std::io::Cursor;

fn plugin() -> PluginInstance<TestHostHandlers> {
    let entry =
        PluginEntry::load_from_clack::<SingleComponentEntry<ProbeComponent, F32AndF64>>(c"")
            .unwrap();
    let descriptor = entry
        .get_factory::<PluginFactory>()
        .unwrap()
        .plugin_descriptor(0)
        .unwrap();
    PluginInstance::<TestHostHandlers>::new(
        |()| TestHostShared {
            latency_changes: Arc::new(AtomicU32::new(0)),
        },
        |shared| TestHostMainThread {
            latency_changes: Arc::clone(&shared.latency_changes),
        },
        &entry,
        descriptor.id().unwrap(),
        &HostInfo::new("modulation", "", "", "").unwrap(),
    )
    .unwrap()
}
fn activate(
    plugin: &mut PluginInstance<TestHostHandlers>,
) -> StartedPluginAudioProcessor<TestHostHandlers> {
    plugin
        .activate(
            |_, _| TestHostAudioProcessor,
            PluginAudioConfiguration {
                sample_rate: 48_000.0,
                min_frames_count: 1,
                max_frames_count: 64,
            },
        )
        .unwrap()
        .start_processing()
        .unwrap()
}
fn controls(base: f64, amount: f64) -> EventBuffer {
    let mut events = EventBuffer::new();
    events.push(&ParamValueEvent::new(
        0,
        ClapId::new(TRIM_CLAP_ID),
        Pckn::match_all(),
        base,
    ));
    events.push(&ParamModEvent::new(
        0,
        ClapId::new(TRIM_CLAP_ID),
        Pckn::match_all(),
        amount,
    ));
    events
}
fn input_channel(samples: &mut [f64; 4]) -> InputChannel<'_, f64> {
    InputChannel::variable(samples.as_mut_slice())
}
fn render(
    processor: &mut StartedPluginAudioProcessor<TestHostHandlers>,
    events: &EventBuffer,
) -> Result<[f64; 4], ()> {
    let mut main = [[1.0_f64; 4]; 2];
    let mut side = [[0.0_f64; 4]; 2];
    let mut outputs = [[-99.0_f64; 4]; 2];
    let mut inputs = AudioPorts::with_capacity(4, 2);
    let mut ports = AudioPorts::with_capacity(2, 1);
    let audio = inputs.with_input_buffers([
        AudioPortBuffer {
            channels: AudioPortBufferType::f64_input_only(main.iter_mut().map(input_channel)),
            latency: 0,
        },
        AudioPortBuffer {
            channels: AudioPortBufferType::f64_input_only(side.iter_mut().map(input_channel)),
            latency: 0,
        },
    ]);
    let mut output = ports.with_output_buffers([AudioPortBuffer {
        channels: AudioPortBufferType::f64_output_only(
            outputs.iter_mut().map(<[f64; 4]>::as_mut_slice),
        ),
        latency: 0,
    }]);
    let result = processor.process(
        &audio,
        &mut output,
        &events.as_input(),
        &mut OutputEvents::void(),
        None,
        None,
    );
    if result.is_err() {
        assert_eq!(
            outputs.map(|channel| channel.map(f64::to_bits)),
            [[(-99.0_f64).to_bits(); 4]; 2],
            "invalid batch must prevent DSP"
        );
        return Err(());
    }
    assert_eq!(outputs[0].map(f64::to_bits), outputs[1].map(f64::to_bits));
    Ok(outputs[0])
}

fn assert_render(
    processor: &mut StartedPluginAudioProcessor<TestHostHandlers>,
    events: &EventBuffer,
    expected: [f64; 4],
) {
    assert_eq!(
        render(processor, events).unwrap().map(f64::to_bits),
        expected.map(f64::to_bits)
    );
}

fn assert_metadata(plugin: &mut PluginInstance<TestHostHandlers>, params: PluginParams) {
    for index in 0..4 {
        let mut buffer = ParamInfoBuffer::new();
        let flags = params
            .get_info(&plugin.plugin_handle(), index, &mut buffer)
            .unwrap()
            .flags;
        assert_eq!(flags.contains(ParamInfoFlags::IS_MODULATABLE), index == 0);
        assert!(
            !flags.contains(ParamInfoFlags::IS_BYPASS),
            "even the ordinary Boolean named bypass has no implicit role"
        );
        assert!(!flags.intersects(
            ParamInfoFlags::IS_MODULATABLE_PER_NOTE_ID
                | ParamInfoFlags::IS_MODULATABLE_PER_KEY
                | ParamInfoFlags::IS_MODULATABLE_PER_CHANNEL
                | ParamInfoFlags::IS_MODULATABLE_PER_PORT
        ));
    }
}

#[test]
fn held_modulation_is_audible_but_not_state_across_flush_reset_and_reactivation() {
    let mut plugin = plugin();
    let params = plugin
        .plugin_handle()
        .get_extension::<PluginParams>()
        .unwrap();
    let state = plugin
        .plugin_handle()
        .get_extension::<PluginState>()
        .unwrap();
    assert_metadata(&mut plugin, params);
    // Flush loses timing, not list order; inactive external input must survive activation.
    params.flush(
        &mut plugin.inactive_plugin_handle().unwrap(),
        &controls(0.25, 0.5).as_input(),
        &mut OutputEvents::void(),
    );
    let mut processor = activate(&mut plugin);
    assert_render(&mut processor, &EventBuffer::new(), [0.75; 4]);
    let mut curve = controls(0.25, -0.5);
    curve.push(&ParamModEvent::new(
        1,
        ClapId::new(TRIM_CLAP_ID),
        Pckn::match_all(),
        0.5,
    ));
    curve.push(&ParamValueEvent::new(
        2,
        ClapId::new(TRIM_CLAP_ID),
        Pckn::match_all(),
        0.75,
    ));
    curve.push(&ParamModEvent::new(
        3,
        ClapId::new(TRIM_CLAP_ID),
        Pckn::match_all(),
        -0.25,
    ));
    curve.push(&ParamModEvent::new(
        3,
        ClapId::new(999),
        Pckn::from_raw(0, 0, 60, 5),
        f64::NAN,
    ));
    curve.push(&ParamValueEvent::new(
        3,
        ClapId::new(999),
        Pckn::from_raw(0, 0, 60, 5),
        f64::NAN,
    ));
    assert_render(&mut processor, &curve, [0.0, 0.75, 1.0, 0.5]);
    assert_eq!(
        params.get_value(&plugin.plugin_handle(), ClapId::new(TRIM_CLAP_ID)),
        Some(0.75)
    );
    let mut saved = Vec::new();
    state.save(&plugin.plugin_handle(), &mut saved).unwrap();
    params.flush_active(
        &mut processor.plugin_handle(),
        &controls(0.25, 0.25).as_input(),
        &mut OutputEvents::void(),
    );
    processor.reset();
    assert_render(&mut processor, &EventBuffer::new(), [0.5; 4]);
    state
        .load(&plugin.plugin_handle(), &mut Cursor::new(&saved))
        .unwrap();
    assert_render(&mut processor, &EventBuffer::new(), [1.0; 4]);
    plugin.deactivate(processor.stop_processing());
    let mut processor = activate(&mut plugin);
    assert_render(&mut processor, &EventBuffer::new(), [1.0; 4]);
    plugin.deactivate(processor.stop_processing());
    let mut reopened = self::plugin();
    let state = reopened
        .plugin_handle()
        .get_extension::<PluginState>()
        .unwrap();
    state
        .load(&reopened.plugin_handle(), &mut Cursor::new(&saved))
        .unwrap();
    let mut processor = activate(&mut reopened);
    assert_render(&mut processor, &EventBuffer::new(), [0.75; 4]);
    reopened.deactivate(processor.stop_processing());
}

#[test]
fn invalid_recognized_batches_cannot_partially_change_base_or_held_signal() {
    let mut plugin = plugin();
    let params = plugin
        .plugin_handle()
        .get_extension::<PluginParams>()
        .unwrap();
    let mut invalid = Vec::new();
    for amount in [f64::NAN, f64::INFINITY] {
        // The initial valid base set must not leak through invalid flush.
        invalid.push(controls(0.75, amount));
    }
    for field in 0..4 {
        for target in [-2, 0] {
            let event = ParamModEvent::new(0, ClapId::new(TRIM_CLAP_ID), Pckn::match_all(), 1.0);
            let mut raw = *event.as_raw();
            match field {
                0 => raw.note_id = target,
                1 => raw.port_index = i16::try_from(target).unwrap(),
                2 => raw.channel = i16::try_from(target).unwrap(),
                _ => raw.key = i16::try_from(target).unwrap(),
            }
            let mut events = controls(0.75, 0.75);
            events.push(&ParamModEvent::from_raw(&raw));
            invalid.push(events);
        }
    }
    let value = ParamValueEvent::new(0, ClapId::new(TRIM_CLAP_ID), Pckn::match_all(), 0.75);
    let mut raw = *value.as_raw();
    raw.channel = -2;
    let mut targeted_value = controls(0.75, 0.75);
    targeted_value.push(&ParamValueEvent::from_raw(&raw));
    invalid.push(targeted_value);
    let mut bad_base = controls(0.75, 0.75);
    bad_base.push(&ParamValueEvent::new(
        0,
        ClapId::new(TRIM_CLAP_ID),
        Pckn::match_all(),
        f64::NAN,
    ));
    invalid.push(bad_base);
    let mut unsupported = controls(0.75, 0.75);
    unsupported.push(&ParamModEvent::new(
        0,
        ClapId::new(8),
        Pckn::match_all(),
        0.5,
    ));
    invalid.push(unsupported);
    let mut malformed = controls(0.75, 0.75);
    let event = ParamModEvent::new(0, ClapId::new(TRIM_CLAP_ID), Pckn::match_all(), 1.0);
    let mut raw = *event.as_raw();
    raw.header.size -= 1;
    // SAFETY: raw is aligned and lives through the copy. Its full allocation
    // contains the shorter declared extent, including a complete event header.
    malformed.push(unsafe { UnknownEvent::from_raw(&raw const raw.header) });
    invalid.push(malformed);
    let mut over_budget = EventBuffer::new();
    for _ in 0..65 {
        over_budget.push(&event);
    }
    invalid.push(over_budget);
    for events in invalid {
        params.flush(
            &mut plugin.inactive_plugin_handle().unwrap(),
            &controls(0.25, 0.25).as_input(),
            &mut OutputEvents::void(),
        );
        // A per-block budget overflow is legal flush (its separate total-input
        // bound still applies), so test that batch only through process below.
        if events.len() <= 64 {
            params.flush(
                &mut plugin.inactive_plugin_handle().unwrap(),
                &events.as_input(),
                &mut OutputEvents::void(),
            );
        }
        let mut processor = activate(&mut plugin);
        assert_render(&mut processor, &EventBuffer::new(), [0.5; 4]);
        params.flush_active(
            &mut processor.plugin_handle(),
            &events.as_input(),
            &mut OutputEvents::void(),
        );
        if events.len() <= 64 {
            assert_render(&mut processor, &EventBuffer::new(), [0.5; 4]);
        }
        assert!(render(&mut processor, &events).is_err());
        assert_eq!(
            params.get_value(&plugin.plugin_handle(), ClapId::new(TRIM_CLAP_ID)),
            Some(0.25)
        );
        plugin.deactivate(processor.stop_processing());
    }
}
