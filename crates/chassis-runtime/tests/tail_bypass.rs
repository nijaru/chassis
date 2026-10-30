//! Explicit bypass identity, product-owned delayed-dry policy, and completed tail metadata.

use chassis_runtime::transport::TransportSnapshot;
use chassis_runtime::{
    audio::{AudioPortIndex, DEFAULT_EFFECT_CONFIGURATION},
    automation::{ParameterCursorError, ParameterEvent, ParameterEventValue, ParameterEvents},
    buffer::{ChannelBuffer, InputEndpoint, OutputEndpoint},
    parameters::{
        ParameterDefinitionError, ParameterDescriptor, ParameterIndex, ParameterStore,
        ParameterStoreError, ParameterValue,
    },
    process::{
        ActivationConfig, ProcessBlock, ProcessBufferSource, ProcessChannel, ProcessConfig,
        ProcessContext, ProcessMode,
    },
    runtime::{
        Component, ComponentStateError, InstanceRuntime, LatencySamples, Process, Processor,
        TailLength,
    },
    schema::{ComponentId, ComponentSchema, ComponentSchemaError, StateSchemaVersion},
    state::{StateDocument, StateEntry, StateLimits, StateValue},
};
use std::{
    convert::Infallible,
    num::NonZeroU32,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

const BYPASS: ParameterIndex = ParameterIndex::new(0);

struct Echo {
    next_tail: Arc<AtomicU64>,
    panic_tail: Arc<AtomicBool>,
}
impl Default for Echo {
    fn default() -> Self {
        Self {
            next_tail: Arc::new(AtomicU64::new(5)),
            panic_tail: Arc::new(AtomicBool::new(false)),
        }
    }
}
impl Component for Echo {
    type Processor = EchoProcessor;
    type ActivationError = Infallible;
    fn schema(&self) -> Result<ComponentSchema, ComponentSchemaError> {
        ComponentSchema::stereo_effect_with_state(
            ComponentId::new("test.tail-bypass").unwrap(),
            StateSchemaVersion::new(1),
            vec![
                ParameterDescriptor::boolean("power", "Bypass", false)
                    .unwrap()
                    .with_bypass()
                    .unwrap(),
            ],
        )
    }
    fn default_custom_state(&self) -> Vec<StateEntry> {
        vec![StateEntry::new(
            "state/long_echo",
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
                if entry.key() == "state/long_echo"
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
        custom: &[StateEntry],
    ) -> Result<EchoProcessor, Infallible> {
        Ok(EchoProcessor {
            dry: [[0.0; 2]; 2],
            wet: [[0.0; 7]; 2],
            wet_length: if custom[0].value() == &StateValue::Boolean(true) {
                7
            } else {
                5
            },
            positions: [0; 2],
            next_tail: Arc::clone(&self.next_tail),
            panic_tail: Arc::clone(&self.panic_tail),
        })
    }
}
struct EchoProcessor {
    dry: [[f64; 2]; 2],
    wet: [[f64; 7]; 2],
    wet_length: usize,
    positions: [usize; 2],
    next_tail: Arc<AtomicU64>,
    panic_tail: Arc<AtomicBool>,
}
impl Processor for EchoProcessor {
    fn latency(&self) -> LatencySamples {
        LatencySamples::new(2)
    }
    fn tail(&self) -> TailLength {
        assert!(
            !self.panic_tail.load(Ordering::Relaxed),
            "injected tail query panic"
        );
        // Deliberately adjustable conservative bound for snapshot tests. The
        // actual echo extent is determined by the activated resource length.
        match self.next_tail.load(Ordering::Relaxed) {
            u64::MAX => TailLength::Infinite,
            samples => {
                TailLength::FiniteSamples(samples.max(u64::try_from(self.wet_length).unwrap()))
            }
        }
    }
    fn reset(&mut self) {
        self.dry = [[0.0; 2]; 2];
        self.wet = [[0.0; 7]; 2];
        self.positions = [0; 2];
    }
}
impl Process<f64> for EchoProcessor {
    fn process<B: ProcessBufferSource<f64> + ?Sized>(
        &mut self,
        block: &mut ProcessBlock<'_, '_, '_, f64, B>,
    ) -> Result<(), chassis_runtime::runtime::ProcessError> {
        let Some(ParameterEventValue::Boolean(initial)) =
            block.parameters().get_event_value(BYPASS)
        else {
            unreachable!()
        };
        let events = block.parameter_events();
        for (channel, mut buffer) in block.channels().enumerate() {
            let mut bypass = events.boolean_cursor(BYPASS, initial);
            for (offset, sample) in buffer
                .copy_input_to_output()
                .unwrap()
                .iter_mut()
                .enumerate()
            {
                let position = self.positions[channel];
                let dry = &mut self.dry[channel][position % 2];
                let wet = &mut self.wet[channel][position % self.wet_length];
                let output = *dry + *wet;
                *dry = *sample;
                // Product policy: preserve two-sample dry latency and let old
                // wet history finish, but stop injecting new wet input on bypass.
                *wet = if bypass.value_at(u32::try_from(offset).unwrap()).unwrap() {
                    0.0
                } else {
                    *sample * 0.5
                };
                *sample = output;
                self.positions[channel] = (position + 1) % (2 * self.wet_length);
            }
        }
        Ok(())
    }
}
fn config() -> ProcessConfig {
    ProcessConfig::new(48_000.0, NonZeroU32::new(1), NonZeroU32::new(8).unwrap(), 8).unwrap()
}
fn active(effect: &Echo) -> InstanceRuntime<EchoProcessor> {
    let mut runtime = InstanceRuntime::for_component(effect).unwrap();
    runtime
        .activate(effect, config(), DEFAULT_EFFECT_CONFIGURATION)
        .unwrap();
    runtime
}
fn render(
    runtime: &mut InstanceRuntime<EchoProcessor>,
    input: [f64; 8],
    events: &[ParameterEvent],
) -> Result<[f64; 8], chassis_runtime::runtime::InstanceProcessError> {
    let mut left = input;
    let mut right = input;
    let mut channels = [
        ChannelBuffer::in_place(
            InputEndpoint::new(AudioPortIndex::new(0), 0),
            OutputEndpoint::new(AudioPortIndex::new(1), 0),
            &mut left,
            8,
        )
        .unwrap(),
        ChannelBuffer::in_place(
            InputEndpoint::new(AudioPortIndex::new(0), 1),
            OutputEndpoint::new(AudioPortIndex::new(1), 1),
            &mut right,
            8,
        )
        .unwrap(),
    ];
    runtime.process(
        8,
        ProcessContext::new(
            ProcessMode::Realtime,
            &TransportSnapshot::UNKNOWN,
            ParameterEvents::new(events, 8, 8).unwrap(),
        ),
        &mut channels,
    )?;
    assert_eq!(left.map(f64::to_bits), right.map(f64::to_bits));
    Ok(left)
}

#[test]
fn bypass_is_explicit_boolean_unique_schema_identity() {
    let ordinary = ParameterDescriptor::boolean("bypass", "Bypass", false).unwrap();
    assert!(!ordinary.is_bypass());
    let bypass = ordinary.clone().with_bypass().unwrap();
    assert!(bypass.is_bypass());
    assert_ne!(ordinary, bypass);
    let other = ParameterDescriptor::boolean("power", "Power", true)
        .unwrap()
        .with_bypass()
        .unwrap();
    assert!(matches!(
        ParameterStore::new(&[bypass, other]),
        Err(ParameterStoreError::InvalidDefinition(
            ParameterDefinitionError::DuplicateBypass(_)
        ))
    ));
    for descriptor in [
        ParameterDescriptor::float("x", "X", 0.0, 1.0, 0.0).unwrap(),
        ParameterDescriptor::integer("x", "X", 0, 1, 0).unwrap(),
    ] {
        assert!(matches!(
            descriptor.with_bypass(),
            Err(ParameterDefinitionError::UnsupportedBypass(_))
        ));
    }
}

#[test]
fn boolean_cursor_preserves_source_order_and_rejects_invalid_direct_use() {
    let other = ParameterIndex::new(1);
    let events = [
        ParameterEvent::set(1, BYPASS, ParameterEventValue::Boolean(true)),
        ParameterEvent::modulation(2, other, 0.5),
        ParameterEvent::set(3, BYPASS, ParameterEventValue::Boolean(false)),
        ParameterEvent::set(3, BYPASS, ParameterEventValue::Boolean(true)),
        ParameterEvent::set(4, BYPASS, ParameterEventValue::Boolean(false)),
    ];
    let events = ParameterEvents::new(&events, 5, 5).unwrap();
    let mut cursor = events.boolean_cursor(BYPASS, false);
    for (offset, expected) in [false, true, true, true, false].into_iter().enumerate() {
        assert_eq!(
            cursor.value_at(u32::try_from(offset).unwrap()),
            Ok(expected)
        );
    }
    assert!(matches!(
        cursor.value_at(3),
        Err(ParameterCursorError::NonMonotonicQuery { .. })
    ));
    assert!(matches!(
        cursor.value_at(5),
        Err(ParameterCursorError::OffsetOutOfRange { .. })
    ));
    let bad = [ParameterEvent::linear(0, BYPASS, 1.0)];
    assert_eq!(
        ParameterEvents::new(&bad, 1, 1)
            .unwrap()
            .boolean_cursor(BYPASS, false)
            .value_at(0),
        Err(ParameterCursorError::NonBooleanValue { index: 0 })
    );
}

#[test]
fn product_bypass_preserves_dry_latency_and_existing_wet_tail() {
    let effect = Echo::default();
    let mut runtime = active(&effect);
    // The first impulse excites the echo. Bypass starts before the second;
    // the old echo still arrives at sample five, but no second echo is injected.
    let events = [ParameterEvent::set(
        1,
        BYPASS,
        ParameterEventValue::Boolean(true),
    )];
    assert_eq!(
        render(
            &mut runtime,
            [1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            &events
        )
        .unwrap()
        .map(f64::to_bits),
        [0.0, 0.0, 1.0, 1.0, 0.0, 0.5, 0.0, 0.0].map(f64::to_bits)
    );
    assert_eq!(runtime.active_latency(), Some(LatencySamples::new(2)));
    assert_eq!(runtime.active_tail(), Some(TailLength::FiniteSamples(5)));
    assert_eq!(
        runtime.parameters().get("power"),
        Some(&ParameterValue::Boolean(true))
    );
    let saved = runtime.encode_state(StateLimits::default()).unwrap();
    runtime.reset().unwrap();
    assert_eq!(
        render(&mut runtime, [0.0; 8], &[])
            .unwrap()
            .map(f64::to_bits),
        [0.0; 8].map(f64::to_bits)
    );
    runtime.deactivate().unwrap();
    assert_eq!(runtime.active_tail(), None);
    let mut reopened = InstanceRuntime::for_component(&effect).unwrap();
    reopened
        .apply_state_bytes(&effect, &saved, StateLimits::default())
        .unwrap();
    reopened
        .activate(&effect, config(), DEFAULT_EFFECT_CONFIGURATION)
        .unwrap();
    assert_eq!(
        render(&mut reopened, [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], &[])
            .unwrap()
            .map(f64::to_bits),
        [0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0].map(f64::to_bits)
    );
}

#[test]
fn tail_refreshes_only_after_successful_completion_and_is_not_latency() {
    let effect = Echo::default();
    let mut runtime = active(&effect);
    effect.next_tail.store(9, Ordering::Relaxed);
    assert_eq!(runtime.active_tail(), Some(TailLength::FiniteSamples(5)));
    let bad = [ParameterEvent::set(
        0,
        BYPASS,
        ParameterEventValue::Float(1.0),
    )];
    assert!(render(&mut runtime, [0.0; 8], &bad).is_err());
    assert_eq!(runtime.active_tail(), Some(TailLength::FiniteSamples(5)));
    render(&mut runtime, [0.0; 8], &[]).unwrap();
    assert_eq!(runtime.active_tail(), Some(TailLength::FiniteSamples(9)));
    effect.next_tail.store(u64::MAX, Ordering::Relaxed);
    runtime.reset().unwrap();
    assert_eq!(runtime.active_tail(), Some(TailLength::Infinite));
    assert_eq!(runtime.active_latency(), Some(LatencySamples::new(2)));
}

#[test]
fn pending_custom_resources_freeze_consumed_bypass_and_tail_until_reactivation() {
    let effect = Echo::default();
    let mut runtime = active(&effect);
    let mut next = StateDocument::new("test.tail-bypass", 1).unwrap();
    next.insert(StateEntry::new(
        "parameter/power",
        StateValue::Boolean(true),
    ))
    .unwrap();
    next.insert(StateEntry::new(
        "state/long_echo",
        StateValue::Boolean(true),
    ))
    .unwrap();
    runtime.apply_state(&effect, &next).unwrap();
    assert!(runtime.restart_requested());
    runtime.reset().unwrap();
    let withheld = [ParameterEvent::set(
        0,
        BYPASS,
        ParameterEventValue::Boolean(false),
    )];
    let impulse = [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    assert_eq!(
        render(&mut runtime, impulse, &withheld)
            .unwrap()
            .map(f64::to_bits),
        [0.0, 0.0, 1.0, 0.0, 0.0, 0.5, 0.0, 0.0].map(f64::to_bits)
    );
    assert_eq!(runtime.active_tail(), Some(TailLength::FiniteSamples(5)));
    assert_eq!(
        runtime.parameters().get("power"),
        Some(&ParameterValue::Boolean(true))
    );
    assert_eq!(runtime.state_document().unwrap(), next);
    runtime.deactivate().unwrap();
    runtime
        .activate(&effect, config(), DEFAULT_EFFECT_CONFIGURATION)
        .unwrap();
    assert_eq!(runtime.active_tail(), Some(TailLength::FiniteSamples(7)));
    assert_eq!(
        render(&mut runtime, impulse, &[])
            .unwrap()
            .map(f64::to_bits),
        [0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0].map(f64::to_bits)
    );
}

#[test]
fn panicking_tail_query_does_not_commit_metadata_or_control_endpoints() {
    let effect = Echo::default();
    let mut runtime = active(&effect);
    effect.panic_tail.store(true, Ordering::Relaxed);
    let events = [ParameterEvent::set(
        0,
        BYPASS,
        ParameterEventValue::Boolean(true),
    )];
    assert!(catch_unwind(AssertUnwindSafe(|| render(&mut runtime, [0.0; 8], &events))).is_err());
    assert_eq!(runtime.active_tail(), Some(TailLength::FiniteSamples(5)));
    assert_eq!(
        runtime.parameters().get("power"),
        Some(&ParameterValue::Boolean(false))
    );
    // Do not resume processing after an arbitrary product panic.
    runtime.deactivate().unwrap();
}
