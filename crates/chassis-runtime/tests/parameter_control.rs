//! Public control policy: numeric bounds, author mapping/text, and schema identity.

use chassis_runtime::{
    audio::DEFAULT_EFFECT_CONFIGURATION,
    parameters::{
        ChoiceOption, DefaultParameterControl, ParameterControlError, ParameterControlPolicy,
        ParameterDescriptor, ParameterKind, ParameterStore, ParameterValue,
    },
    process::{ActivationConfig, ProcessConfig},
    runtime::{Component, InstanceRuntime, Processor},
    schema::{ComponentSchema, ComponentSchemaError},
    state::StateEntry,
};
use core::{convert::Infallible, fmt, num::NonZeroU32};

#[path = "../../../examples/clap-conformance/src/trim_control.rs"]
mod trim_control;
use trim_control::TrimControl;

fn trim(precision: u8) -> ParameterDescriptor {
    ParameterDescriptor::float("trim", "Trim", 0.0, 1.0, 0.5)
        .unwrap()
        .with_control_policy(TrimControl { precision })
        .unwrap()
}

#[test]
fn author_mapping_units_precision_and_plain_state_are_one_policy() {
    let descriptor = trim(1);
    for (normalized, plain) in [(0.0, 0.0), (0.5, 0.25), (1.0, 1.0)] {
        let value = ParameterValue::Float(plain);
        assert_eq!(descriptor.normalized_to_value(normalized).unwrap(), value);
        assert_eq!(
            descriptor.value_to_normalized(&value).unwrap().to_bits(),
            normalized.to_bits()
        );
    }
    let mut text = String::new();
    descriptor
        .format_value(&ParameterValue::Float(0.12345), &mut text)
        .unwrap();
    assert_eq!(text, "12.3 %");
    let ParameterValue::Float(parsed) = descriptor.parse_value(&text).unwrap() else {
        panic!("float policy");
    };
    // Decimal percent parsing introduces binary rounding, independently of
    // the deliberately coarser one-decimal display quantization.
    assert!((parsed - 0.123).abs() <= f64::EPSILON * 0.123);
    for text in ["12.3", "101 %", "-1 %", "NaN %"] {
        assert!(descriptor.parse_value(text).is_err());
    }
    assert!(
        ParameterDescriptor::float("wrong", "Wrong", 1.0, 2.0, 1.0)
            .unwrap()
            .with_control_policy(TrimControl { precision: 1 })
            .is_err()
    );
    let mut store = ParameterStore::new(&[descriptor]).unwrap();
    store.set("trim", ParameterValue::Float(0.25)).unwrap();
    let document = store.state_document("test.controls", 1).unwrap();
    assert!(
        document
            .entries()
            .iter()
            .any(|entry| entry.key() == "parameter/trim"
                && entry.value() == &chassis_runtime::state::StateValue::Float(0.25))
    );
}

#[test]
fn standard_mapping_handles_extreme_float_domains_and_exact_endpoints() {
    for (minimum, maximum) in [
        (-f64::MAX, f64::MAX),
        (f64::MAX / 2.0, f64::MAX),
        (f64::from_bits(1), f64::from_bits(2)),
        (-f64::from_bits(2), f64::from_bits(1)),
        (7.0, 7.0),
    ] {
        let descriptor = ParameterDescriptor::float("x", "X", minimum, maximum, minimum).unwrap();
        assert_eq!(
            descriptor.normalized_to_value(0.0).unwrap(),
            ParameterValue::Float(minimum)
        );
        assert_eq!(
            descriptor.normalized_to_value(1.0).unwrap(),
            ParameterValue::Float(maximum)
        );
        assert_eq!(
            descriptor
                .value_to_normalized(&ParameterValue::Float(minimum))
                .unwrap()
                .to_bits(),
            0.0_f64.to_bits()
        );
        assert_eq!(
            descriptor.normalized_to_value(0.5).unwrap(),
            ParameterValue::Float(minimum.midpoint(maximum))
        );
        for normalized in [0.01, 0.25, 0.75, 0.99] {
            let value = descriptor.normalized_to_value(normalized).unwrap();
            descriptor.validate_value(&value).unwrap();
        }
    }
}

#[test]
fn standard_discrete_mapping_and_text_use_their_declared_rounding() {
    let integer = ParameterDescriptor::integer("i", "I", -4, 4, 0).unwrap();
    assert_eq!(
        integer.normalized_to_value(0.5625).unwrap(),
        ParameterValue::Integer(1)
    );
    assert_eq!(
        integer.parse_value("-2.7").unwrap(),
        ParameterValue::Integer(-2)
    );
    assert!(integer.parse_value("-4.1").is_err());
    let boolean = ParameterDescriptor::boolean("b", "B", false).unwrap();
    assert_eq!(
        boolean.normalized_to_value(0.5).unwrap(),
        ParameterValue::Boolean(true)
    );
    assert_eq!(
        boolean.parse_value("0.99").unwrap(),
        ParameterValue::Boolean(false)
    );
    let choice = ParameterDescriptor::choice(
        "c",
        "C",
        vec![
            ChoiceOption::new("first-id", "First").unwrap(),
            ChoiceOption::new("last-id", "Last").unwrap(),
        ],
        "first-id",
    )
    .unwrap();
    assert_eq!(
        choice.normalized_to_value(0.5).unwrap(),
        choice.parse_value("Last").unwrap()
    );
    let value = choice.parse_value(" First ").unwrap();
    assert_eq!(
        choice
            .value_from_event(choice.event_from_value(&value).unwrap())
            .unwrap(),
        value
    );
    let wide = ParameterDescriptor::integer("wide", "Wide", i64::MIN, i64::MAX, 0).unwrap();
    for value in [i64::MIN, i64::MAX - 1, i64::MAX] {
        let mut text = String::new();
        wide.format_value(&ParameterValue::Integer(value), &mut text)
            .unwrap();
        assert_eq!(
            wide.parse_value(&text).unwrap(),
            ParameterValue::Integer(value)
        );
    }
    for text in [
        "9223372036854775808",
        "-9223372036854775809",
        "-9223372036854775809.0",
    ] {
        assert!(wide.parse_value(text).is_err());
    }
    assert_eq!(
        wide.normalized_to_value(0.0).unwrap(),
        ParameterValue::Integer(i64::MIN)
    );
    assert_eq!(
        wide.normalized_to_value(1.0).unwrap(),
        ParameterValue::Integer(i64::MAX)
    );
    let constant =
        ParameterDescriptor::integer("fixed", "Fixed", i64::MAX, i64::MAX, i64::MAX).unwrap();
    assert_eq!(
        constant.normalized_to_value(0.25).unwrap(),
        ParameterValue::Integer(i64::MAX)
    );
}

#[derive(Debug, PartialEq)]
struct BadInterior;
impl ParameterControlPolicy for BadInterior {
    fn value_to_normalized(
        &self,
        kind: &ParameterKind,
        value: &ParameterValue,
    ) -> Result<f64, ParameterControlError> {
        if value == &ParameterValue::Float(0.25) {
            Ok(f64::NAN)
        } else {
            DefaultParameterControl.value_to_normalized(kind, value)
        }
    }
    fn normalized_to_value(
        &self,
        kind: &ParameterKind,
        value: f64,
    ) -> Result<ParameterValue, ParameterControlError> {
        if value.to_bits() == 0.25_f64.to_bits() {
            Ok(ParameterValue::Boolean(true))
        } else {
            DefaultParameterControl.normalized_to_value(kind, value)
        }
    }
    fn parse(&self, _: &ParameterKind, _: &str) -> Result<ParameterValue, ParameterControlError> {
        Ok(ParameterValue::Float(2.0))
    }
    fn format(
        &self,
        _: &ParameterKind,
        _: &ParameterValue,
        writer: &mut dyn fmt::Write,
    ) -> fmt::Result {
        let _ = writer.write_str("ignored error");
        Ok(())
    }
}
struct RejectWriter;
impl fmt::Write for RejectWriter {
    fn write_str(&mut self, _: &str) -> fmt::Result {
        Err(fmt::Error)
    }
}

#[test]
fn policy_outputs_and_writer_failures_are_checked_after_admission() {
    let descriptor = ParameterDescriptor::float("x", "X", 0.0, 1.0, 0.5)
        .unwrap()
        .with_control_policy(BadInterior)
        .unwrap();
    for normalized in [f64::NAN, f64::INFINITY, -0.1, 1.1, 0.25] {
        assert!(descriptor.normalized_to_value(normalized).is_err());
    }
    assert!(
        descriptor
            .value_to_normalized(&ParameterValue::Float(0.25))
            .is_err()
    );
    assert!(
        descriptor
            .value_to_normalized(&ParameterValue::Boolean(true))
            .is_err()
    );
    assert!(descriptor.parse_value("anything").is_err());
    assert_eq!(
        descriptor.format_value(&ParameterValue::Float(0.5), &mut RejectWriter),
        Err(ParameterControlError::Formatting)
    );
}

struct Definition(u8);
struct Dsp;
impl Processor for Dsp {}
impl Component for Definition {
    type Processor = Dsp;
    type ActivationError = Infallible;
    fn schema(&self) -> Result<ComponentSchema, ComponentSchemaError> {
        ComponentSchema::stereo_effect(vec![trim(self.0)])
    }
    fn activate(
        &self,
        _: &ActivationConfig<'_>,
        _: &ParameterStore,
        _: &[StateEntry],
    ) -> Result<Dsp, Infallible> {
        Ok(Dsp)
    }
}

#[test]
fn frozen_schema_compares_policy_configuration_not_allocation_addresses() {
    let config = ProcessConfig::new(
        48_000.0,
        NonZeroU32::new(1),
        NonZeroU32::new(64).unwrap(),
        0,
    )
    .unwrap();
    let mut runtime = InstanceRuntime::for_component(&Definition(1)).unwrap();
    runtime
        .activate(&Definition(1), config, DEFAULT_EFFECT_CONFIGURATION)
        .unwrap();
    runtime.deactivate().unwrap();
    assert!(
        runtime
            .activate(&Definition(2), config, DEFAULT_EFFECT_CONFIGURATION)
            .is_err()
    );
}
