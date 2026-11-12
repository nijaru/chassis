//! Timed source coordinates use production cursors and actual runtime completion.

use super::*;

fn instance(mapping: Mapping) -> (Definition, InstanceRuntime<Dsp>) {
    let component = Definition {
        mapping,
        ..Definition::default()
    };
    let mut runtime = InstanceRuntime::for_component(&component).unwrap();
    activate(&mut runtime, &component);
    (component, runtime)
}
fn samples(actual: [f64; 4], expected: [f64; 4]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
    }
}

#[test]
fn explicit_plain_cursors_reject_normalized_points_and_store_cursors_remain_borrowed() {
    use chassis_runtime::automation::ParameterCursorError;
    let component = Definition {
        mapping: Mapping::Square,
        ..Definition::default()
    };
    let mut store = ParameterStore::new(component.schema().unwrap().parameters()).unwrap();
    let records = [
        ParameterEvent::normalized_set(0, GAIN, 0.5),
        ParameterEvent::modulation(1, GAIN, 0.2),
        ParameterEvent::normalized_linear(2, GAIN, 1.0),
    ];
    let events = ParameterEvents::new(&records, 4, 3).unwrap();
    store.validate_events(events).unwrap();
    assert_eq!(pair(&store), (0.0_f64.to_bits(), None)); // Admission is not completion.
    let mut plain = events.float_cursor(GAIN, 0.0).unwrap();
    assert_eq!(
        plain.value_at(0),
        Err(ParameterCursorError::MappingUnavailable)
    );
    let mut amount = events.modulation_cursor(GAIN, 0.0).unwrap();
    assert_eq!(amount.value_at(2).unwrap().to_bits(), 0.2_f64.to_bits());
    let mut base = store.float_cursor(events, GAIN).unwrap();
    assert_eq!(base.value_at(0).unwrap().to_bits(), 0.25_f64.to_bits());
    assert_eq!(base.value_at(1).unwrap().to_bits(), 0.5625_f64.to_bits());
    let mut copy = base.clone();
    assert_eq!(copy.value_at(2).unwrap().to_bits(), 1.0_f64.to_bits());
    assert_eq!(base.value_at(2).unwrap().to_bits(), 1.0_f64.to_bits());
    assert!(matches!(
        base.value_at(1),
        Err(ParameterCursorError::NonMonotonicQuery { .. })
    ));
    assert!(matches!(
        base.value_at(4),
        Err(ParameterCursorError::OffsetOutOfRange { .. })
    ));
    assert!(store.float_cursor(events, BOOL).is_err());
    assert!(store.float_cursor(events, ParameterIndex::new(99)).is_err());
}

#[test]
fn mixed_completion_keeps_parameters_and_discrete_values_independent() {
    let (_, mut runtime) = instance(Mapping::Square);
    let records = [
        ParameterEvent::normalized_set(0, GAIN, 0.5),
        ParameterEvent::normalized_set(0, OTHER, 0.25),
        ParameterEvent::set(
            1,
            ParameterIndex::new(3),
            ParameterEventValue::Choice(ChoiceIndex::new(1)),
        ),
        ParameterEvent::set(1, BOOL, ParameterEventValue::Boolean(false)),
        ParameterEvent::set(1, ParameterIndex::new(4), ParameterEventValue::Integer(2)),
        ParameterEvent::normalized_linear(3, GAIN, 1.0),
        ParameterEvent::normalized_linear(3, OTHER, 0.75),
        ParameterEvent::modulation(3, GAIN, -0.2),
    ];
    samples(
        render_frames(&mut runtime, &records, 4).unwrap()[0],
        [0.25, 4.0 / 9.0, 25.0 / 36.0, 0.8],
    );
    assert_eq!(
        pair(runtime.parameters()),
        (1.0_f64.to_bits(), Some(1.0_f64.to_bits()))
    );
    assert_eq!(
        runtime
            .parameters()
            .normalized_coordinate(OTHER)
            .unwrap()
            .map(f64::to_bits),
        Some(0.75_f64.to_bits())
    );
    for (index, expected) in [
        (OTHER, ParameterEventValue::Float(0.75)),
        (BOOL, ParameterEventValue::Boolean(false)),
        (
            ParameterIndex::new(3),
            ParameterEventValue::Choice(ChoiceIndex::new(1)),
        ),
        (ParameterIndex::new(4), ParameterEventValue::Integer(2)),
    ] {
        assert_eq!(runtime.parameters().get_event_value(index), Some(expected));
    }
}

#[test]
fn mapping_follows_interpolation_and_precedes_modulation_and_clamping() {
    let (_, mut runtime) = instance(Mapping::Square);
    let events = [
        ParameterEvent::normalized_set(0, GAIN, 0.0),
        ParameterEvent::modulation(0, GAIN, 0.1),
        ParameterEvent::modulation(1, GAIN, 0.2),
        ParameterEvent::normalized_linear(2, GAIN, 1.0),
    ];
    let output = render_frames(&mut runtime, &events, 4).unwrap();
    samples(output[0], [0.1, 0.45, 1.0, 1.0]);
    assert_eq!(
        pair(runtime.parameters()),
        (1.0_f64.to_bits(), Some(1.0_f64.to_bits()))
    );
    assert_eq!(
        render(&mut runtime, &[]).unwrap().map(f64::to_bits),
        [1.0, 1.0].map(f64::to_bits)
    );
}

#[test]
fn unseeded_ramps_reject_even_at_zero_and_flat_regions_keep_exact_history() {
    let (component, mut runtime) = instance(Mapping::Plateau(0.5));
    for offset in [0, 2] {
        assert!(
            render(
                &mut runtime,
                &[ParameterEvent::normalized_linear(offset, GAIN, 0.75)]
            )
            .is_err()
        );
    }
    runtime
        .parameters_mut()
        .set_normalized_index(GAIN, 0.25)
        .unwrap();
    let output = render_frames(
        &mut runtime,
        &[ParameterEvent::normalized_linear(2, GAIN, 0.75)],
        4,
    )
    .unwrap();
    samples(output[0], [0.0, 0.0, 0.5, 0.5]);
    assert_eq!(
        pair(runtime.parameters()),
        (0.5_f64.to_bits(), Some(0.75_f64.to_bits()))
    );
    runtime.reset().unwrap();
    runtime.deactivate().unwrap();
    activate(&mut runtime, &component);
    assert_eq!(
        runtime
            .parameters()
            .normalized_coordinate(GAIN)
            .unwrap()
            .map(f64::to_bits),
        Some(0.75_f64.to_bits())
    );
    let same = runtime.state_document().unwrap();
    runtime.apply_state(&component, &same).unwrap();
    assert!(
        render(
            &mut runtime,
            &[ParameterEvent::normalized_linear(2, GAIN, 1.0)]
        )
        .is_err()
    );
    let events = [
        ParameterEvent::normalized_set(0, GAIN, 0.5),
        ParameterEvent::normalized_linear(2, GAIN, 0.75),
    ];
    samples(
        render_frames(&mut runtime, &events, 4).unwrap()[0],
        [0.0, 0.25, 0.5, 0.5],
    );
}

#[test]
fn mixed_domains_use_the_next_endpoint_domain_and_never_invert_plain_points() {
    let (_, mut runtime) = instance(Mapping::Square);
    let events = [
        ParameterEvent::normalized_set(0, GAIN, 0.5),
        ParameterEvent::linear(2, GAIN, 0.75),
    ];
    samples(
        render_frames(&mut runtime, &events, 4).unwrap()[0],
        [0.25, 0.5, 0.75, 0.75],
    );
    assert_eq!(pair(runtime.parameters()), (0.75_f64.to_bits(), None));
    runtime
        .parameters_mut()
        .set_normalized_index(GAIN, 0.5)
        .unwrap();
    let before = pair(runtime.parameters());
    let invalid = [
        ParameterEvent::set(0, GAIN, ParameterEventValue::Float(0.25)),
        ParameterEvent::normalized_linear(2, GAIN, 1.0),
    ];
    assert!(render(&mut runtime, &invalid).is_err());
    assert_eq!(pair(runtime.parameters()), before);
    let events = [
        ParameterEvent::set(0, GAIN, ParameterEventValue::Float(0.75)),
        ParameterEvent::normalized_set(1, GAIN, 0.5),
        ParameterEvent::normalized_linear(3, GAIN, 1.0),
    ];
    samples(
        render_frames(&mut runtime, &events, 4).unwrap()[0],
        [0.75, 0.25, 0.5625, 1.0],
    );
}

#[test]
fn equal_offset_overwrites_do_not_erase_the_incoming_segment() {
    let (_, mut runtime) = instance(Mapping::Square);
    let events = [
        ParameterEvent::normalized_set(0, GAIN, 0.0),
        ParameterEvent::normalized_linear(2, GAIN, 0.5),
        ParameterEvent::set(2, GAIN, ParameterEventValue::Float(0.8)),
        ParameterEvent::normalized_set(2, GAIN, 0.75),
        ParameterEvent::normalized_linear(3, GAIN, 1.0),
    ];
    samples(
        render_frames(&mut runtime, &events, 4).unwrap()[0],
        [0.0, 0.0625, 0.5625, 1.0],
    );
    let events = [
        ParameterEvent::normalized_set(0, GAIN, 0.0),
        ParameterEvent::normalized_linear(2, GAIN, 1.0),
        ParameterEvent::set(2, GAIN, ParameterEventValue::Float(0.2)),
    ];
    samples(
        render_frames(&mut runtime, &events, 4).unwrap()[0],
        [0.0, 0.25, 0.2, 0.2],
    );
    assert_eq!(pair(runtime.parameters()), (0.2_f64.to_bits(), None));
    let events = [
        ParameterEvent::normalized_set(2, GAIN, 0.3),
        ParameterEvent::normalized_linear(2, GAIN, 0.8),
    ];
    samples(
        render_frames(&mut runtime, &events, 4).unwrap()[0],
        [0.2, 0.2, 0.64, 0.64],
    );
}

#[test]
fn endpoints_are_admitted_before_dsp_but_interiors_are_checked_when_consumed() {
    let (_, mut runtime) = instance(Mapping::Broken);
    runtime
        .parameters_mut()
        .set_normalized_index(GAIN, 0.5)
        .unwrap();
    let before = pair(runtime.parameters());
    for events in [
        vec![
            ParameterEvent::normalized_set(0, GAIN, 0.25),
            ParameterEvent::set(0, GAIN, ParameterEventValue::Float(0.5)),
        ],
        vec![ParameterEvent::normalized_set(0, BOOL, 0.5)],
        vec![ParameterEvent::normalized_set(
            0,
            ParameterIndex::new(99),
            0.5,
        )],
        vec![ParameterEvent::normalized_set(0, GAIN, 0.5); 9],
    ] {
        assert!(matches!(
            render(&mut runtime, &events),
            Err(InstanceProcessError::InvalidBlock(_))
        ));
        assert_eq!(pair(runtime.parameters()), before);
    }
    // Admission rejected a batch; a clean retry must not read stale scratch.
    render(
        &mut runtime,
        &[ParameterEvent::normalized_set(0, GAIN, 0.5)],
    )
    .unwrap();
    // Endpoints .5 and 1 are valid, but the queried .75 maps outside the domain.
    let events = [
        ParameterEvent::normalized_set(0, GAIN, 0.5),
        ParameterEvent::normalized_linear(2, GAIN, 1.0),
    ];
    assert!(matches!(
        render(&mut runtime, &events),
        Err(InstanceProcessError::Processor(_))
    ));
    assert_eq!(pair(runtime.parameters()), before);
    // Abandon the damaged processor, preserving the unchanged canonical pair.
    runtime.deactivate().unwrap();
    let (_, mut runtime) = instance(Mapping::Broken);
    runtime
        .parameters_mut()
        .set_normalized_index(GAIN, 1.0)
        .unwrap();
    let before = pair(runtime.parameters());
    let events = [
        ParameterEvent::normalized_set(0, GAIN, 0.0),
        ParameterEvent::normalized_linear(2, GAIN, 0.5),
    ];
    assert!(matches!(
        render(&mut runtime, &events),
        Err(InstanceProcessError::Processor(_))
    ));
    assert_eq!(pair(runtime.parameters()), before); // Interior .25 mapped to NaN.
    runtime.deactivate().unwrap();
    for coordinate in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
        for event in [
            ParameterEvent::normalized_set(0, GAIN, coordinate),
            ParameterEvent::normalized_linear(1, GAIN, coordinate),
        ] {
            assert!(ParameterEvents::new(&[event], 4, 1).is_err());
        }
    }
}

#[test]
fn completion_uses_prepared_endpoints_and_failure_publishes_neither_field() {
    let (_, mut runtime) = instance(Mapping::CompletionProbe);
    let events = [
        ParameterEvent::normalized_set(0, GAIN, 0.5),
        ParameterEvent::normalized_linear(3, GAIN, 1.0),
    ];
    render(&mut runtime, &events).unwrap(); // The mapper panics if called after DSP.
    assert_eq!(
        pair(runtime.parameters()),
        (1.0_f64.to_bits(), Some(1.0_f64.to_bits()))
    );
    for failure in [Failure::Process, Failure::Tail] {
        let component = Definition {
            mapping: Mapping::Square,
            failure,
        };
        let mut runtime = InstanceRuntime::for_component(&component).unwrap();
        runtime
            .parameters_mut()
            .set_normalized_index(GAIN, 0.25)
            .unwrap();
        activate(&mut runtime, &component);
        let before = pair(runtime.parameters());
        let result = catch_unwind(AssertUnwindSafe(|| render(&mut runtime, &events)));
        match failure {
            Failure::Process => assert!(matches!(
                result,
                Ok(Err(InstanceProcessError::Processor(_)))
            )),
            Failure::Tail => assert!(result.is_err()),
            Failure::None => unreachable!(),
        }
        assert_eq!(pair(runtime.parameters()), before);
        runtime.deactivate().unwrap();
    }
}

#[test]
fn frozen_resources_validate_desired_sources_but_never_publish_their_endpoints() {
    let (component, mut runtime) = instance(Mapping::Plateau(0.5));
    runtime
        .parameters_mut()
        .set_normalized_index(GAIN, 0.5)
        .unwrap();
    runtime
        .apply_state(&component, &document(&runtime, 0.75, true))
        .unwrap();
    assert!(
        render(
            &mut runtime,
            &[ParameterEvent::normalized_linear(3, GAIN, 1.0)]
        )
        .is_err()
    );
    let events = [
        ParameterEvent::normalized_set(0, GAIN, 1.0),
        ParameterEvent::modulation(0, GAIN, 0.1),
        ParameterEvent::normalized_linear(3, GAIN, 0.5),
    ];
    assert_eq!(
        render(&mut runtime, &events).unwrap().map(f64::to_bits),
        [0.0, 0.5].map(f64::to_bits)
    );
    assert_eq!(pair(runtime.parameters()), (0.75_f64.to_bits(), None));
    runtime.reset().unwrap();
    runtime.deactivate().unwrap();
    activate(&mut runtime, &component);
    assert_eq!(
        render(&mut runtime, &[]).unwrap().map(f64::to_bits),
        [2.85, -1.0].map(f64::to_bits)
    );
}

#[test]
fn previous_sample_source_clipping_is_partition_invariant_in_actual_dsp() {
    for mapping in [Mapping::Square, Mapping::Plateau(0.5)] {
        for falling in [false, true] {
            for partitions in [&[4_u32][..], &[1, 3][..], &[2, 2][..]] {
                let (_, mut runtime) = instance(mapping);
                runtime
                    .parameters_mut()
                    .set_normalized_index(GAIN, f64::from(u8::from(falling)))
                    .unwrap();
                let mut consumed = 0;
                for &frames in partitions {
                    let initial = runtime
                        .parameters()
                        .normalized_coordinate(GAIN)
                        .unwrap()
                        .unwrap();
                    let end = if falling {
                        1.0 - f64::from(consumed + frames) / 4.0
                    } else {
                        f64::from(consumed + frames) / 4.0
                    };
                    let clipped = initial + (end - initial) / f64::from(frames);
                    let events = [
                        ParameterEvent::normalized_set(0, GAIN, clipped),
                        ParameterEvent::normalized_linear(frames - 1, GAIN, end),
                    ];
                    let output = render_frames(&mut runtime, &events, frames).unwrap();
                    for offset in 0..frames {
                        let n = f64::from(consumed + offset + 1) / 4.0;
                        let n = if falling { 1.0 - n } else { n };
                        let expected = match mapping {
                            Mapping::Square => n * n,
                            Mapping::Plateau(_) => {
                                if n <= 0.5 {
                                    0.0
                                } else {
                                    2.0 * n - 1.0
                                }
                            }
                            _ => unreachable!(),
                        };
                        assert!((output[0][offset as usize] - expected).abs() < 1e-12);
                    }
                    assert_eq!(
                        runtime
                            .parameters()
                            .normalized_coordinate(GAIN)
                            .unwrap()
                            .map(f64::to_bits),
                        Some(end.to_bits())
                    );
                    consumed += frames;
                }
                samples(
                    render_frames(&mut runtime, &[], 4).unwrap()[0],
                    [f64::from(u8::from(!falling)); 4],
                );
            }
        }
    }
}
