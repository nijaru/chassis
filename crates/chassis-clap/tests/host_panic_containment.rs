//! In-process CLAP proof that product panics are contained at the Clack FFI boundary.

use std::{
    convert::Infallible,
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
};

use chassis_clap::{ClapComponent, SingleComponentEntry};
use chassis_runtime::{
    process::{ActivationConfig, ProcessBlock},
    runtime::{Component, Process, Processor},
};
use clack_host::{factory::plugin::PluginFactory, prelude::*};

struct TestHostShared;
struct TestHostMainThread;
struct TestHostAudioProcessor;
struct TestHostHandlers;

impl SharedHandler<'_> for TestHostShared {
    fn request_restart(&self) {}
    fn request_process(&self) {}
    fn request_callback(&self) {}
}

impl MainThreadHandler<'_> for TestHostMainThread {}
impl AudioProcessorHandler<'_> for TestHostAudioProcessor {}

impl HostHandlers for TestHostHandlers {
    type Shared<'a> = TestHostShared;
    type MainThread<'a> = TestHostMainThread;
    type AudioProcessor<'a> = TestHostAudioProcessor;
}

fn host_info() -> HostInfo {
    HostInfo::new("chassis-test", "", "", "").expect("host info is valid")
}

fn instance(entry: &PluginEntry) -> PluginInstance<TestHostHandlers> {
    let descriptor = entry
        .get_factory::<PluginFactory>()
        .unwrap()
        .plugin_descriptor(0)
        .unwrap();
    PluginInstance::new(
        |()| TestHostShared,
        |_| TestHostMainThread,
        entry,
        descriptor.id().unwrap(),
        &host_info(),
    )
    .unwrap()
}

const FACTORY_PANIC: u8 = 0;
const LATENCY_PANIC: u8 = 1;
const TAIL_PANIC: u8 = 2;
static ACTIVATION_DROPS: AtomicU32 = AtomicU32::new(0);
static ACTIVATION_DSP_CALLS: AtomicU32 = AtomicU32::new(0);

struct ActivationPanicProbe<const STAGE: u8> {
    panic_next_activation: AtomicBool,
}

impl<const STAGE: u8> Default for ActivationPanicProbe<STAGE> {
    fn default() -> Self {
        Self {
            panic_next_activation: AtomicBool::new(true),
        }
    }
}

impl<const STAGE: u8> Component for ActivationPanicProbe<STAGE> {
    type Processor = ActivationPanicProcessor;
    type ActivationError = Infallible;

    fn schema(
        &self,
    ) -> Result<
        chassis_runtime::schema::ComponentSchema,
        chassis_runtime::schema::ComponentSchemaError,
    > {
        chassis_runtime::schema::ComponentSchema::new(
            chassis_runtime::schema::ComponentId::new("test.activation-panic").unwrap(),
            chassis_runtime::schema::StateSchemaVersion::new(1),
            vec![],
            chassis_runtime::audio::AudioIoPolicy::any_structurally_valid(),
            vec![],
            vec![
                chassis_runtime::parameters::ParameterDescriptor::float(
                    "gain", "Gain", 0.0, 1.0, 0.0,
                )
                .unwrap()
                .with_global_modulation()
                .unwrap(),
            ],
        )
    }

    fn activate(
        &self,
        _config: &ActivationConfig<'_>,
        parameters: &chassis_runtime::parameters::ParameterStore,
        _custom_state: &[chassis_runtime::state::StateEntry],
    ) -> Result<Self::Processor, Self::ActivationError> {
        let panic = self.panic_next_activation.swap(false, Ordering::Relaxed);
        assert!(
            !panic || STAGE != FACTORY_PANIC,
            "intentional activation factory panic"
        );
        Ok(ActivationPanicProcessor {
            panic_stage: panic.then_some(STAGE),
            gain: parameters.float_parameter("gain").unwrap(),
        })
    }
}

impl<const STAGE: u8> ClapComponent for ActivationPanicProbe<STAGE> {
    const CLAP_DESCRIPTOR: chassis_clap::ClapDescriptor = chassis_clap::ClapDescriptor {
        features: &[c"utility"],
        version: env!("CARGO_PKG_VERSION"),
        ..chassis_clap::ClapDescriptor::new(
            "org.nijaru.chassis.activation-panic-probe",
            "Chassis Activation Panic Probe",
        )
    };
    const CLAP_AUDIO_PORTS: &'static [chassis_clap::ClapAudioPort] = &[];
    const CLAP_PARAMETER_IDS: &'static [(&'static str, u32)] = &[("gain", 7)];
    const CLAP_MAX_PARAMETER_EVENTS: u32 = 8;
}

struct ActivationPanicProcessor {
    panic_stage: Option<u8>,
    gain: chassis_runtime::parameters::FloatParameter,
}
impl Processor for ActivationPanicProcessor {
    fn latency(&self) -> chassis_runtime::runtime::LatencySamples {
        assert_ne!(
            self.panic_stage,
            Some(LATENCY_PANIC),
            "intentional activation latency panic"
        );
        chassis_runtime::runtime::LatencySamples::new(7)
    }
    fn tail(&self) -> chassis_runtime::runtime::TailLength {
        assert_ne!(
            self.panic_stage,
            Some(TAIL_PANIC),
            "intentional activation tail panic"
        );
        chassis_runtime::runtime::TailLength::FiniteSamples(11)
    }
}
impl<S> Process<S> for ActivationPanicProcessor {
    fn process(
        &mut self,
        block: &mut ProcessBlock<'_, S>,
    ) -> Result<(), chassis_runtime::runtime::ProcessError> {
        assert_eq!(
            self.gain.value(block.parameters()).unwrap().to_bits(),
            0.5_f64.to_bits()
        );
        assert_eq!(
            self.gain
                .cursor(block)
                .unwrap()
                .value_at(0)
                .unwrap()
                .to_bits(),
            0.75_f64.to_bits(),
            "held modulation survives failed activation"
        );
        ACTIVATION_DSP_CALLS.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}
impl Drop for ActivationPanicProcessor {
    fn drop(&mut self) {
        ACTIVATION_DROPS.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn activation_factory_and_metadata_panics_preserve_state_and_permit_retry() {
    for (entry, constructed) in
        [
            (
                PluginEntry::load_from_clack::<
                    SingleComponentEntry<ActivationPanicProbe<FACTORY_PANIC>>,
                >(c"")
                .unwrap(),
                false,
            ),
            (
                PluginEntry::load_from_clack::<
                    SingleComponentEntry<ActivationPanicProbe<LATENCY_PANIC>>,
                >(c"")
                .unwrap(),
                true,
            ),
            (
                PluginEntry::load_from_clack::<
                    SingleComponentEntry<ActivationPanicProbe<TAIL_PANIC>>,
                >(c"")
                .unwrap(),
                true,
            ),
        ]
    {
        assert_activation_failure(&entry, constructed);
    }
}

fn assert_activation_failure(entry: &PluginEntry, constructed: bool) {
    use clack_extensions::{
        latency::PluginLatency,
        params::PluginParams,
        state::PluginState,
        tail::{PluginTail, TailLength},
    };
    use clack_host::events::event_types::{ParamModEvent, ParamValueEvent};
    ACTIVATION_DROPS.store(0, Ordering::Relaxed);
    ACTIVATION_DSP_CALLS.store(0, Ordering::Relaxed);
    let mut plugin = instance(entry);
    let config = PluginAudioConfiguration {
        sample_rate: 48_000.0,
        min_frames_count: 1,
        max_frames_count: 64,
    };

    let params = plugin
        .plugin_handle()
        .get_extension::<PluginParams>()
        .unwrap();
    let state = plugin
        .plugin_handle()
        .get_extension::<PluginState>()
        .unwrap();
    let latency = plugin
        .plugin_handle()
        .get_extension::<PluginLatency>()
        .unwrap();
    let tail = plugin
        .plugin_handle()
        .get_extension::<PluginTail>()
        .unwrap();
    let mut controls = EventBuffer::new();
    controls.push(&ParamValueEvent::new(
        0,
        ClapId::new(7),
        Pckn::match_all(),
        0.5,
    ));
    controls.push(&ParamModEvent::new(
        0,
        ClapId::new(7),
        Pckn::match_all(),
        0.25,
    ));
    params.flush(
        &mut plugin.inactive_plugin_handle().unwrap(),
        &controls.as_input(),
        &mut OutputEvents::void(),
    );
    let mut before = Vec::new();
    state.save(&plugin.plugin_handle(), &mut before).unwrap();
    assert_eq!(latency.get(&plugin.plugin_handle()), 0);
    assert_eq!(inactive_tail(&mut plugin), 0);
    assert!(
        plugin
            .activate(|_, _| TestHostAudioProcessor, config)
            .is_err()
    );
    assert!(!plugin.is_active());
    assert_eq!(latency.get(&plugin.plugin_handle()), 0);
    assert_eq!(inactive_tail(&mut plugin), 0);
    assert_eq!(
        ACTIVATION_DROPS.load(Ordering::Relaxed),
        u32::from(constructed)
    );
    assert_eq!(ACTIVATION_DSP_CALLS.load(Ordering::Relaxed), 0);
    assert_eq!(
        params.get_value(&plugin.plugin_handle(), ClapId::new(7)),
        Some(0.5)
    );
    let mut after = Vec::new();
    state.save(&plugin.plugin_handle(), &mut after).unwrap();
    assert_eq!(after, before);
    let mut processor = plugin
        .activate(|_, _| TestHostAudioProcessor, config)
        .unwrap()
        .start_processing()
        .unwrap();
    assert_eq!(latency.get(&plugin.plugin_handle()), 7);
    assert_eq!(tail.get(&processor.plugin_handle()), TailLength::Finite(11));
    assert_eq!(
        process_without_audio(&mut processor),
        clap_sys::process::CLAP_PROCESS_CONTINUE
    );
    assert_eq!(ACTIVATION_DSP_CALLS.load(Ordering::Relaxed), 1);
    plugin.deactivate(processor.stop_processing());
    assert_eq!(
        ACTIVATION_DROPS.load(Ordering::Relaxed),
        u32::from(constructed) + 1
    );
}

#[derive(Default)]
struct ProcessPanicProbe;

impl Component for ProcessPanicProbe {
    type Processor = ProcessPanicProcessor;
    type ActivationError = Infallible;

    fn schema(
        &self,
    ) -> Result<
        chassis_runtime::schema::ComponentSchema,
        chassis_runtime::schema::ComponentSchemaError,
    > {
        chassis_runtime::schema::ComponentSchema::stereo_effect_with_state(
            chassis_runtime::schema::ComponentId::new("org.nijaru.chassis.semantic.crates.chassis-clap.tests.host-panic-containment.processpanicprobe")
                .expect("semantic component identity is valid"),
            chassis_runtime::schema::StateSchemaVersion::new(1),
            vec![],
        )
    }

    fn activate(
        &self,
        _config: &ActivationConfig<'_>,
        _parameters: &chassis_runtime::parameters::ParameterStore,
        _custom_state: &[chassis_runtime::state::StateEntry],
    ) -> Result<Self::Processor, Self::ActivationError> {
        Ok(ProcessPanicProcessor)
    }
}

impl ClapComponent for ProcessPanicProbe {
    const CLAP_DESCRIPTOR: chassis_clap::ClapDescriptor = chassis_clap::ClapDescriptor {
        features: &[c"audio-effect", c"stereo"],
        version: env!("CARGO_PKG_VERSION"),
        ..chassis_clap::ClapDescriptor::new(
            "org.nijaru.chassis.process-panic-probe",
            "Chassis Process Panic Probe",
        )
    };
}

struct ProcessPanicProcessor;

impl Processor for ProcessPanicProcessor {}

impl Process<f32> for ProcessPanicProcessor {
    fn process(
        &mut self,
        _block: &mut ProcessBlock<'_, f32>,
    ) -> Result<(), chassis_runtime::runtime::ProcessError> {
        panic!("intentional process panic");
    }
}

#[test]
fn process_panic_becomes_host_processing_failure_and_deactivates_cleanly() {
    let entry = PluginEntry::load_from_clack::<SingleComponentEntry<ProcessPanicProbe>>(c"")
        .expect("static process panic entry loads");
    let mut plugin = instance(&entry);
    let processor = plugin
        .activate(
            |_, _| TestHostAudioProcessor,
            PluginAudioConfiguration {
                sample_rate: 48_000.0,
                min_frames_count: 1,
                max_frames_count: 64,
            },
        )
        .expect("process panic probe activates");
    let mut processor = processor.start_processing().expect("processing starts");

    let mut main_inputs = [vec![0.0_f32; 64], vec![0.0_f32; 64]];
    let mut sidechain_inputs = [vec![0.0_f32; 64], vec![0.0_f32; 64]];
    let mut outputs = [vec![0.0_f32; 64], vec![0.0_f32; 64]];
    let mut input_ports = AudioPorts::with_capacity(4, 2);
    let mut output_ports = AudioPorts::with_capacity(2, 1);
    let input_audio = input_ports.with_input_buffers([
        AudioPortBuffer {
            channels: AudioPortBufferType::f32_input_only(
                main_inputs.iter_mut().map(InputChannel::variable),
            ),
            latency: 0,
        },
        AudioPortBuffer {
            channels: AudioPortBufferType::f32_input_only(
                sidechain_inputs.iter_mut().map(InputChannel::variable),
            ),
            latency: 0,
        },
    ]);
    let mut output_audio = output_ports.with_output_buffers([AudioPortBuffer {
        channels: AudioPortBufferType::f32_output_only(outputs.iter_mut().map(Vec::as_mut_slice)),
        latency: 0,
    }]);
    let input_events = EventBuffer::with_capacity(0);
    let mut output_events = EventBuffer::with_capacity(0);

    assert!(
        processor
            .process(
                &input_audio,
                &mut output_audio,
                &input_events.as_input(),
                &mut output_events.as_output(),
                None,
                None,
            )
            .is_err()
    );

    plugin.deactivate(processor.stop_processing());
}

static RESET_CALLS: AtomicU32 = AtomicU32::new(0);
static RESET_DSP_CALLS: AtomicU32 = AtomicU32::new(0);
static RESET_DROPS: AtomicU32 = AtomicU32::new(0);

struct ResetPanicProbe<const TAIL: bool>(AtomicBool);
impl<const TAIL: bool> Default for ResetPanicProbe<TAIL> {
    fn default() -> Self {
        Self(AtomicBool::new(true))
    }
}
impl<const TAIL: bool> Component for ResetPanicProbe<TAIL> {
    type Processor = ResetPanicProcessor<TAIL>;
    type ActivationError = Infallible;
    fn schema(
        &self,
    ) -> Result<
        chassis_runtime::schema::ComponentSchema,
        chassis_runtime::schema::ComponentSchemaError,
    > {
        chassis_runtime::schema::ComponentSchema::new(
            chassis_runtime::schema::ComponentId::new("test.reset-panic").unwrap(),
            chassis_runtime::schema::StateSchemaVersion::new(1),
            vec![],
            chassis_runtime::audio::AudioIoPolicy::any_structurally_valid(),
            vec![],
            vec![],
        )
    }
    fn activate(
        &self,
        _: &ActivationConfig<'_>,
        _: &chassis_runtime::parameters::ParameterStore,
        _: &[chassis_runtime::state::StateEntry],
    ) -> Result<Self::Processor, Infallible> {
        Ok(ResetPanicProcessor {
            panic: self.0.swap(false, Ordering::Relaxed),
            reset: false,
        })
    }
}
impl<const TAIL: bool> ClapComponent for ResetPanicProbe<TAIL> {
    const CLAP_DESCRIPTOR: chassis_clap::ClapDescriptor =
        chassis_clap::ClapDescriptor::new("test.reset-panic", "Reset Panic");
    const CLAP_AUDIO_PORTS: &'static [chassis_clap::ClapAudioPort] = &[];
}
struct ResetPanicProcessor<const TAIL: bool> {
    panic: bool,
    reset: bool,
}
impl<const TAIL: bool> Processor for ResetPanicProcessor<TAIL> {
    fn reset(&mut self) {
        RESET_CALLS.fetch_add(1, Ordering::Relaxed);
        self.reset = true;
        assert!(
            !self.panic || TAIL,
            "intentional reset panic after history mutation"
        );
    }
    fn tail(&self) -> chassis_runtime::runtime::TailLength {
        assert!(
            !self.panic || !self.reset || !TAIL,
            "intentional post-reset tail panic"
        );
        chassis_runtime::runtime::TailLength::FiniteSamples(if self.reset { 3 } else { 7 })
    }
}
impl<S, const TAIL: bool> Process<S> for ResetPanicProcessor<TAIL> {
    fn process(
        &mut self,
        _: &mut ProcessBlock<'_, S>,
    ) -> Result<(), chassis_runtime::runtime::ProcessError> {
        RESET_DSP_CALLS.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}
impl<const TAIL: bool> Drop for ResetPanicProcessor<TAIL> {
    fn drop(&mut self) {
        RESET_DROPS.fetch_add(1, Ordering::Relaxed);
    }
}

#[allow(unsafe_code)]
fn inactive_tail(plugin: &mut PluginInstance<TestHostHandlers>) -> u32 {
    let handle = plugin.plugin_handle();
    let raw = handle.as_raw();
    // SAFETY: initialized live instance, main-thread query of Chassis's shared
    // tail snapshot. The pinned host helper incorrectly requires an audio handle.
    unsafe {
        let extension =
            raw.get_extension.unwrap()(raw, clap_sys::ext::tail::CLAP_EXT_TAIL.as_ptr());
        assert!(!extension.is_null());
        let extension = &*extension.cast::<clap_sys::ext::tail::clap_plugin_tail>();
        extension.get.unwrap()(raw)
    }
}

#[allow(unsafe_code)]
fn process_without_audio(processor: &mut StartedPluginAudioProcessor<TestHostHandlers>) -> i32 {
    // Clack's convenience host process derives zero frames from empty buffers;
    // an event-only component still needs a real, nonempty CLAP process interval.
    let input = InputEvents::empty();
    let mut output = OutputEvents::void();
    let process = clap_sys::process::clap_process {
        steady_time: -1,
        frames_count: 8,
        transport: std::ptr::null(),
        audio_inputs: std::ptr::null(),
        audio_outputs: std::ptr::null_mut(),
        audio_inputs_count: 0,
        audio_outputs_count: 0,
        in_events: input.as_raw(),
        out_events: output.as_raw(),
    };
    let handle = processor.plugin_handle();
    let raw = handle.as_raw();
    // SAFETY: this thread exclusively owns the started processor. There are no
    // audio ports, and both event lists remain live for this synchronous call.
    unsafe { raw.process.unwrap()(raw, &raw const process) }
}

#[test]
fn reset_panics_disable_dsp_and_restart_until_fresh_activation_in_both_wrappers() {
    use clack_extensions::tail::{PluginTail, TailLength};
    for entry in [
        PluginEntry::load_from_clack::<SingleComponentEntry<ResetPanicProbe<false>>>(c"").unwrap(),
        PluginEntry::load_from_clack::<SingleComponentEntry<ResetPanicProbe<true>>>(c"").unwrap(),
        PluginEntry::load_from_clack::<
            SingleComponentEntry<ResetPanicProbe<false>, chassis_clap::F32AndF64>,
        >(c"")
        .unwrap(),
        PluginEntry::load_from_clack::<
            SingleComponentEntry<ResetPanicProbe<true>, chassis_clap::F32AndF64>,
        >(c"")
        .unwrap(),
    ] {
        RESET_CALLS.store(0, Ordering::Relaxed);
        RESET_DSP_CALLS.store(0, Ordering::Relaxed);
        RESET_DROPS.store(0, Ordering::Relaxed);
        let mut plugin = instance(&entry);
        let config = PluginAudioConfiguration {
            sample_rate: 48_000.0,
            min_frames_count: 1,
            max_frames_count: 64,
        };
        let tail = plugin
            .plugin_handle()
            .get_extension::<PluginTail>()
            .unwrap();
        let mut processor = plugin
            .activate(|_, _| TestHostAudioProcessor, config)
            .unwrap()
            .start_processing()
            .unwrap();
        assert_eq!(tail.get(&processor.plugin_handle()), TailLength::Finite(7));
        processor.reset();
        assert_eq!(tail.get(&processor.plugin_handle()), TailLength::Finite(7));
        assert_eq!(
            process_without_audio(&mut processor),
            clap_sys::process::CLAP_PROCESS_ERROR
        );
        assert_eq!(RESET_DSP_CALLS.load(Ordering::Relaxed), 0);
        processor.reset();
        assert_eq!(
            RESET_CALLS.load(Ordering::Relaxed),
            1,
            "another reset cannot re-enter damaged product code"
        );
        let failure = processor
            .stop_processing()
            .start_processing()
            .err()
            .expect("failed reset must also reject start");
        assert_eq!(
            RESET_DROPS.load(Ordering::Relaxed),
            0,
            "failure callbacks must retain ownership until deactivation"
        );
        plugin.deactivate(failure.into_stopped_processor());
        assert_eq!(RESET_DROPS.load(Ordering::Relaxed), 1);
        let mut fresh = plugin
            .activate(|_, _| TestHostAudioProcessor, config)
            .unwrap()
            .start_processing()
            .unwrap();
        assert_eq!(
            process_without_audio(&mut fresh),
            clap_sys::process::CLAP_PROCESS_CONTINUE
        );
        fresh.reset();
        assert_eq!(tail.get(&fresh.plugin_handle()), TailLength::Finite(3));
        assert_eq!(
            process_without_audio(&mut fresh),
            clap_sys::process::CLAP_PROCESS_CONTINUE
        );
        assert_eq!(RESET_DSP_CALLS.load(Ordering::Relaxed), 2);
        plugin.deactivate(fresh.stop_processing());
        assert_eq!(RESET_DROPS.load(Ordering::Relaxed), 2);
    }
}
