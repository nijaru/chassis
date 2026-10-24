//! Modulation admission and composition independent of persistence/control mapping.
use chassis_runtime::{
    automation::{ParameterEvent, ParameterEvents, ParameterEventsError},
    parameters::{ChoiceOption, ParameterDescriptor, ParameterIndex, ParameterValueError},
};

#[test]
fn only_explicit_float_domains_admit_finite_unbounded_amounts() {
    let plain = ParameterDescriptor::float("gain", "Gain", 0.0, 1.0, 0.5).unwrap();
    assert!(plain.global_modulation().is_none());
    assert_eq!(
        plain.validate_modulation_amount(0.0),
        Err(ParameterValueError::ModulationNotSupported)
    );
    let enabled = plain.clone().with_global_modulation().unwrap();
    assert_ne!(plain, enabled);
    for amount in [-f64::MAX, 0.0, f64::MAX] {
        enabled.validate_modulation_amount(amount).unwrap();
    }
    for amount in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            enabled.validate_modulation_amount(amount),
            Err(ParameterValueError::NonFiniteFloat)
        );
        let raw = [ParameterEvent::modulation(
            0,
            ParameterIndex::new(0),
            amount,
        )];
        assert_eq!(
            ParameterEvents::new(&raw, 1, 1),
            Err(ParameterEventsError::NonFiniteValue { index: 0 })
        );
    }
    for descriptor in [
        ParameterDescriptor::integer("count", "Count", 0, 2, 0).unwrap(),
        ParameterDescriptor::boolean("mute", "Mute", false).unwrap(),
        ParameterDescriptor::choice(
            "mode",
            "Mode",
            vec![ChoiceOption::new("one", "One").unwrap()],
            "one",
        )
        .unwrap(),
    ] {
        assert!(descriptor.with_global_modulation().is_err());
    }
}

#[test]
fn effective_sum_saturates_without_clamping_or_persisting_the_signal() {
    let index = ParameterIndex::new(0);
    let spec = ParameterDescriptor::float("wide", "Wide", -f64::MAX, f64::MAX, 0.0)
        .unwrap()
        .with_global_modulation()
        .unwrap()
        .global_modulation()
        .unwrap();
    for (base, amount, expected) in [
        (f64::MAX, f64::MAX, f64::MAX),
        (-f64::MAX, -f64::MAX, -f64::MAX),
        (f64::MAX, -f64::MAX, 0.0),
    ] {
        let mut cursor = spec
            .cursor(ParameterEvents::empty_for_block(1), index, base, amount)
            .unwrap();
        assert_eq!(cursor.value_at(0).unwrap().to_bits(), expected.to_bits());
    }
}
