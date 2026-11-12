//! Independent source-contract reference versus compact production cursors and
//! explicitly bounded dense expansion. No production source-format adapter.
//!
//! Independent implementation of the documented VST3 queue mathematics, checked
//! 2026-11-12 against `ivstparameterchanges.h` at
//! <https://github.com/steinbergmedia/vst3_pluginterfaces/blob/4f547e8e102b47de4a8b8aaf343c73b700786372/vst/ivstparameterchanges.h>
//! No SDK code, dependency, ABI, or adapter is included.

use std::{convert::Infallible, num::NonZeroU32};

use chassis_runtime::{
    audio::{AudioPortIndex, DEFAULT_EFFECT_CONFIGURATION},
    automation::{ParameterEvent, ParameterEventChange, ParameterEventValue, ParameterEvents},
    buffer::{ChannelBuffer, InputEndpoint, OutputEndpoint},
    parameters::{
        FloatMapping, FloatParameter, ParameterControlError, ParameterDescriptor, ParameterStore,
        ParameterValue,
    },
    process::{ActivationConfig, ProcessBlock, ProcessConfig, ProcessContext, ProcessMode},
    runtime::{Component, InstanceProcessError, InstanceRuntime, Process, ProcessError, Processor},
    schema::{ComponentId, ComponentSchema, ComponentSchemaError, StateSchemaVersion},
    state::{StateEntry, StateLimits},
    transport::TransportSnapshot,
};

const FRAMES: u32 = 16;
const PARAMETERS: usize = 2;
const SOURCE_POINTS: usize = 8;
const EXTRA_EVENTS: usize = 8;
const INDEX: chassis_runtime::parameters::ParameterIndex =
    chassis_runtime::parameters::ParameterIndex::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Curve {
    Identity,
    Square,
    Plateau,
}
impl Curve {
    // Keep these closed reference formulas independent of the interpolation
    // implementations. Only representative inverse conversion below is non-RT.
    fn map(self, coordinate: f64) -> f64 {
        match self {
            Self::Identity => coordinate,
            Self::Square => coordinate * coordinate,
            Self::Plateau => {
                if coordinate <= 0.5 {
                    0.0
                } else {
                    2.0 * coordinate - 1.0
                }
            }
        }
    }
}
impl FloatMapping for Curve {
    fn validate(&self, minimum: f64, maximum: f64) -> Result<(), ParameterControlError> {
        if matches!((minimum, maximum), (0.0, 1.0)) {
            Ok(())
        } else {
            Err(ParameterControlError::InvalidPolicy(
                "proof mapping requires 0..=1",
            ))
        }
    }
    fn normalized_to_plain(&self, coordinate: f64) -> f64 {
        self.map(coordinate)
    }
    fn plain_to_normalized(&self, value: f64) -> Result<f64, ParameterControlError> {
        assert!(
            !super::counting_this_thread(),
            "non-realtime inverse entered the callback"
        );
        Ok(match self {
            Self::Identity => value,
            Self::Square => value.sqrt(),
            Self::Plateau => {
                if value == 0.0 {
                    0.0
                } else {
                    0.5 + value * 0.5
                }
            }
        })
    }
}

#[derive(Clone, Copy)]
struct Point {
    offset: u32,
    coordinate: f64,
}
#[derive(Clone, Copy)]
struct Queue<'a> {
    parameter: usize,
    initial: f64,
    points: &'a [Point],
}

#[derive(Debug, PartialEq, Eq)]
enum Rejection {
    Source,
    Capacity,
    Block,
    Runtime,
}

// Independent oracle: rescan source segments and use barycentric weights.
// It shares neither the compact cursor's clipping nor the expansion's streaming
// interpolation and is never called by the measured callback.
fn reference(queue: Queue<'_>, sample: u32) -> f64 {
    let mut left_time = -1.0;
    let mut left = queue.initial;
    let time = f64::from(sample);
    for point in queue.points {
        let right_time = f64::from(point.offset);
        if sample == point.offset {
            return point.coordinate;
        }
        if sample < point.offset {
            return ((right_time - time) * left + (time - left_time) * point.coordinate)
                / (right_time - left_time);
        }
        left_time = right_time;
        left = point.coordinate;
    }
    left
}

fn validate_queue(queue: Queue<'_>, frames: u32) -> Result<(), Rejection> {
    if queue.parameter >= PARAMETERS
        || !queue.initial.is_finite()
        || !(0.0..=1.0).contains(&queue.initial)
        || queue.points.len() > SOURCE_POINTS
    {
        return Err(Rejection::Source);
    }
    let mut previous = None;
    for point in queue.points {
        // This proof admits strictly increasing points. It does not infer a
        // duplicate-offset rule that the source interface does not specify.
        if point.offset >= frames
            || previous.is_some_and(|offset| point.offset <= offset)
            || !point.coordinate.is_finite()
            || !(0.0..=1.0).contains(&point.coordinate)
        {
            return Err(Rejection::Source);
        }
        previous = Some(point.offset);
    }
    Ok(())
}

// Compact production path: retain the source coordinate, but clip its -1 anchor to
// an explicit sample-zero coordinate BEFORE mapping. A signed runtime offset
// is consequently not the only faithful representation. At most N+1 records.
fn compact(queue: Queue<'_>, curve: Curve, frames: u32) -> [f64; FRAMES as usize] {
    validate_queue(queue, frames).unwrap();
    let descriptor = ParameterDescriptor::float("source", "Source", 0.0, 1.0, 0.0)
        .unwrap()
        .with_float_mapping(curve)
        .unwrap();
    let mut store = ParameterStore::new(&[descriptor]).unwrap();
    store.set_normalized_index(INDEX, queue.initial).unwrap();
    let mut records = [ParameterEvent::normalized_linear(0, INDEX, 0.0); SOURCE_POINTS + 1];
    let mut length = 0;
    if let Some(first) = queue.points.first().filter(|p| p.offset > 0) {
        records[0] = ParameterEvent::normalized_set(
            0,
            INDEX,
            queue.initial + (first.coordinate - queue.initial) / f64::from(first.offset + 1),
        );
        length = 1;
    }
    for point in queue.points {
        records[length] = ParameterEvent::normalized_linear(point.offset, INDEX, point.coordinate);
        length += 1;
    }
    let events = ParameterEvents::new(
        &records[..length],
        frames,
        u32::try_from(SOURCE_POINTS + 1).unwrap(),
    )
    .unwrap();
    store.validate_events(events).unwrap();
    let mut cursor = store.float_cursor(events, INDEX).unwrap();
    let mut values = [0.0; FRAMES as usize];
    for frame in 0..frames {
        values[frame as usize] = cursor.value_at(frame).unwrap();
    }
    values
}

struct Expansion {
    events: Vec<ParameterEvent>,
    maximum: u32,
}
impl Expansion {
    fn new(config: ProcessConfig) -> Self {
        Self {
            events: Vec::with_capacity(config.max_parameter_events() as usize),
            maximum: config.max_parameter_events(),
        }
    }
    fn prepare(
        &mut self,
        frames: u32,
        queues: &[Queue<'_>],
        curves: [Curve; PARAMETERS],
        extras: &[ParameterEvent],
    ) -> Result<&[ParameterEvent], Rejection> {
        if frames == 0 || frames > FRAMES {
            return Err(Rejection::Block);
        }
        if queues.len() > PARAMETERS || extras.len() > EXTRA_EVENTS {
            return Err(Rejection::Source);
        }
        let mut seen = [false; PARAMETERS];
        let mut points = 0_usize;
        for queue in queues {
            points = points
                .checked_add(queue.points.len())
                .ok_or(Rejection::Source)?;
            if points > SOURCE_POINTS {
                return Err(Rejection::Source);
            }
            validate_queue(*queue, frames)?;
            if seen[queue.parameter] {
                return Err(Rejection::Source);
            }
            seen[queue.parameter] = true;
        }
        ParameterEvents::new(extras, frames, u32::try_from(EXTRA_EVENTS).unwrap())
            .map_err(|_| Rejection::Source)?;
        // Supplemental inputs are held modulation only. Plain overrides need
        // the production coordinate-invalidation protocol, not silent carry.
        if extras
            .iter()
            .any(|event| !matches!(event.change(), ParameterEventChange::Modulate { .. }))
        {
            return Err(Rejection::Source);
        }
        let needed = (frames as usize)
            .checked_mul(queues.len())
            .and_then(|n| n.checked_add(extras.len()))
            .ok_or(Rejection::Capacity)?;
        if needed > self.maximum as usize {
            return Err(Rejection::Capacity);
        }
        self.events.clear();
        let mut next = [0; PARAMETERS];
        let mut left_time = [-1.0; PARAMETERS];
        let mut left_value = [0.0; PARAMETERS];
        for (i, queue) in queues.iter().enumerate() {
            left_value[i] = queue.initial;
        }
        let mut extra = 0;
        for frame in 0..frames {
            // Deterministic queue-list order breaks cross-parameter ties; the
            // source's independent queues supply no causal total order.
            for (i, queue) in queues.iter().enumerate() {
                while let Some(point) = queue.points.get(next[i]).filter(|p| p.offset <= frame) {
                    left_time[i] = f64::from(point.offset);
                    left_value[i] = point.coordinate;
                    next[i] += 1;
                }
                let coordinate = match queue.points.get(next[i]) {
                    Some(point) => {
                        let fraction = (f64::from(frame) - left_time[i])
                            / (f64::from(point.offset) - left_time[i]);
                        left_value[i] + (point.coordinate - left_value[i]) * fraction
                    }
                    None => left_value[i],
                };
                self.events.push(ParameterEvent::set(
                    frame,
                    chassis_runtime::parameters::ParameterIndex::new(
                        u32::try_from(queue.parameter).unwrap(),
                    ),
                    ParameterEventValue::Float(curves[queue.parameter].map(coordinate)),
                ));
            }
            while let Some(event) = extras.get(extra).filter(|e| e.offset() == frame) {
                self.events.push(*event);
                extra += 1;
            }
        }
        assert_eq!(self.events.len(), needed);
        Ok(&self.events)
    }
}

struct Effect {
    curves: [Curve; PARAMETERS],
    fail: bool,
}
struct Dsp {
    handles: [FloatParameter; PARAMETERS],
    fail: bool,
}
impl Component for Effect {
    type Processor = Dsp;
    type ActivationError = Infallible;
    fn schema(&self) -> Result<ComponentSchema, ComponentSchemaError> {
        ComponentSchema::stereo_effect_with_state(
            ComponentId::new("org.nijaru.chassis.trajectory-proof").unwrap(),
            StateSchemaVersion::new(1),
            ["gain", "other"]
                .into_iter()
                .zip(self.curves)
                .map(|(key, curve)| {
                    ParameterDescriptor::float(key, key, 0.0, 1.0, 0.0)
                        .unwrap()
                        .with_float_mapping(curve)
                        .unwrap()
                        .with_global_modulation()
                        .unwrap()
                })
                .collect(),
        )
    }
    fn activate(
        &self,
        _: &ActivationConfig<'_>,
        parameters: &ParameterStore,
        _: &[StateEntry],
    ) -> Result<Dsp, Infallible> {
        Ok(Dsp {
            handles: [
                parameters.float_parameter("gain").unwrap(),
                parameters.float_parameter("other").unwrap(),
            ],
            fail: self.fail,
        })
    }
}
impl Processor for Dsp {}
impl Process<f64> for Dsp {
    fn process(&mut self, block: &mut ProcessBlock<'_, f64>) -> Result<(), ProcessError> {
        let mut cursors = [
            self.handles[0].cursor(block)?,
            self.handles[1].cursor(block)?,
        ];
        block.try_for_each_channel(|mut channel| {
            let index = channel.output_endpoint().unwrap().channel() as usize;
            for (frame, sample) in channel
                .copy_input_to_output()
                .unwrap()
                .iter_mut()
                .enumerate()
            {
                *sample *= cursors[index].value_at(u32::try_from(frame).unwrap())?;
            }
            Ok::<_, ProcessError>(())
        })?;
        if self.fail {
            return Err(ProcessError::Message("failure after audio mutation"));
        }
        Ok(())
    }
}

struct Probe {
    runtime: InstanceRuntime<Dsp>,
    expansion: Expansion,
    curves: [Curve; PARAMETERS],
    carry: [f64; PARAMETERS],
    output: [[f64; FRAMES as usize]; PARAMETERS],
}
impl Probe {
    fn new(
        curves: [Curve; PARAMETERS],
        initial: [f64; PARAMETERS],
        maximum: u32,
        fail: bool,
    ) -> Self {
        let component = Effect { curves, fail };
        let mut runtime = InstanceRuntime::for_component(&component).unwrap();
        for (key, value) in ["gain", "other"]
            .into_iter()
            .zip(initial.into_iter().zip(curves).map(|(x, c)| c.map(x)))
        {
            runtime
                .parameters_mut()
                .set(key, ParameterValue::Float(value))
                .unwrap();
        }
        let config = ProcessConfig::new(
            48_000.0,
            NonZeroU32::new(1),
            NonZeroU32::new(FRAMES).unwrap(),
            maximum,
        )
        .unwrap();
        runtime
            .activate(&component, config, DEFAULT_EFFECT_CONFIGURATION)
            .unwrap();
        Self {
            runtime,
            expansion: Expansion::new(config),
            curves,
            carry: initial,
            output: [[-99.0; FRAMES as usize]; PARAMETERS],
        }
    }
    fn process(
        &mut self,
        frames: u32,
        queues: &[Queue<'_>],
        extras: &[ParameterEvent],
    ) -> Result<(), Rejection> {
        // Source carry is a transient consumption projection, not a separately
        // writable plain store. Its inputs must match the previously consumed
        // source coordinates. Production override/epoch reseeding is NOT modeled.
        if queues.len() > PARAMETERS {
            return Err(Rejection::Source);
        }
        for queue in queues {
            if self.carry.get(queue.parameter).copied() != Some(queue.initial) {
                return Err(Rejection::Source);
            }
        }
        let maximum = self.expansion.maximum;
        let events = self
            .expansion
            .prepare(frames, queues, self.curves, extras)?;
        render(
            &mut self.runtime,
            ParameterEvents::new(events, frames, maximum).unwrap(),
            &mut self.output,
        )
        .map_err(|_| Rejection::Runtime)?;
        // Update only after successful DSP AND endpoint/tail completion.
        for queue in queues {
            if let Some(last) = queue.points.last() {
                self.carry[queue.parameter] = last.coordinate;
            }
        }
        Ok(())
    }
}

fn render(
    runtime: &mut InstanceRuntime<Dsp>,
    events: ParameterEvents<'_>,
    output: &mut [[f64; FRAMES as usize]; PARAMETERS],
) -> Result<(), InstanceProcessError> {
    let frames = events.frame_count();
    let count = frames as usize;
    let input = [1.0; FRAMES as usize];
    let [left, right] = output;
    let mut channels = [
        ChannelBuffer::separate(
            InputEndpoint::new(AudioPortIndex::new(0), 0),
            &input[..count],
            OutputEndpoint::new(AudioPortIndex::new(1), 0),
            &mut left[..count],
            frames,
        )
        .unwrap(),
        ChannelBuffer::separate(
            InputEndpoint::new(AudioPortIndex::new(0), 1),
            &input[..count],
            OutputEndpoint::new(AudioPortIndex::new(1), 1),
            &mut right[..count],
            frames,
        )
        .unwrap(),
    ];
    let context = ProcessContext::new(ProcessMode::Realtime, &TransportSnapshot::UNKNOWN, events);
    runtime.process(frames, context, &mut channels)
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 1e-12,
        "actual={actual}, expected={expected}"
    );
}

#[test]
fn interpolation_domain_and_previous_sample_anchor_are_independent_requirements() {
    let points = [
        Point {
            offset: 0,
            coordinate: 0.0,
        },
        Point {
            offset: 4,
            coordinate: 1.0,
        },
    ];
    let queue = Queue {
        parameter: 0,
        initial: 0.0,
        points: &points,
    };
    let wrong = [
        ParameterEvent::set(0, INDEX, ParameterEventValue::Float(0.0)),
        ParameterEvent::linear(4, INDEX, 1.0),
    ];
    let mut cursor = ParameterEvents::new(&wrong, 5, 2)
        .unwrap()
        .float_cursor(INDEX, 0.0)
        .unwrap();
    assert_eq!(
        Curve::Square.map(reference(queue, 2)).to_bits(),
        0.25_f64.to_bits()
    );
    assert_eq!(cursor.value_at(2).unwrap().to_bits(), 0.5_f64.to_bits());
    assert_eq!(
        compact(queue, Curve::Square, 5)[2].to_bits(),
        0.25_f64.to_bits()
    );
    let mut probe = Probe::new([Curve::Square, Curve::Identity], [0.0; 2], 5, false);
    probe.process(5, &[queue], &[]).unwrap();
    assert_eq!(probe.output[0][2].to_bits(), 0.25_f64.to_bits());
    let point = [Point {
        offset: 3,
        coordinate: 1.0,
    }];
    let queue = Queue {
        parameter: 0,
        initial: 0.0,
        points: &point,
    };
    let wrong = [ParameterEvent::linear(3, INDEX, 1.0)];
    let mut cursor = ParameterEvents::new(&wrong, 4, 1)
        .unwrap()
        .float_cursor(INDEX, 0.0)
        .unwrap();
    assert_eq!(
        (0..4).map(|f| reference(queue, f)).collect::<Vec<_>>(),
        [0.25, 0.5, 0.75, 1.0]
    );
    assert_eq!(cursor.value_at(0).unwrap().to_bits(), 0.0_f64.to_bits());
    assert_eq!(
        compact(queue, Curve::Identity, 4)[..4],
        [0.25, 0.5, 0.75, 1.0]
    );
}

#[test]
fn source_domain_clipping_and_dense_runtime_agree_across_partitions() {
    for falling in [false, true] {
        for partitions in [&[16_u32][..], &[1, 3, 4, 8][..], &[3, 5, 8][..]] {
            let start = if falling { 1.0 } else { 0.0 };
            let mut probe = Probe::new([Curve::Square, Curve::Identity], [start, start], 32, false);
            let mut consumed = 0;
            for &frames in partitions {
                let position = f64::from(consumed + frames) / f64::from(FRAMES);
                let end = if falling { 1.0 - position } else { position };
                let points = [Point {
                    offset: frames - 1,
                    coordinate: end,
                }];
                let queues = [
                    Queue {
                        parameter: 0,
                        initial: probe.carry[0],
                        points: &points,
                    },
                    Queue {
                        parameter: 1,
                        initial: probe.carry[1],
                        points: &points,
                    },
                ];
                let compact = compact(queues[0], Curve::Square, frames);
                probe.process(frames, &queues, &[]).unwrap();
                for f in 0..frames {
                    let position = f64::from(consumed + f + 1) / f64::from(FRAMES);
                    let coordinate = if falling { 1.0 - position } else { position };
                    close(reference(queues[0], f), coordinate);
                    close(probe.output[0][f as usize], coordinate * coordinate);
                    close(probe.output[1][f as usize], coordinate);
                    close(compact[f as usize], coordinate * coordinate);
                }
                consumed += frames;
            }
            let completed = if falling { 0.0 } else { 1.0 };
            probe.process(4, &[], &[]).unwrap();
            assert_eq!(probe.output[0][..4], [completed; 4]);
            assert_eq!(
                probe.carry.map(f64::to_bits),
                [completed; 2].map(f64::to_bits)
            );
        }
    }
}

#[test]
fn separated_interior_knots_reverse_slope_without_reanchoring_or_extrapolation() {
    let points = [
        Point {
            offset: 2,
            coordinate: 0.75,
        },
        Point {
            offset: 6,
            coordinate: 0.25,
        },
        Point {
            offset: 9,
            coordinate: 1.0,
        },
    ];
    let queue = Queue {
        parameter: 0,
        initial: 0.0,
        points: &points,
    };
    let candidate = compact(queue, Curve::Square, FRAMES);
    let mut probe = Probe::new([Curve::Square, Curve::Identity], [0.0; 2], FRAMES, false);
    probe.process(FRAMES, &[queue], &[]).unwrap();
    for frame in 0..FRAMES {
        let coordinate = reference(queue, frame);
        close(candidate[frame as usize], coordinate * coordinate);
        close(probe.output[0][frame as usize], coordinate * coordinate);
    }
    assert_eq!(reference(queue, 4).to_bits(), 0.5_f64.to_bits());
    assert_eq!(probe.output[0][4].to_bits(), 0.25_f64.to_bits());
    assert_eq!(probe.output[0][10..], [1.0; 6]);
}

#[test]
fn supplemental_plain_overrides_cannot_diverge_consumed_source_carry() {
    let points = [Point {
        offset: 3,
        coordinate: 0.5,
    }];
    let queue = Queue {
        parameter: 0,
        initial: 0.0,
        points: &points,
    };
    let mut probe = Probe::new([Curve::Square, Curve::Identity], [0.0; 2], 5, false);
    let before = probe.runtime.encode_state(StateLimits::default()).unwrap();
    for event in [
        ParameterEvent::set(3, INDEX, ParameterEventValue::Float(0.9)),
        ParameterEvent::linear(3, INDEX, 0.9),
    ] {
        assert_eq!(probe.process(4, &[queue], &[event]), Err(Rejection::Source));
        assert_eq!(
            probe.runtime.encode_state(StateLimits::default()).unwrap(),
            before
        );
        assert_eq!(probe.carry.map(f64::to_bits), [0.0; 2].map(f64::to_bits));
        assert_eq!(probe.output, [[-99.0; FRAMES as usize]; 2]);
    }
}

#[test]
fn completed_state_reopens_in_plain_units_without_the_source_anchor_or_modulation() {
    let curves = [Curve::Square, Curve::Identity];
    let mut probe = Probe::new(curves, [0.0; 2], 8, false);
    let points = [Point {
        offset: 3,
        coordinate: 0.5,
    }];
    let queue = Queue {
        parameter: 0,
        initial: 0.0,
        points: &points,
    };
    probe
        .process(4, &[queue], &[ParameterEvent::modulation(0, INDEX, 0.5)])
        .unwrap();
    assert_eq!(probe.output[0][3].to_bits(), 0.75_f64.to_bits());
    let bytes = probe.runtime.encode_state(StateLimits::default()).unwrap();
    let component = Effect {
        curves,
        fail: false,
    };
    let mut reopened = InstanceRuntime::for_component(&component).unwrap();
    reopened
        .apply_state_bytes(&component, &bytes, StateLimits::default())
        .unwrap();
    let config = ProcessConfig::new(
        48_000.0,
        NonZeroU32::new(1),
        NonZeroU32::new(FRAMES).unwrap(),
        8,
    )
    .unwrap();
    reopened
        .activate(&component, config, DEFAULT_EFFECT_CONFIGURATION)
        .unwrap();
    let mut output = [[-99.0; FRAMES as usize]; PARAMETERS];
    // A complete plain-state reopen has no retained source coordinate. This
    // reference resumes no queue; production normalized sets explicitly reseed.
    render(
        &mut reopened,
        ParameterEvents::empty_for_block(4),
        &mut output,
    )
    .unwrap();
    assert_eq!(output[0][..4], [0.25; 4]);
    assert_eq!(output[1][..4], [0.0; 4]);
    assert_eq!(
        reopened.encode_state(StateLimits::default()).unwrap(),
        bytes
    );
}

#[test]
fn jumps_held_tails_and_noninvertible_carry_are_not_smoothed_or_reconstructed() {
    let mut probe = Probe::new([Curve::Square, Curve::Plateau], [0.0, 0.5], 16, false);
    let jump = [
        Point {
            offset: 1,
            coordinate: 0.0,
        },
        Point {
            offset: 2,
            coordinate: 1.0,
        },
    ];
    let plateau = [Point {
        offset: 3,
        coordinate: 1.0,
    }];
    let queues = [
        Queue {
            parameter: 0,
            initial: 0.0,
            points: &jump,
        },
        Queue {
            parameter: 1,
            initial: 0.5,
            points: &plateau,
        },
    ];
    probe.process(4, &queues, &[]).unwrap();
    assert_eq!(probe.output[0][..4], [0.0, 0.0, 1.0, 1.0]);
    assert_eq!(probe.output[1][..4], [0.25, 0.5, 0.75, 1.0]);
    let at_zero = [Point {
        offset: 0,
        coordinate: 0.0,
    }];
    let queue = Queue {
        parameter: 0,
        initial: 1.0,
        points: &at_zero,
    };
    probe.process(1, &[queue], &[]).unwrap();
    assert_eq!(probe.output[0][0].to_bits(), 0.0_f64.to_bits());
    assert_eq!(
        compact(queue, Curve::Square, 1)[0].to_bits(),
        0.0_f64.to_bits()
    );
    // Both source coordinates map to the same durable plain value, but yield
    // different next curves. A plain store alone cannot recover this anchor.
    let points = [Point {
        offset: 3,
        coordinate: 1.0,
    }];
    let a = Queue {
        parameter: 1,
        initial: 0.0,
        points: &points,
    };
    let b = Queue {
        parameter: 1,
        initial: 0.5,
        points: &points,
    };
    assert_eq!(
        Curve::Plateau.map(a.initial).to_bits(),
        Curve::Plateau.map(b.initial).to_bits()
    );
    assert_eq!(
        Curve::Plateau.map(reference(a, 0)).to_bits(),
        0.0_f64.to_bits()
    );
    assert_eq!(
        Curve::Plateau.map(reference(b, 0)).to_bits(),
        0.25_f64.to_bits()
    );
}

#[test]
fn expansion_reserves_actual_runtime_budget_and_commits_carry_only_on_success() {
    let points = [Point {
        offset: 3,
        coordinate: 0.5,
    }];
    let queue = Queue {
        parameter: 0,
        initial: 0.0,
        points: &points,
    };
    let amount = ParameterEvent::modulation(0, INDEX, 0.5);
    let mut exact = Probe::new([Curve::Square, Curve::Identity], [0.0; 2], 5, false);
    exact.process(4, &[queue], &[amount]).unwrap();
    assert_eq!(exact.expansion.events.len(), 5);
    assert_eq!(exact.output[0][..4], [0.515_625, 0.5625, 0.640_625, 0.75]);
    assert_eq!(
        exact.runtime.parameters().get("gain"),
        Some(&ParameterValue::Float(0.25))
    );
    assert_eq!(exact.carry[0].to_bits(), 0.5_f64.to_bits());
    exact.process(4, &[], &[]).unwrap();
    assert_eq!(exact.output[0][..4], [0.75; 4]);
    let mut short = Probe::new([Curve::Square, Curve::Identity], [0.0; 2], 4, false);
    let bytes = short.runtime.encode_state(StateLimits::default()).unwrap();
    assert_eq!(
        short.process(4, &[queue], &[amount]),
        Err(Rejection::Capacity)
    );
    assert_eq!(
        short.runtime.encode_state(StateLimits::default()).unwrap(),
        bytes
    );
    assert_eq!(short.output, [[-99.0; FRAMES as usize]; 2]);
    assert_eq!(short.carry.map(f64::to_bits), [0.0; 2].map(f64::to_bits));
    // Successful conversion is not successful consumption: schema rejection
    // and product failure must not advance the source projection either.
    let invalid =
        ParameterEvent::modulation(0, chassis_runtime::parameters::ParameterIndex::new(99), 0.5);
    let mut rejected = Probe::new([Curve::Square, Curve::Identity], [0.0; 2], 5, false);
    assert_eq!(
        rejected.process(4, &[queue], &[invalid]),
        Err(Rejection::Runtime)
    );
    assert_eq!(rejected.carry.map(f64::to_bits), [0.0; 2].map(f64::to_bits));
    assert_eq!(rejected.output, [[-99.0; FRAMES as usize]; 2]);
    assert_eq!(
        rejected
            .runtime
            .encode_state(StateLimits::default())
            .unwrap(),
        bytes
    );
    let mut failed = Probe::new([Curve::Square, Curve::Identity], [0.0; 2], 5, true);
    assert_eq!(failed.process(4, &[queue], &[]), Err(Rejection::Runtime));
    assert_eq!(failed.carry.map(f64::to_bits), [0.0; 2].map(f64::to_bits));
    assert_eq!(
        failed.runtime.encode_state(StateLimits::default()).unwrap(),
        bytes
    );
    assert_ne!(failed.output[0][0].to_bits(), (-99.0_f64).to_bits());
    // Do not resume the damaged processor.
}

#[test]
fn source_admission_is_bounded_and_failure_does_not_publish_partial_work() {
    let mut probe = Probe::new([Curve::Square, Curve::Identity], [0.0; 2], 32, false);
    let repeated = [Point {
        offset: 0,
        coordinate: 0.0,
    }; SOURCE_POINTS + 1];
    let nan = [Point {
        offset: 1,
        coordinate: f64::NAN,
    }];
    let unordered = [
        Point {
            offset: 2,
            coordinate: 0.5,
        },
        Point {
            offset: 1,
            coordinate: 1.0,
        },
    ];
    let outside = [Point {
        offset: 4,
        coordinate: 1.0,
    }];
    let range = [Point {
        offset: 1,
        coordinate: 1.1,
    }];
    let duplicate_offset = [unordered[0]; 2];
    for points in [
        &repeated[..],
        &nan,
        &unordered,
        &outside,
        &range,
        &duplicate_offset,
    ] {
        assert_eq!(
            probe.process(
                4,
                &[Queue {
                    parameter: 0,
                    initial: 0.0,
                    points
                }],
                &[]
            ),
            Err(Rejection::Source)
        );
        assert_eq!(probe.carry.map(f64::to_bits), [0.0; 2].map(f64::to_bits));
        assert_eq!(probe.output, [[-99.0; FRAMES as usize]; 2]);
    }
    let empty = Queue {
        parameter: 0,
        initial: 0.0,
        points: &[],
    };
    assert_eq!(
        probe.process(4, &[empty, empty], &[]),
        Err(Rejection::Source)
    );
    assert_eq!(probe.process(0, &[], &[]), Err(Rejection::Block));
    assert_eq!(probe.process(FRAMES + 1, &[], &[]), Err(Rejection::Block));
    probe.process(4, &[empty], &[]).unwrap();
    assert_eq!(probe.output[0][..4], [0.0; 4]);
}

#[test]
fn independent_queue_order_and_total_source_bounds_are_explicit() {
    let mut probe = Probe::new([Curve::Square, Curve::Identity], [0.0; 2], 32, false);
    let points: [Point; 5] = std::array::from_fn(|i| Point {
        offset: u32::try_from(i).unwrap(),
        coordinate: f64::from(u32::try_from(i).unwrap()) / 4.0,
    });
    let a = Queue {
        parameter: 0,
        initial: 0.0,
        points: &points,
    };
    let b = Queue {
        parameter: 1,
        initial: 0.0,
        points: &points,
    };
    let before = probe.runtime.encode_state(StateLimits::default()).unwrap();
    // Each queue fits individually, but ten total points exceed the source
    // budget even though the resulting sixteen events fit the output budget.
    assert_eq!(probe.process(8, &[b, a], &[]), Err(Rejection::Source));
    assert_eq!(probe.process(8, &[a, b, a], &[]), Err(Rejection::Source));
    assert_eq!(
        probe.process(8, &[Queue { initial: 1.0, ..a }], &[]),
        Err(Rejection::Source)
    );
    assert_eq!(probe.carry.map(f64::to_bits), [0.0; 2].map(f64::to_bits));
    assert_eq!(probe.output, [[-99.0; FRAMES as usize]; 2]);
    assert_eq!(
        probe.runtime.encode_state(StateLimits::default()).unwrap(),
        before
    );
    let a = Queue {
        points: &points[..4],
        ..a
    };
    let b = Queue {
        points: &points[..4],
        ..b
    };
    probe.process(4, &[b, a], &[]).unwrap();
    assert_eq!(probe.output[0][..4], [0.0, 0.0625, 0.25, 0.5625]);
    assert_eq!(probe.output[1][..4], [0.0, 0.25, 0.5, 0.75]);
    assert_eq!(probe.carry.map(f64::to_bits), [0.75; 2].map(f64::to_bits));
}

pub(super) fn qualify() {
    let mut probe = Probe::new([Curve::Square, Curve::Identity], [0.0; 2], 33, false);
    let amount = [ParameterEvent::modulation(0, INDEX, 0.125)];
    super::ALLOCATIONS.store(0, std::sync::atomic::Ordering::Relaxed);
    super::DEALLOCATIONS.store(0, std::sync::atomic::Ordering::Relaxed);
    super::COUNT_THIS_THREAD.with(|flag| flag.set(true));
    for block in 0..1_000 {
        let end = if block % 2 == 0 { 1.0 } else { 0.0 };
        let points = [Point {
            offset: FRAMES - 1,
            coordinate: end,
        }];
        let queues = [
            Queue {
                parameter: 0,
                initial: probe.carry[0],
                points: &points,
            },
            Queue {
                parameter: 1,
                initial: probe.carry[1],
                points: &points,
            },
        ];
        probe.process(FRAMES, &queues, &amount).unwrap();
        assert_eq!(probe.expansion.events.len(), 33);
        assert_eq!(probe.carry.map(f64::to_bits), [end; 2].map(f64::to_bits));
        close(
            probe.output[0][FRAMES as usize - 1],
            (end + 0.125).clamp(0.0, 1.0),
        );
    }
    // Both rejection paths are measured too; none enters product DSP.
    let repeated = [Point {
        offset: 0,
        coordinate: 0.0,
    }; SOURCE_POINTS + 1];
    assert_eq!(
        probe.process(
            FRAMES,
            &[Queue {
                parameter: 0,
                initial: probe.carry[0],
                points: &repeated
            }],
            &[]
        ),
        Err(Rejection::Source)
    );
    let overflow = [amount[0]; 2];
    let points = [Point {
        offset: FRAMES - 1,
        coordinate: 1.0,
    }];
    let queues = [
        Queue {
            parameter: 0,
            initial: probe.carry[0],
            points: &points,
        },
        Queue {
            parameter: 1,
            initial: probe.carry[1],
            points: &points,
        },
    ];
    assert_eq!(
        probe.process(FRAMES, &queues, &overflow),
        Err(Rejection::Capacity)
    );
    super::COUNT_THIS_THREAD.with(|flag| flag.set(false));
    assert_eq!(
        super::ALLOCATIONS.load(std::sync::atomic::Ordering::Relaxed),
        0
    );
    assert_eq!(
        super::DEALLOCATIONS.load(std::sync::atomic::Ordering::Relaxed),
        0
    );
}
