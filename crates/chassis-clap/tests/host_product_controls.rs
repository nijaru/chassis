//! Public-consumer proof of the opt-in product controller through a real CLAP entry.
//! No controller registry or production test endpoint is used: scripts own their
//! tokens, the host drives callbacks/flushes, and audio exposes consumed controls.

#![allow(
    clippy::float_cmp,
    reason = "Exact copied coordinates and specified one-step audio values are the contract"
)]

use std::{
    cell::Cell,
    convert::Infallible,
    io::{self, Cursor, Read},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc::{Receiver, SyncSender, sync_channel},
    },
    time::Duration,
};

use chassis_clap::{
    ClapComponent, ClapControlError, ClapController, ClapControls, ClapNotePort, F32AndF64,
    F32Only, SingleComponentEntry,
};
use chassis_runtime::{
    audio::{AudioIoPolicy, DEFAULT_EFFECT_PORTS},
    automation::ParameterEventValue,
    controls::{ControlError, ControlValue, GestureStatus, GestureToken},
    events::{EventDialect, EventPortDescriptor, EventPortDirection, EventPortIndex, EventPortKey},
    parameters::{
        FloatMapping, ParameterControlError, ParameterDescriptor, ParameterIndex, ParameterStore,
    },
    process::{ActivationConfig, ProcessBlock},
    runtime::{Component, ComponentStateError, Process, Processor, TailLength},
    schema::{ComponentId, ComponentSchema, ComponentSchemaError, StateSchemaVersion},
    state::{StateDocument, StateEntry, StateValue},
};
use clack_extensions::{
    log::{HostLog, HostLogImpl, LogSeverity},
    params::{
        HostParams, HostParamsImplMainThread, HostParamsImplShared, ParamClearFlags,
        ParamRescanFlags, PluginParams,
    },
    state::PluginState,
};
use clack_host::{
    events::{
        UnknownEvent,
        event_types::{MidiEvent, ParamGestureBeginEvent, ParamGestureEndEvent, ParamValueEvent},
        io::{InputEvents, OutputEventBuffer, OutputEvents, TryPushError},
    },
    factory::plugin::PluginFactory,
    prelude::*,
};
use clack_plugin::plugin::PluginError;

const GAIN: u32 = 17;
const SWITCH: u32 = 41;
const COMPONENT_ID: &str = "test.product-controls.semantic";
const HISTORY: u8 = 0;
const FULL: u8 = 1;
const LOAD: u8 = 2;
const LIFECYCLE: u8 = 3;
const CUSTOM: u8 = 4;
const PANIC: u8 = 5;
const UNAVAILABLE: u8 = 6;
const MALFORMED: u8 = 7;
const STALE_BOUND: u8 = 8;
const RECOVER_LOAD: u8 = 9;
const MIDI_INPUT: EventPortKey = EventPortKey::new("trigger.in");
const MIDI_OUTPUT: EventPortKey = EventPortKey::new("trigger.out");

// Count only the actual consumer call, not component/controller setup or host
// preparation. TLS isolates concurrent tests and the guard restores on unwind.
mod allocation {
    use super::Cell;
    use std::alloc::{GlobalAlloc, Layout, System};
    thread_local! {
        static ENABLED: Cell<bool> = const { Cell::new(false) };
        static COUNTS: Cell<(usize, usize)> = const { Cell::new((0, 0)) };
    }
    fn count(allocation: bool) {
        if ENABLED.try_with(Cell::get).unwrap_or(false) {
            COUNTS.with(|counts| {
                let (alloc, dealloc) = counts.get();
                counts.set((
                    alloc + usize::from(allocation),
                    dealloc + usize::from(!allocation),
                ));
            });
        }
    }
    struct Allocator;
    #[allow(unsafe_code)]
    // SAFETY: instrumentation forwards every original allocation contract to System.
    unsafe impl GlobalAlloc for Allocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            count(true);
            // SAFETY: unchanged caller-supplied valid layout.
            unsafe { System.alloc(layout) }
        }
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            count(true);
            // SAFETY: unchanged caller-supplied valid layout.
            unsafe { System.alloc_zeroed(layout) }
        }
        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            count(false);
            // SAFETY: unchanged live pointer/layout pair.
            unsafe { System.dealloc(pointer, layout) }
        }
        unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            count(true);
            count(false);
            // SAFETY: unchanged live allocation and requested replacement size.
            unsafe { System.realloc(pointer, layout, size) }
        }
    }
    #[global_allocator]
    static ALLOCATOR: Allocator = Allocator;
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            ENABLED.with(|enabled| enabled.set(false));
        }
    }
    pub fn none(f: impl FnOnce()) {
        COUNTS.with(|counts| counts.set((0, 0)));
        ENABLED.with(|enabled| assert!(!enabled.replace(true)));
        let guard = Guard;
        f();
        drop(guard);
        assert_eq!(
            COUNTS.with(Cell::get),
            (0, 0),
            "consumer allocated or reclaimed storage"
        );
    }
}

// A nested render restores its caller's phase. This guards the real callback,
// including adapter work after the core tail hook, rather than just DSP helpers.
thread_local! {
    static TAIL_COMPLETED: Cell<Option<bool>> = const { Cell::new(None) };
}
struct RenderScope(Option<bool>);
impl RenderScope {
    fn enter() -> Self {
        Self(TAIL_COMPLETED.with(|phase| phase.replace(Some(false))))
    }
}
impl Drop for RenderScope {
    fn drop(&mut self) {
        TAIL_COMPLETED.with(|phase| phase.set(self.0));
    }
}

#[derive(Debug, PartialEq)]
struct Plateau;
impl FloatMapping for Plateau {
    fn validate(&self, minimum: f64, maximum: f64) -> Result<(), ParameterControlError> {
        assert_eq!((minimum, maximum), (0.0, 1.0));
        Ok(())
    }
    fn normalized_to_plain(&self, normalized: f64) -> f64 {
        assert_ne!(
            TAIL_COMPLETED.with(Cell::get),
            Some(true),
            "forward mapping ran after successful DSP/tail completion"
        );
        if normalized < 0.1 {
            normalized * 5.0
        } else if normalized > 0.9 {
            (normalized - 0.9) * 5.0 + 0.5
        } else {
            0.5
        }
    }
    fn plain_to_normalized(&self, plain: f64) -> Result<f64, ParameterControlError> {
        Ok(if plain < 0.5 {
            plain / 5.0
        } else if plain > 0.5 {
            (plain - 0.5) / 5.0 + 0.9
        } else {
            0.5
        })
    }
}
#[derive(Default)]
struct Probe;
fn bank(custom: &[StateEntry]) -> Option<u8> {
    let [entry] = custom else {
        return None;
    };
    match entry.value() {
        StateValue::Signed(value) if entry.key() == "state/bank" && (0..=1).contains(value) => {
            u8::try_from(*value).ok()
        }
        _ => None,
    }
}
impl Component for Probe {
    type Processor = Dsp;
    type ActivationError = Infallible;
    fn schema(&self) -> Result<ComponentSchema, ComponentSchemaError> {
        ComponentSchema::new(
            ComponentId::new(COMPONENT_ID).unwrap(),
            StateSchemaVersion::new(1),
            DEFAULT_EFFECT_PORTS.to_vec(),
            AudioIoPolicy::stereo_effect(),
            vec![
                EventPortDescriptor::new(
                    MIDI_OUTPUT,
                    "Trigger output",
                    EventPortDirection::Output,
                    &[EventDialect::Midi1],
                ),
                EventPortDescriptor::new(
                    MIDI_INPUT,
                    "Trigger input",
                    EventPortDirection::Input,
                    &[EventDialect::Midi1],
                ),
            ],
            vec![
                ParameterDescriptor::float("gain", "Gain", 0.0, 1.0, 0.5)
                    .unwrap()
                    .with_float_mapping(Plateau)
                    .unwrap(),
                ParameterDescriptor::boolean("switch", "Switch", false).unwrap(),
            ],
        )
    }
    fn default_custom_state(&self) -> Vec<StateEntry> {
        vec![StateEntry::new("state/bank", StateValue::Signed(0))]
    }
    fn validate_custom_state(&self, custom: &[StateEntry]) -> Result<(), ComponentStateError> {
        // A deliberately unadmitted payload exercises the plugin's Rust panic
        // fence. Valid bank 0/1 states remain operational for every parameter.
        if custom
            .iter()
            .any(|entry| entry.key() == "state/bank" && entry.value() == &StateValue::Signed(99))
        {
            panic!("intentional product state validation panic");
        }
        if bank(custom).is_some() {
            Ok(())
        } else {
            Err(ComponentStateError::new(io::Error::other(
                "bank must be 0 or 1",
            )))
        }
    }
    fn activate(
        &self,
        config: &ActivationConfig<'_>,
        parameters: &ParameterStore,
        custom: &[StateEntry],
    ) -> Result<Dsp, Infallible> {
        let gain = parameters.index("gain").unwrap();
        Ok(Dsp {
            gain,
            bank: bank(custom).unwrap(),
            activation_coordinate: parameters.normalized_coordinate(gain).unwrap(),
            midi_output: config.schema().event_port_index(&MIDI_OUTPUT).unwrap(),
        })
    }
}
struct Dsp {
    gain: ParameterIndex,
    bank: u8,
    activation_coordinate: Option<f64>,
    midi_output: EventPortIndex,
}
impl Processor for Dsp {
    fn tail(&self) -> TailLength {
        TAIL_COMPLETED.with(|phase| {
            if phase.get().is_some() {
                phase.set(Some(true));
            }
        });
        TailLength::ZERO
    }
}
impl Dsp {
    fn process_samples<S: Copy>(
        &mut self,
        block: &mut ProcessBlock<'_, S>,
        sample: impl Fn(f64) -> S,
    ) -> Result<(), chassis_runtime::runtime::ProcessError> {
        let coordinate = block.parameters().normalized_coordinate(self.gain).unwrap();
        let gain = block
            .parameters()
            .float_cursor(block.parameter_events(), self.gain)
            .unwrap()
            .value_at(0)
            .unwrap();
        // The first block also proves the pair was installed BEFORE activate.
        // Subsequent external ingress is allowed to clear provenance.
        if let Some(expected) = self.activation_coordinate.take() {
            assert_eq!(coordinate, Some(expected));
        }
        block.for_each_channel(|mut channel| {
            let Some(endpoint) = channel.output_endpoint() else {
                return;
            };
            channel
                .output_mut()
                .unwrap()
                .fill(if endpoint.channel() == 0 {
                    sample(gain + f64::from(self.bank))
                } else {
                    sample(coordinate.unwrap_or(-1.0))
                });
        });
        // Passive unless explicitly triggered. Preserve a real input timestamp
        // while routing to an independently resolved semantic output port.
        for event in block.context().midi_events().iter() {
            block.try_push_midi_event(event.with_port(self.midi_output))?;
        }
        Ok(())
    }
}
impl Process<f32> for Dsp {
    #[allow(clippy::cast_possible_truncation)]
    fn process(
        &mut self,
        block: &mut ProcessBlock<'_, f32>,
    ) -> Result<(), chassis_runtime::runtime::ProcessError> {
        self.process_samples(block, |value| value as f32)
    }
}
impl Process<f64> for Dsp {
    fn process(
        &mut self,
        block: &mut ProcessBlock<'_, f64>,
    ) -> Result<(), chassis_runtime::runtime::ProcessError> {
        self.process_samples(block, |value| value)
    }
}
impl ClapComponent for Probe {
    const CLAP_DESCRIPTOR: chassis_clap::ClapDescriptor = chassis_clap::ClapDescriptor {
        features: &[c"audio-effect", c"stereo"],
        version: env!("CARGO_PKG_VERSION"),
        ..chassis_clap::ClapDescriptor::new("test.product-controls.export", "Product controls")
    };
    const CLAP_PARAMETER_IDS: &'static [(&'static str, u32)] =
        &[("gain", GAIN), ("switch", SWITCH)];
    const CLAP_MAX_PARAMETER_EVENTS: u32 = 4;
    const CLAP_NOTE_PORTS: &'static [ClapNotePort] = &[
        ClapNotePort::new(MIDI_INPUT, 91, EventDialect::Midi1),
        ClapNotePort::new(MIDI_OUTPUT, 73, EventDialect::Midi1),
    ];
    const CLAP_MAX_MIDI_EVENTS: u32 = 1;
    const CLAP_MAX_INPUT_SYSEX_BYTES: u32 = 1;
    const CLAP_MAX_OUTPUT_MIDI_EVENTS: u32 = 1;
}

// Deliberately not Default: the controller is constructed with its definition.
struct ScriptController<const SCENARIO: u8> {
    phase: u8,
    token: Option<GestureToken>,
}
fn plain(value: f64) -> ControlValue {
    ControlValue::Plain(ParameterEventValue::Float(value))
}
fn boolean(value: bool) -> ControlValue {
    ControlValue::Plain(ParameterEventValue::Boolean(value))
}
fn observation(
    controls: &ClapControls<'_>,
    parameter: ParameterIndex,
    coordinate: Option<f64>,
    gesture: GestureStatus,
) {
    let observed = controls.observe(parameter).unwrap();
    assert_eq!(observed.parameter(), parameter);
    assert_eq!(observed.normalized_coordinate(), coordinate);
    assert_eq!(observed.gesture(), gesture);
}
impl<const S: u8> ClapController<Probe> for ScriptController<S> {
    const MAX_PENDING_NOTIFICATIONS: u32 = match S {
        FULL | LOAD | PANIC | UNAVAILABLE | MALFORMED | RECOVER_LOAD => 3,
        _ => 8,
    };
    const MAX_DELIVERIES_PER_CALLBACK: u32 = if S == FULL || S == STALE_BOUND {
        2
    } else {
        Self::MAX_PENDING_NOTIFICATIONS
    };
    fn new(_: &Probe) -> Result<Self, PluginError> {
        Ok(Self {
            phase: 0,
            token: None,
        })
    }
    #[expect(
        clippy::too_many_lines,
        reason = "Keep the product script's callback phases and token ownership together"
    )]
    fn on_main_thread(&mut self, controls: &mut ClapControls<'_>) {
        let phase = self.phase;
        self.phase += 1;
        let gain = controls.parameter_index("gain").unwrap();
        let switch = controls.parameter_index("switch").unwrap();
        assert!(controls.parameter_index("not-a-parameter").is_none());
        match (S, phase) {
            (HISTORY | STALE_BOUND, 0) => {
                let token = controls.begin(gain).unwrap();
                controls
                    .change(&token, ControlValue::Normalized(0.2))
                    .unwrap();
                controls
                    .change(&token, ControlValue::Normalized(0.8))
                    .unwrap();
                assert_eq!(
                    controls.observe(gain).unwrap().value(),
                    ParameterEventValue::Float(0.5)
                );
                observation(controls, gain, Some(0.8), GestureStatus::Active);
                controls.end(&token).unwrap();
                controls.edit(switch, boolean(true)).unwrap();
                observation(controls, gain, Some(0.8), GestureStatus::Idle);
            }
            (FULL, 0) => {
                let token = controls.begin(gain).unwrap();
                controls.change(&token, plain(0.25)).unwrap();
                assert_eq!(
                    controls.change(&token, plain(0.75)),
                    Err(ClapControlError::Full)
                );
                assert_eq!(
                    controls.edit(switch, boolean(true)),
                    Err(ClapControlError::Full)
                );
                assert!(matches!(
                    controls.begin(switch),
                    Err(ClapControlError::Full)
                ));
                assert_eq!(
                    controls.observe(gain).unwrap().value(),
                    ParameterEventValue::Float(0.25)
                );
                assert_eq!(
                    controls.observe(switch).unwrap().value(),
                    ParameterEventValue::Boolean(false)
                );
                observation(controls, gain, None, GestureStatus::Active);
                observation(controls, switch, None, GestureStatus::Idle);
                self.token = Some(token);
                controls.request_callback();
            }
            (FULL, 1) => {
                let token = self.token.as_ref().unwrap();
                controls.end(token).unwrap(); // reserved End even before any drain
                assert_eq!(
                    controls.change(token, plain(0.75)),
                    Err(ControlError::InvalidGesture.into())
                );
                assert_eq!(
                    controls.end(token),
                    Err(ControlError::InvalidGesture.into())
                );
                assert_eq!(
                    controls.edit(switch, boolean(true)),
                    Err(ClapControlError::Full)
                );
                assert!(matches!(
                    controls.begin(switch),
                    Err(ClapControlError::Full)
                ));
            }
            (FULL, 2) => {
                let token = controls.begin(gain).unwrap();
                drop(token); // Drop is not a protocol operation.
                observation(controls, gain, None, GestureStatus::Active);
                assert_eq!(controls.close(), 1);
                assert_eq!(controls.close(), 0);
                observation(controls, gain, None, GestureStatus::Idle);
            }
            (LOAD | MALFORMED | LIFECYCLE | CUSTOM | RECOVER_LOAD, 0) => {
                let token = controls.begin(gain).unwrap();
                controls
                    .change(&token, ControlValue::Normalized(0.8))
                    .unwrap();
                self.token = Some(token);
                if S != RECOVER_LOAD {
                    controls.request_callback();
                }
            }
            (LOAD | CUSTOM, 1) => {
                observation(controls, gain, None, GestureStatus::Stale);
                let token = self.token.as_ref().unwrap();
                assert_eq!(
                    controls.change(token, plain(0.75)),
                    Err(ControlError::StateReplaced.into())
                );
                controls.end(token).unwrap();
                if S == CUSTOM {
                    controls.edit(gain, ControlValue::Normalized(0.2)).unwrap();
                }
            }
            (MALFORMED | RECOVER_LOAD, 1) => {
                observation(controls, gain, Some(0.8), GestureStatus::Active);
                let token = self.token.as_ref().unwrap();
                controls
                    .change(token, ControlValue::Normalized(0.2))
                    .unwrap();
                controls.end(token).unwrap();
            }
            (LIFECYCLE, 1) => {
                observation(controls, gain, Some(0.8), GestureStatus::Active);
                controls.request_callback();
            }
            (LIFECYCLE, 2) => {
                observation(controls, gain, None, GestureStatus::Active);
                controls.end(self.token.as_ref().unwrap()).unwrap();
            }
            (PANIC, 0) => {
                self.token = Some(controls.begin(gain).unwrap());
                controls.request_callback();
                panic!("intentional product controller panic");
            }
            (PANIC, _) => panic!("disabled controller resumed"),
            (UNAVAILABLE, 0) => {
                observation(controls, gain, None, GestureStatus::Idle);
                assert!(matches!(
                    controls.begin(gain),
                    Err(ClapControlError::HostUnavailable)
                ));
                assert_eq!(
                    controls.edit(gain, ControlValue::Normalized(0.8)),
                    Err(ClapControlError::HostUnavailable)
                );
                assert_eq!(
                    controls.edit(switch, boolean(true)),
                    Err(ClapControlError::HostUnavailable)
                );
                observation(controls, gain, None, GestureStatus::Idle);
                assert_eq!(
                    controls.observe(gain).unwrap().value(),
                    ParameterEventValue::Float(0.5)
                );
                assert_eq!(
                    controls.observe(switch).unwrap().value(),
                    ParameterEventValue::Boolean(false)
                );
            }
            _ => {}
        }
    }
}

#[derive(Default)]
struct Signals {
    callbacks: AtomicUsize,
    flushes: AtomicUsize,
    restarts: AtomicUsize,
    rescans: AtomicUsize,
    logs: AtomicUsize,
}
struct Shared {
    signals: Arc<Signals>,
    params: bool,
}
struct Main(Arc<Signals>);
struct Audio;
struct Handlers;
impl SharedHandler<'_> for Shared {
    fn request_restart(&self) {
        self.signals.restarts.fetch_add(1, Ordering::Relaxed);
    }
    fn request_process(&self) {}
    fn request_callback(&self) {
        self.signals.callbacks.fetch_add(1, Ordering::Relaxed);
    }
}
impl HostParamsImplShared for Shared {
    fn request_flush(&self) {
        self.signals.flushes.fetch_add(1, Ordering::Relaxed);
    }
}
impl HostLogImpl for Shared {
    fn log(&self, _: LogSeverity, _: &str) {
        self.signals.logs.fetch_add(1, Ordering::Relaxed);
    }
}
impl MainThreadHandler<'_> for Main {}
impl AudioProcessorHandler<'_> for Audio {}
impl HostParamsImplMainThread for Main {
    fn rescan(&self, flags: ParamRescanFlags) {
        assert!(flags.contains(ParamRescanFlags::VALUES));
        self.0.rescans.fetch_add(1, Ordering::Relaxed);
    }
    fn clear(&self, _: ClapId, _: ParamClearFlags) {}
}
impl HostHandlers for Handlers {
    type Shared<'a> = Shared;
    type MainThread<'a> = Main;
    type AudioProcessor<'a> = Audio;
    fn declare_extensions(builder: &mut HostExtensions<Self>, shared: &Shared) {
        builder.register::<HostLog>();
        if shared.params {
            builder.register::<HostParams>();
        }
    }
}
struct Host {
    plugin: PluginInstance<Handlers>,
    signals: Arc<Signals>,
    params: PluginParams,
    state: PluginState,
    callbacks_serviced: usize,
}
impl Host {
    fn new<const S: u8>(params_available: bool) -> Self {
        let entry = PluginEntry::load_from_clack::<
            SingleComponentEntry<Probe, F32Only, ScriptController<S>>,
        >(c"")
        .unwrap();
        Self::from_entry(&entry, params_available)
    }
    fn from_entry(entry: &PluginEntry, params_available: bool) -> Self {
        let descriptor = entry
            .get_factory::<PluginFactory>()
            .unwrap()
            .plugin_descriptor(0)
            .unwrap();
        let signals = Arc::new(Signals::default());
        let shared = Arc::clone(&signals);
        let mut plugin = PluginInstance::new(
            move |()| Shared {
                signals: shared,
                params: params_available,
            },
            |shared: &Shared| Main(Arc::clone(&shared.signals)),
            entry,
            descriptor.id().unwrap(),
            &HostInfo::new("product-controls-fixture", "", "", "").unwrap(),
        )
        .unwrap();
        let params = plugin
            .plugin_handle()
            .get_extension::<PluginParams>()
            .unwrap();
        let state = plugin
            .plugin_handle()
            .get_extension::<PluginState>()
            .unwrap();
        assert_eq!(
            signals.callbacks.load(Ordering::Relaxed),
            1,
            "controller initialization requests host service"
        );
        Self {
            plugin,
            signals,
            params,
            state,
            callbacks_serviced: 0,
        }
    }
    fn callback(&mut self) {
        assert!(
            self.signals.callbacks.load(Ordering::Relaxed) > self.callbacks_serviced,
            "the fixture services only callbacks actually requested by the plugin"
        );
        self.callbacks_serviced += 1;
        self.plugin.call_on_main_thread_callback();
    }
    fn no_panics(&self) {
        assert_eq!(
            self.signals.logs.load(Ordering::Relaxed),
            0,
            "Clack must not hide a script assertion panic"
        );
    }
    fn flush(&mut self, input: &InputEvents<'_>, output: &mut impl OutputEventBuffer) {
        let mut inactive = self.plugin.inactive_plugin_handle().unwrap();
        allocation::none(|| {
            self.params
                .flush(&mut inactive, input, &mut OutputEvents::from_buffer(output));
        });
    }
    fn drain(&mut self, limit: usize) -> Sink {
        let mut sink = Sink::new(limit);
        self.flush(&InputEvents::empty(), &mut sink);
        sink
    }
    fn save(&mut self) -> Vec<u8> {
        let mut bytes = Vec::new();
        self.state
            .save(&self.plugin.plugin_handle(), &mut bytes)
            .unwrap();
        bytes
    }
    fn load(&mut self, bytes: &[u8]) {
        self.state
            .load(&self.plugin.plugin_handle(), &mut &*bytes)
            .unwrap();
    }
    fn activate(&mut self) -> StartedPluginAudioProcessor<Handlers> {
        self.stopped(1).start_processing().unwrap()
    }
    fn stopped(&mut self, maximum_frames: u32) -> StoppedPluginAudioProcessor<Handlers> {
        self.plugin
            .activate(
                |_, _| Audio,
                PluginAudioConfiguration {
                    sample_rate: 48_000.0,
                    min_frames_count: 1,
                    max_frames_count: maximum_frames,
                },
            )
            .unwrap()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Record {
    Begin(u32),
    Value(u32, u64),
    End(u32),
}
fn value(id: u32, plain: f64) -> Record {
    Record::Value(id, plain.to_bits())
}
struct Sink {
    records: Vec<Record>,
    limit: usize,
    attempts: usize,
}
impl Sink {
    fn new(limit: usize) -> Self {
        Self {
            records: Vec::with_capacity(16),
            limit,
            attempts: 0,
        }
    }
}
impl OutputEventBuffer for Sink {
    fn try_push(&mut self, event: &UnknownEvent) -> Result<(), TryPushError> {
        self.attempts += 1;
        if self.records.len() == self.limit {
            return Err(TryPushError::new());
        }
        assert_eq!(event.header().time(), 0);
        let record = if let Some(begin) = event.as_event::<ParamGestureBeginEvent>() {
            Record::Begin(begin.param_id().unwrap().get())
        } else if let Some(value) = event.as_event::<ParamValueEvent>() {
            Record::Value(value.param_id().unwrap().get(), value.value().to_bits())
        } else if let Some(end) = event.as_event::<ParamGestureEndEvent>() {
            Record::End(end.param_id().unwrap().get())
        } else {
            panic!("unexpected control event: {event:?}");
        };
        self.records.push(record);
        Ok(())
    }
}
fn history() -> [Record; 7] {
    [
        Record::Begin(GAIN),
        value(GAIN, 0.5),
        value(GAIN, 0.5),
        Record::End(GAIN),
        Record::Begin(SWITCH),
        value(SWITCH, 1.0),
        Record::End(SWITCH),
    ]
}
fn external(id: u32, plain: f64) -> [ParamValueEvent; 1] {
    [ParamValueEvent::new(
        0,
        ClapId::new(id),
        Pckn::match_all(),
        plain,
    )]
}

#[test]
fn accepted_history_survives_every_host_refusal_prefix_without_audio_or_echo() {
    for prefix in 0..7 {
        let mut host = Host::new::<HISTORY>(true);
        host.callback();
        assert_eq!(host.signals.flushes.load(Ordering::Relaxed), 1);
        let first = host.drain(prefix);
        assert_eq!(first.records, history()[..prefix]);
        assert_eq!(first.attempts, prefix + 1);
        assert!(host.signals.callbacks.load(Ordering::Relaxed) > 1);
        // Service the real callback -> request_flush path, without an audio run.
        host.callback();
        assert_eq!(host.signals.flushes.load(Ordering::Relaxed), 2);
        assert_eq!(host.drain(usize::MAX).records, history()[prefix..]);
        assert_eq!(host.drain(usize::MAX).records, []);
        host.no_panics();
    }
}

#[test]
fn full_admission_preserves_authority_and_reserved_end_with_bounded_drain() {
    let mut host = Host::new::<FULL>(true);
    host.callback();
    assert!(host.signals.callbacks.load(Ordering::Relaxed) >= 2);
    allocation::none(|| host.callback()); // reserved End and Full rejections before ANY drain
    assert_eq!(
        host.drain(usize::MAX).records,
        [Record::Begin(GAIN), value(GAIN, 0.25)]
    );
    assert_eq!(host.drain(usize::MAX).records, [Record::End(GAIN)]);
    assert_eq!(
        host.params
            .get_value(&host.plugin.plugin_handle(), ClapId::new(SWITCH)),
        Some(0.0)
    );
    host.callback();
    assert_eq!(
        host.drain(usize::MAX).records,
        [Record::Begin(GAIN), Record::End(GAIN)]
    );
    host.no_panics();
}

#[test]
fn identical_complete_load_revokes_changes_and_preserves_ordered_closure_debt() {
    for prefix in 0..=2 {
        let mut host = Host::new::<LOAD>(true);
        host.callback();
        let before = host.save(); // plain .5, no persisted normalized coordinate
        let delivered = host.drain(prefix);
        let original = [Record::Begin(GAIN), value(GAIN, 0.5)];
        assert_eq!(delivered.records, original[..prefix]);
        host.load(&before); // byte-identical accepted replacement is still a load
        assert_eq!(host.save(), before);
        host.callback();
        let expected = if prefix == 0 {
            vec![Record::Begin(GAIN), Record::End(GAIN)]
        } else {
            vec![Record::End(GAIN)]
        };
        assert_eq!(host.drain(usize::MAX).records, expected);
        assert_eq!(host.signals.rescans.load(Ordering::Relaxed), 1);
        assert_eq!(host.signals.restarts.load(Ordering::Relaxed), 1);
        host.no_panics();
    }
}

#[test]
fn stale_values_count_against_delivery_budget_without_spending_end_debt() {
    let mut host = Host::new::<STALE_BOUND>(true);
    host.callback();
    let same = host.save();
    host.load(&same);
    let first = host.drain(usize::MAX);
    assert_eq!(first.records, [Record::Begin(GAIN)]); // Begin + stale Value = 2
    assert_eq!(first.attempts, 1);
    assert_eq!(host.drain(usize::MAX).records, [Record::End(GAIN)]); // stale Value + End
    assert_eq!(host.drain(usize::MAX).records, [Record::Begin(SWITCH)]);
    assert_eq!(host.drain(usize::MAX).records, [Record::End(SWITCH)]);
    assert_eq!(host.drain(usize::MAX).records, []);
    host.no_panics();
}

#[test]
fn rejected_load_keeps_coordinate_and_live_gesture_authority() {
    let mut host = Host::new::<MALFORMED>(true);
    host.callback();
    assert_eq!(
        host.drain(usize::MAX).records,
        [Record::Begin(GAIN), value(GAIN, 0.5)]
    );
    let before = host.save();
    assert!(
        host.state
            .load(&host.plugin.plugin_handle(), &mut b"malformed".as_slice())
            .is_err()
    );
    assert_eq!(host.save(), before);
    host.callback();
    assert_eq!(
        host.drain(usize::MAX).records,
        [value(GAIN, 0.5), Record::End(GAIN)]
    );
    assert_eq!(host.signals.restarts.load(Ordering::Relaxed), 0);
    assert_eq!(host.signals.rescans.load(Ordering::Relaxed), 0);
    // Clack logs expected extension errors as well as caught panics.
    assert_eq!(host.signals.logs.load(Ordering::Relaxed), 1);
}

#[test]
fn controller_panic_is_logged_disables_script_and_delivers_reserved_closure_inactive() {
    let mut host = Host::new::<PANIC>(true);
    host.callback();
    assert_eq!(host.signals.logs.load(Ordering::Relaxed), 1);
    assert_eq!(host.signals.flushes.load(Ordering::Relaxed), 1);
    assert_eq!(host.drain(0).attempts, 1);
    host.callback();
    assert_eq!(host.drain(1).records, [Record::Begin(GAIN)]);
    host.callback();
    assert_eq!(host.drain(usize::MAX).records, [Record::End(GAIN)]);
    assert_eq!(
        host.signals.logs.load(Ordering::Relaxed),
        1,
        "disabled controller is never resumed"
    );
    let mut processor = host.activate();
    assert_eq!(
        render(&mut processor, &InputEvents::empty(), &mut Sink::new(16)),
        [0.5, -1.0]
    );
    host.plugin.deactivate(processor.stop_processing());
    assert_eq!(
        host.signals.logs.load(Ordering::Relaxed),
        1,
        "valid DSP continues after controller failure"
    );
}

#[test]
fn absent_host_params_rejects_local_mutation_but_allows_observation() {
    let mut host = Host::new::<UNAVAILABLE>(false);
    let before = host.save();
    host.callback();
    assert_eq!(host.save(), before);
    assert_eq!(host.signals.flushes.load(Ordering::Relaxed), 0);
    assert_eq!(host.drain(usize::MAX).records, []);
    host.no_panics();
}

trait Sample: Copy + Default {
    fn buffer(channels: *mut *mut Self) -> clap_sys::audio_buffer::clap_audio_buffer;
}
impl Sample for f32 {
    fn buffer(channels: *mut *mut Self) -> clap_sys::audio_buffer::clap_audio_buffer {
        clap_sys::audio_buffer::clap_audio_buffer {
            data32: channels,
            data64: std::ptr::null_mut(),
            channel_count: 2,
            latency: 0,
            constant_mask: 0,
        }
    }
}
impl Sample for f64 {
    fn buffer(channels: *mut *mut Self) -> clap_sys::audio_buffer::clap_audio_buffer {
        clap_sys::audio_buffer::clap_audio_buffer {
            data32: std::ptr::null_mut(),
            data64: channels,
            channel_count: 2,
            latency: 0,
            constant_mask: 0,
        }
    }
}
fn render(
    processor: &mut StartedPluginAudioProcessor<Handlers>,
    input: &InputEvents<'_>,
    sink: &mut impl OutputEventBuffer,
) -> [f32; 2] {
    render_frames::<f32, 1>(processor, input, sink, true).map(|channel| channel[0])
}
// Caller-owned stable pointer tables avoid the pinned host AudioPorts builder's
// multiport Stacked Borrows retag bug. Exactly two stereo input ports (main +
// sidechain), one stereo output, and N initialized frames remain live throughout.
#[allow(unsafe_code)]
fn render_frames<S: Sample, const N: usize>(
    processor: &mut StartedPluginAudioProcessor<Handlers>,
    input: &InputEvents<'_>,
    sink: &mut impl OutputEventBuffer,
    measure_allocations: bool,
) -> [[S; N]; 2] {
    let mut main = [[S::default(); N]; 2];
    let mut sidechain = [[S::default(); N]; 2];
    let mut output = [[S::default(); N]; 2];
    let mut main_pointers = main.each_mut().map(|channel| channel.as_mut_ptr());
    let mut sidechain_pointers = sidechain.each_mut().map(|channel| channel.as_mut_ptr());
    let mut output_pointers = output.each_mut().map(|channel| channel.as_mut_ptr());
    let inputs = [
        S::buffer(main_pointers.as_mut_ptr()),
        S::buffer(sidechain_pointers.as_mut_ptr()),
    ];
    let mut outputs = [S::buffer(output_pointers.as_mut_ptr())];
    let frames = u32::try_from(N).unwrap();
    {
        // SAFETY: all tables/channels remain initialized, disjoint, stable and
        // unreborrowed until process returns. Input tables are read-only.
        let inputs = unsafe { InputAudioBuffers::from_raw_buffers(&inputs, frames) };
        // SAFETY: exclusive output tables reference disjoint N-frame channels.
        let mut outputs = unsafe { OutputAudioBuffers::from_raw_buffers(&mut outputs, frames) };
        let mut process = || {
            let _scope = RenderScope::enter();
            processor
                .process(
                    &inputs,
                    &mut outputs,
                    input,
                    &mut OutputEvents::from_buffer(sink),
                    None,
                    None,
                )
                .unwrap();
        };
        if measure_allocations {
            allocation::none(process);
        } else {
            process();
        }
    }
    output
}

#[test]
fn coordinate_and_gesture_survive_lifecycle_but_equal_external_flush_clears_only_coordinate() {
    let mut host = Host::new::<LIFECYCLE>(true);
    host.callback();
    let mut processor = host.activate();
    let mut sink = Sink::new(16);
    assert_eq!(
        render(&mut processor, &InputEvents::empty(), &mut sink),
        [0.5, 0.8]
    );
    assert_eq!(sink.records, [Record::Begin(GAIN), value(GAIN, 0.5)]);
    processor.reset();
    assert_eq!(
        render(&mut processor, &InputEvents::empty(), &mut Sink::new(16)),
        [0.5, 0.8]
    );
    host.plugin.deactivate(processor.stop_processing());
    let mut processor = host.activate();
    assert_eq!(
        render(&mut processor, &InputEvents::empty(), &mut Sink::new(16)),
        [0.5, 0.8]
    );
    host.callback(); // the token remains Active across both activations and reset
    let input = external(GAIN, 0.5);
    let mut sink = Sink::new(16);
    allocation::none(|| {
        host.params.flush_active(
            &mut processor.plugin_handle(),
            &InputEvents::from_buffer(&input),
            &mut OutputEvents::from_buffer(&mut sink),
        );
    });
    assert!(
        sink.records.is_empty(),
        "external ingress is not echoed even at equal plain value"
    );
    assert_eq!(
        render(&mut processor, &InputEvents::empty(), &mut sink),
        [0.5, -1.0]
    );
    host.callback(); // equality clears provenance, NOT gesture authority
    assert_eq!(
        render(&mut processor, &InputEvents::empty(), &mut sink),
        [0.5, -1.0]
    );
    assert_eq!(sink.records, [Record::End(GAIN)]);
    host.plugin.deactivate(processor.stop_processing());
    host.no_panics();
}

#[test]
fn other_parameter_external_ingress_does_not_supersede_queued_local_history() {
    for process_ingress in [false, true] {
        let mut host = Host::new::<HISTORY>(true);
        host.callback();
        let input = external(SWITCH, 0.0);
        if process_ingress {
            let mut processor = host.activate();
            assert_eq!(
                render(
                    &mut processor,
                    &InputEvents::from_buffer(&input),
                    &mut Sink::new(0)
                ),
                [0.5, 0.8]
            );
            host.plugin.deactivate(processor.stop_processing());
        } else {
            host.flush(&InputEvents::from_buffer(&input), &mut Sink::new(0));
        }
        assert_eq!(
            host.drain(usize::MAX).records,
            [
                Record::Begin(GAIN),
                value(GAIN, 0.5),
                value(GAIN, 0.5),
                Record::End(GAIN),
                Record::Begin(SWITCH),
                Record::End(SWITCH)
            ]
        );
        assert_eq!(
            host.params
                .get_value(&host.plugin.plugin_handle(), ClapId::new(SWITCH)),
            Some(0.0)
        );
        host.no_panics();
    }
}

#[test]
fn active_custom_replacement_freezes_consumed_pair_while_controller_edits_latest_desired() {
    let mut host = Host::new::<CUSTOM>(true);
    host.callback();
    let mut processor = host.activate();
    assert_eq!(
        render(&mut processor, &InputEvents::empty(), &mut Sink::new(16)),
        [0.5, 0.8]
    );
    let mut replacement = StateDocument::new(COMPONENT_ID, 1).unwrap();
    replacement
        .insert(StateEntry::new("parameter/gain", StateValue::Float(0.5)))
        .unwrap();
    replacement
        .insert(StateEntry::new(
            "parameter/switch",
            StateValue::Boolean(false),
        ))
        .unwrap();
    replacement
        .insert(StateEntry::new("state/bank", StateValue::Signed(1)))
        .unwrap();
    host.load(&replacement.encode().unwrap());
    host.callback(); // stale old token can end; new normalized edit affects desired
    let mut sink = Sink::new(16);
    assert_eq!(
        render(&mut processor, &InputEvents::empty(), &mut sink),
        [0.5, 0.8]
    );
    assert_eq!(
        sink.records,
        [
            Record::End(GAIN),
            Record::Begin(GAIN),
            value(GAIN, 0.5),
            Record::End(GAIN)
        ]
    );
    processor.reset();
    assert_eq!(
        render(&mut processor, &InputEvents::empty(), &mut Sink::new(16)),
        [0.5, 0.8]
    );
    host.plugin.deactivate(processor.stop_processing());
    let mut processor = host.activate();
    assert_eq!(
        render(&mut processor, &InputEvents::empty(), &mut Sink::new(16)),
        [1.5, 0.2]
    );
    host.plugin.deactivate(processor.stop_processing());
    assert!(host.signals.restarts.load(Ordering::Relaxed) >= 1);
    host.no_panics();
}

struct CountingInput {
    reads: usize,
}
impl Read for CountingInput {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        self.reads += 1;
        Ok(0)
    }
}
// Reentry is deliberately limited to inactive-main flush. No main-thread-only
// CLAP function is invoked from process or an active audio-domain flush.
struct ReentrantSink<'a> {
    plugin: &'a clap_sys::plugin::clap_plugin,
    params: &'a clap_sys::ext::params::clap_plugin_params,
    state: &'a clap_sys::ext::state::clap_plugin_state,
    sink: Sink,
    reads: usize,
    queries: usize,
}
impl OutputEventBuffer for ReentrantSink<'_> {
    #[allow(unsafe_code)]
    fn try_push(&mut self, event: &UnknownEvent) -> Result<(), TryPushError> {
        let mut nested = Sink::new(16);
        let input = InputEvents::empty();
        let mut output = OutputEvents::from_buffer(&mut nested);
        allocation::none(|| {
            // SAFETY: the same inactive-main lifetime as below. The nested
            // consumer has distinct valid event tables and must stop at the
            // already-held delivery claim without attempting any host push.
            unsafe {
                self.params.flush.unwrap()(self.plugin, input.as_raw(), output.as_raw());
            }
        });
        assert_eq!(
            nested.attempts, 0,
            "Busy consumer must not enter host output"
        );
        assert_eq!(nested.records, []);
        let mut gain = f64::NAN;
        let mut switch = f64::NAN;
        let mut reader = CountingInput { reads: 0 };
        let mut stream = clack_host::stream::InputStream::from_reader(&mut reader);
        // SAFETY: plugin/extension tables borrow the live initialized inactive
        // instance for this synchronous flush. Scalars and stream remain valid
        // for the calls. Reentry queries are read-only; load must reject Busy
        // before touching the stream or attempting any canonical mutation.
        unsafe {
            assert!(self.params.get_value.unwrap()(
                self.plugin,
                GAIN,
                &raw mut gain
            ));
            assert!(self.params.get_value.unwrap()(
                self.plugin,
                SWITCH,
                &raw mut switch
            ));
            assert!(!self.state.load.unwrap()(self.plugin, stream.as_raw_mut()));
        }
        assert_eq!(
            (gain, switch),
            (0.5, 1.0),
            "queries see latest canonical, not the queued prefix"
        );
        self.reads += reader.reads;
        self.queries += 1;
        self.sink.try_push(event)
    }
}

#[test]
#[allow(unsafe_code)]
fn output_push_allows_latest_queries_but_load_is_busy_before_any_stream_io() {
    let mut host = Host::new::<HISTORY>(true);
    host.callback();
    let mut inactive = host.plugin.inactive_plugin_handle().unwrap();
    let plugin = inactive.as_raw();
    // SAFETY: extension pointers belong to this initialized inactive instance;
    // the handle and instance outlive the borrowed tables and synchronous flush.
    let (params, state) = unsafe {
        (
            &*plugin.get_extension.unwrap()(plugin, c"clap.params".as_ptr())
                .cast::<clap_sys::ext::params::clap_plugin_params>(),
            &*plugin.get_extension.unwrap()(plugin, c"clap.state".as_ptr())
                .cast::<clap_sys::ext::state::clap_plugin_state>(),
        )
    };
    let mut sink = ReentrantSink {
        plugin,
        params,
        state,
        sink: Sink::new(0),
        reads: 0,
        queries: 0,
    };
    // Expected Busy is logged by Clack, so this is measured separately from
    // allocation-free ordinary drains (the logger may format the error).
    host.params.flush(
        &mut inactive,
        &InputEvents::empty(),
        &mut OutputEvents::from_buffer(&mut sink),
    );
    assert_eq!(sink.queries, 1);
    assert_eq!(sink.reads, 0);
    assert_eq!(host.signals.rescans.load(Ordering::Relaxed), 0);
    assert_eq!(host.signals.restarts.load(Ordering::Relaxed), 0);
    sink.sink.limit = usize::MAX;
    host.params.flush(
        &mut inactive,
        &InputEvents::empty(),
        &mut OutputEvents::from_buffer(&mut sink),
    );
    assert_eq!(sink.sink.records, history());
    assert_eq!(sink.reads, 0);
    assert_eq!(sink.queries, 8);
    assert_eq!(
        host.signals.logs.load(Ordering::Relaxed),
        sink.queries,
        "only expected Busy errors were caught"
    );
    assert_eq!(host.signals.rescans.load(Ordering::Relaxed), 0);
    assert_eq!(host.signals.restarts.load(Ordering::Relaxed), 0);
}

struct Push<F>(F);
impl<F: FnMut(&UnknownEvent) -> Result<(), TryPushError>> OutputEventBuffer for Push<F> {
    fn try_push(&mut self, event: &UnknownEvent) -> Result<(), TryPushError> {
        (self.0)(event)
    }
}

#[test]
#[allow(unsafe_code)]
fn nested_instance_output_preserves_outer_scope_and_both_instances_notification_debt() {
    let mut a = Host::new::<HISTORY>(true);
    let mut b = Host::new::<HISTORY>(true);
    a.callback();
    b.callback();
    let mut inactive_a = a.plugin.inactive_plugin_handle().unwrap();
    let mut inactive_b = b.plugin.inactive_plugin_handle().unwrap();
    let plugin = inactive_a.as_raw();
    // SAFETY: immutable extension tables borrow the initialized inactive A;
    // both instance handles remain alive through their nested synchronous flushes.
    let (params, state) = unsafe {
        (
            &*plugin.get_extension.unwrap()(plugin, c"clap.params".as_ptr())
                .cast::<clap_sys::ext::params::clap_plugin_params>(),
            &*plugin.get_extension.unwrap()(plugin, c"clap.state".as_ptr())
                .cast::<clap_sys::ext::state::clap_plugin_state>(),
        )
    };
    let make_sink = |limit| ReentrantSink {
        plugin,
        params,
        state,
        sink: Sink::new(limit),
        reads: 0,
        queries: 0,
    };
    // B's output reenters A.load; after B returns, A's own output reenters
    // A.load again. Restoring the previous TLS scope, not simply clearing it,
    // is essential to reject both calls before reading either stream.
    let mut from_b = make_sink(usize::MAX);
    let mut from_a = make_sink(0);
    {
        let mut nested = Push(|event: &UnknownEvent| {
            b.params.flush(
                &mut inactive_b,
                &InputEvents::empty(),
                &mut OutputEvents::from_buffer(&mut from_b),
            );
            from_a.try_push(event)
        });
        a.params.flush(
            &mut inactive_a,
            &InputEvents::empty(),
            &mut OutputEvents::from_buffer(&mut nested),
        );
    }
    assert_eq!(from_b.sink.records, history());
    assert_eq!(from_b.reads + from_a.reads, 0);
    assert_eq!(from_a.sink.records, []);
    from_a.sink.limit = usize::MAX;
    a.params.flush(
        &mut inactive_a,
        &InputEvents::empty(),
        &mut OutputEvents::from_buffer(&mut from_a),
    );
    assert_eq!(
        from_a.sink.records,
        history(),
        "A retained its complete refused suffix"
    );
    assert_eq!(from_a.reads, 0);
    assert_eq!(
        a.signals.logs.load(Ordering::Relaxed),
        from_a.queries + from_b.queries
    );
    assert_eq!(a.signals.rescans.load(Ordering::Relaxed), 0);
    assert_eq!(a.signals.restarts.load(Ordering::Relaxed), 0);
    b.no_panics();
}

struct ReenteringRead<'a> {
    plugin: &'a clap_sys::plugin::clap_plugin,
    payload: Cursor<Vec<u8>>,
    fail_io: bool,
    reentries: usize,
}
impl Read for ReenteringRead<'_> {
    #[allow(unsafe_code)]
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.reentries == 0 {
            self.reentries += 1;
            // SAFETY: a synchronous inactive-main load owns this initialized
            // instance. Clack contains the callback; it must defer its borrowed
            // controller, not execute the product script inside stream I/O.
            unsafe {
                self.plugin.on_main_thread.unwrap()(self.plugin);
            }
        }
        // Never panic in a host extern-C stream callback. The panic case is
        // encoded below and raised later by the plugin's Rust validation hook.
        if self.fail_io {
            Err(io::Error::other("intentional host stream failure"))
        } else {
            self.payload.read(buffer)
        }
    }
}
fn bank_document(bank: i64) -> Vec<u8> {
    let mut document = StateDocument::new(COMPONENT_ID, 1).unwrap();
    document
        .insert(StateEntry::new("parameter/gain", StateValue::Float(0.5)))
        .unwrap();
    document
        .insert(StateEntry::new(
            "parameter/switch",
            StateValue::Boolean(false),
        ))
        .unwrap();
    document
        .insert(StateEntry::new("state/bank", StateValue::Signed(bank)))
        .unwrap();
    document.encode().unwrap()
}

#[test]
fn failed_load_reschedules_deferred_controller_on_stream_error_and_product_validation_panic() {
    for fail_io in [true, false] {
        let mut host = Host::new::<RECOVER_LOAD>(true);
        host.callback();
        let before = host.save();
        assert_eq!(
            host.signals.callbacks.load(Ordering::Relaxed),
            host.callbacks_serviced
        );
        {
            let handle = host.plugin.plugin_handle();
            let mut reader = ReenteringRead {
                plugin: handle.as_raw(),
                payload: Cursor::new(bank_document(99)),
                fail_io,
                reentries: 0,
            };
            assert!(host.state.load(&handle, &mut reader).is_err());
            assert_eq!(reader.reentries, 1);
        }
        assert_eq!(host.save(), before, "rejection retains canonical state");
        assert_eq!(host.signals.restarts.load(Ordering::Relaxed), 0);
        assert_eq!(host.signals.rescans.load(Ordering::Relaxed), 0);
        assert_eq!(
            host.signals.logs.load(Ordering::Relaxed),
            1,
            "one expected load rejection or contained product panic"
        );
        // The stream callback consumed main service while the loader held the
        // protocol borrow. Only the loader's failure cleanup can re-request it.
        assert!(host.signals.callbacks.load(Ordering::Relaxed) > host.callbacks_serviced);
        assert_eq!(
            host.drain(usize::MAX).records,
            [Record::Begin(GAIN), value(GAIN, 0.5)]
        );
        host.callback(); // asserts Active/.8, successfully changes and ends old token
        assert_eq!(
            host.drain(usize::MAX).records,
            [value(GAIN, 0.5), Record::End(GAIN)]
        );
        assert_eq!(
            host.signals.logs.load(Ordering::Relaxed),
            1,
            "no hidden assertion panic after recovering loader guards"
        );
    }
}

#[derive(Debug, PartialEq, Eq)]
enum TimedRecord {
    Control(u32, Record),
    Midi(u32, u16, [u8; 3]),
}
struct TimedSink {
    controls: Sink,
    records: Vec<TimedRecord>,
}
impl OutputEventBuffer for TimedSink {
    fn try_push(&mut self, event: &UnknownEvent) -> Result<(), TryPushError> {
        let record = if let Some(midi) = event.as_event::<MidiEvent>() {
            TimedRecord::Midi(event.header().time(), midi.port_index(), midi.data())
        } else {
            self.controls.try_push(event)?;
            TimedRecord::Control(
                event.header().time(),
                *self.controls.records.last().unwrap(),
            )
        };
        self.records.push(record);
        Ok(())
    }
}

#[test]
fn local_control_history_precedes_actual_timed_product_midi_output() {
    let mut host = Host::new::<HISTORY>(true);
    host.callback();
    let mut processor = host.stopped(4).start_processing().unwrap();
    let input = [MidiEvent::new(2, 0, [0x90, 60, 100])];
    let mut sink = TimedSink {
        controls: Sink::new(16),
        records: Vec::with_capacity(8),
    };
    assert_eq!(
        render_frames::<f32, 4>(
            &mut processor,
            &InputEvents::from_buffer(&input),
            &mut sink,
            true
        ),
        [[0.5; 4], [0.8; 4]]
    );
    let mut expected: Vec<_> = history()
        .into_iter()
        .map(|record| TimedRecord::Control(0, record))
        .collect();
    expected.push(TimedRecord::Midi(2, 0, [0x90, 60, 100]));
    assert_eq!(sink.records, expected);
    host.plugin.deactivate(processor.stop_processing());
    host.no_panics();
}

#[test]
fn first_output_on_fresh_then_migrated_audio_threads_is_allocation_free_and_ordered() {
    let mut host = Host::new::<HISTORY>(true);
    host.callback();
    let stopped = host.stopped(1);
    let (stopped, prefix) = std::thread::scope(|scope| {
        scope
            .spawn(move || {
                let mut processor = stopped.start_processing().unwrap();
                let mut sink = Sink::new(3);
                assert_eq!(
                    render(&mut processor, &InputEvents::empty(), &mut sink),
                    [0.5, 0.8]
                );
                (processor.stop_processing(), sink.records)
            })
            .join()
            .unwrap()
    });
    assert_eq!(prefix, history()[..3]);
    // The first worker is joined before moving exclusive processor ownership to
    // a second new worker. No plugin callback or output-scope TLS ran there yet.
    let (stopped, suffix) = std::thread::scope(|scope| {
        scope
            .spawn(move || {
                let mut processor = stopped.start_processing().unwrap();
                let mut sink = Sink::new(16);
                assert_eq!(
                    render(&mut processor, &InputEvents::empty(), &mut sink),
                    [0.5, 0.8]
                );
                (processor.stop_processing(), sink.records)
            })
            .join()
            .unwrap()
    });
    assert_eq!(suffix, history()[3..]);
    host.plugin.deactivate(stopped);
    host.no_panics();
}

struct BlockingSink {
    entered: SyncSender<()>,
    release: Receiver<()>,
    sink: Sink,
}
impl OutputEventBuffer for BlockingSink {
    fn try_push(&mut self, event: &UnknownEvent) -> Result<(), TryPushError> {
        if self.sink.attempts == 0 {
            // This deliberately blocking host is NOT allocation/timing-qualified.
            // Disconnect also releases audio if the main-thread fixture unwinds.
            let _ = self.entered.send(());
            if self.release.recv_timeout(Duration::from_secs(5)).is_err() {
                return Err(TryPushError::new());
            }
        }
        self.sink.try_push(event)
    }
}

#[test]
fn valid_main_load_waits_for_concurrent_audio_push_and_suppresses_remaining_old_values() {
    let mut host = Host::new::<HISTORY>(true);
    let replacement = host.save();
    host.callback();
    let stopped = host.stopped(1);
    let (entered_tx, entered_rx) = sync_channel(1);
    let (release_tx, release_rx) = sync_channel(1);
    let (stopped, delivered, loaded) = std::thread::scope(|scope| {
        let audio = scope.spawn(move || {
            let mut processor = stopped.start_processing().unwrap();
            let mut sink = BlockingSink {
                entered: entered_tx,
                release: release_rx,
                sink: Sink::new(1),
            };
            render_frames::<f32, 1>(&mut processor, &InputEvents::empty(), &mut sink, false);
            (processor.stop_processing(), sink.sink.records)
        });
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let release = scope.spawn(move || {
            // Public APIs expose no waiter-registration hook. Give the main
            // loader a turn, then release regardless of scheduling. Private
            // claim tests own deterministic fairness; no latency claim is made.
            std::thread::sleep(Duration::from_millis(20));
            let _ = release_tx.send(());
        });
        let loaded = host
            .state
            .load(&host.plugin.plugin_handle(), &mut replacement.as_slice());
        release.join().unwrap();
        let (stopped, delivered) = audio.join().unwrap();
        (stopped, delivered, loaded)
    });
    loaded.expect("ordinary concurrent host output must not reject a valid preset as Busy");
    assert_eq!(delivered, [Record::Begin(GAIN)]);
    host.plugin.deactivate(stopped);
    assert_eq!(host.save(), replacement);
    assert_eq!(
        host.drain(usize::MAX).records,
        [
            Record::End(GAIN),
            Record::Begin(SWITCH),
            Record::End(SWITCH)
        ]
    );
    assert_eq!(host.signals.restarts.load(Ordering::Relaxed), 1);
    assert_eq!(host.signals.rescans.load(Ordering::Relaxed), 1);
    host.no_panics();
}

struct MidiBarrierSink {
    entered: SyncSender<()>,
    release: Receiver<()>,
    controls: Sink,
}
impl OutputEventBuffer for MidiBarrierSink {
    fn try_push(&mut self, event: &UnknownEvent) -> Result<(), TryPushError> {
        if event.as_event::<MidiEvent>().is_some() {
            // The product emitted this from DSP, after local history drained.
            // Blocking is deliberately host-owned and not realtime-qualified.
            let _ = self.entered.send(());
            self.release
                .recv_timeout(Duration::from_secs(5))
                .map_err(|_| TryPushError::new())
        } else {
            self.controls.try_push(event)
        }
    }
}

#[test]
fn normalized_edit_during_dsp_is_mapped_next_block_not_after_tail() {
    let mut host = Host::new::<MALFORMED>(true);
    host.callback(); // accepted .8; a later requested callback changes it to .2
    let stopped = host.stopped(1);
    let (entered_tx, entered_rx) = sync_channel(1);
    let (release_tx, release_rx) = sync_channel(1);
    let (stopped, first, delivered) = std::thread::scope(|scope| {
        let audio = scope.spawn(move || {
            let mut processor = stopped.start_processing().unwrap();
            let mut sink = MidiBarrierSink {
                entered: entered_tx,
                release: release_rx,
                controls: Sink::new(16),
            };
            let input = [MidiEvent::new(0, 0, [0x90, 60, 100])];
            let first = render_frames::<f32, 1>(
                &mut processor,
                &InputEvents::from_buffer(&input),
                &mut sink,
                false,
            );
            (processor.stop_processing(), first, sink.controls.records)
        });
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        host.callback(); // genuine main thread; audio is still inside product DSP
        release_tx.send(()).unwrap();
        audio.join().unwrap()
    });
    assert_eq!(first, [[0.5], [0.8]]);
    assert_eq!(delivered, [Record::Begin(GAIN), value(GAIN, 0.5)]);
    let mut processor = stopped.start_processing().unwrap();
    let mut sink = Sink::new(16);
    assert_eq!(
        render(&mut processor, &InputEvents::empty(), &mut sink),
        [0.5, 0.2]
    );
    assert_eq!(sink.records, [value(GAIN, 0.5), Record::End(GAIN)]);
    host.plugin.deactivate(processor.stop_processing());
    host.no_panics();
}

#[test]
fn dual_precision_controlled_entry_consumes_exact_pair_in_real_f64_audio() {
    let entry = PluginEntry::load_from_clack::<
        SingleComponentEntry<Probe, F32AndF64, ScriptController<LIFECYCLE>>,
    >(c"")
    .unwrap();
    let mut host = Host::from_entry(&entry, true);
    host.callback();
    let mut processor = host.activate();
    let mut sink = Sink::new(16);
    assert_eq!(
        render_frames::<f64, 1>(&mut processor, &InputEvents::empty(), &mut sink, true),
        [[0.5], [0.8]]
    );
    assert_eq!(sink.records, [Record::Begin(GAIN), value(GAIN, 0.5)]);
    processor.reset();
    // Unrelated successful plain completion must preserve gain's exact pair.
    let input = external(SWITCH, 0.0);
    assert_eq!(
        render_frames::<f64, 1>(
            &mut processor,
            &InputEvents::from_buffer(&input),
            &mut Sink::new(16),
            true
        ),
        [[0.5], [0.8]]
    );
    host.plugin.deactivate(processor.stop_processing());
    host.no_panics();
}
