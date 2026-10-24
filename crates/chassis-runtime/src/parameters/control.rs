//! Non-realtime control mapping and text policy, separate from DSP trajectories.

use core::{any::Any, fmt};
use std::sync::Arc;

use super::{
    ParameterDefinitionError, ParameterDescriptor, ParameterKind, ParameterValue,
    ParameterValueError,
};

/// Failure to map or present a parameter on the control thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterControlError {
    /// A typed input or policy result violates the descriptor's domain.
    Value(ParameterValueError),
    /// A normalized input or policy result is not finite and within `0..=1`.
    InvalidNormalized,
    /// Text does not represent a value accepted by this policy.
    InvalidText,
    /// The policy cannot support this domain or configuration.
    InvalidPolicy(&'static str),
    /// The formatter or output writer rejected the text.
    Formatting,
}

impl fmt::Display for ParameterControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Value(error) => error.fmt(formatter),
            Self::InvalidNormalized => {
                formatter.write_str("normalized value must be finite and within 0..=1")
            }
            Self::InvalidText => formatter.write_str("invalid parameter text"),
            Self::InvalidPolicy(reason) => write!(formatter, "invalid control policy: {reason}"),
            Self::Formatting => formatter.write_str("parameter text could not be written"),
        }
    }
}
impl std::error::Error for ParameterControlError {}

/// Immutable author-defined control mapping, units, precision, and text syntax.
///
/// Attach a value-comparable configuration with
/// [`ParameterDescriptor::with_control_policy`]. Use the descriptor's methods,
/// not these hooks directly: they validate typed domains and normalized inputs
/// and outputs. Policies must be deterministic and immutable; equality must
/// compare every configuration value that changes behavior.
///
/// These are non-realtime hooks. They do not transform stored plain units,
/// automation trajectories, or modulation. Display rounding may be lossy;
/// authors define the precision and supported text round trips together.
/// Unoverridden hooks use [`DefaultParameterControl`]'s behavior.
pub trait ParameterControlPolicy: fmt::Debug + Send + Sync + 'static {
    /// Check domain/configuration compatibility during schema validation.
    ///
    /// # Errors
    ///
    /// Return an error when this policy cannot support the validated domain.
    fn validate(&self, _kind: &ParameterKind) -> Result<(), ParameterControlError> {
        Ok(())
    }

    /// Map a validated plain-unit value to a finite normalized control position.
    ///
    /// # Errors
    ///
    /// Return an error if this value cannot be mapped.
    fn value_to_normalized(
        &self,
        kind: &ParameterKind,
        value: &ParameterValue,
    ) -> Result<f64, ParameterControlError> {
        linear_to_normalized(kind, value)
    }

    /// Map a finite `0..=1` control position to a typed plain-unit value.
    ///
    /// # Errors
    ///
    /// Return an error if this position cannot be mapped.
    fn normalized_to_value(
        &self,
        kind: &ParameterKind,
        normalized: f64,
    ) -> Result<ParameterValue, ParameterControlError> {
        linear_from_normalized(kind, normalized)
    }

    /// Write display text, including any product-defined units and precision.
    ///
    /// # Errors
    ///
    /// Return an error when formatting or the output writer fails.
    fn format(
        &self,
        kind: &ParameterKind,
        value: &ParameterValue,
        writer: &mut dyn fmt::Write,
    ) -> fmt::Result {
        format_plain(kind, value, writer)
    }

    /// Parse product text into a typed plain-unit value.
    ///
    /// # Errors
    ///
    /// Return an error for unsupported text. The descriptor checks the result's
    /// type, range, and stable choice identity before returning it to the caller.
    fn parse(
        &self,
        kind: &ParameterKind,
        text: &str,
    ) -> Result<ParameterValue, ParameterControlError> {
        parse_plain(kind, text)
    }
}

/// Default linear control mapping and plain-unit text.
///
/// Normalized discrete controls choose the nearest step, with ties toward the
/// higher step; boolean switches at `0.5`. Constant domains map to zero and every
/// valid normalized position maps back to the constant. Endpoints are exact.
/// Interior round trips are limited by floating-point resolution, including for
/// wide integer domains; text preserves exact integer values.
///
/// Text uses numeric `Display`, `0`/`1` booleans, and choice names. Numeric
/// stepped text truncates toward zero after checking its unrounded domain;
/// non-integer spellings require magnitude below `2^53`. Choice text trims outer
/// whitespace and rejects ambiguous names.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DefaultParameterControl;
impl ParameterControlPolicy for DefaultParameterControl {}

#[derive(Debug, Clone)]
pub(super) struct ControlPolicy(Arc<dyn ErasedPolicy>);
trait ErasedPolicy: ParameterControlPolicy {
    fn as_any(&self) -> &dyn Any;
    fn equal(&self, other: &dyn ErasedPolicy) -> bool;
}
impl<T: ParameterControlPolicy + PartialEq> ErasedPolicy for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn equal(&self, other: &dyn ErasedPolicy) -> bool {
        other
            .as_any()
            .downcast_ref::<Self>()
            .is_some_and(|other| self == other)
    }
}
impl PartialEq for ControlPolicy {
    fn eq(&self, other: &Self) -> bool {
        self.0.equal(other.0.as_ref())
    }
}

impl ParameterDescriptor {
    /// Attach immutable author policy and validate it against this domain.
    ///
    /// Configuration equality participates in frozen-schema drift detection;
    /// pointer addresses and Rust type names are not semantic/persistent IDs.
    ///
    /// # Errors
    ///
    /// Returns [`ParameterDefinitionError`] if the policy rejects this domain.
    pub fn with_control_policy<P: ParameterControlPolicy + PartialEq>(
        mut self,
        policy: P,
    ) -> Result<Self, ParameterDefinitionError> {
        self.control = Some(ControlPolicy(Arc::new(policy)));
        self.validate()?;
        Ok(self)
    }

    pub(super) fn validate_control(&self) -> Result<(), ParameterControlError> {
        self.control_policy().validate(&self.kind)?;
        // Probe basic usability, not a proof of arbitrary policy inverses or
        // monotonicity. Display rounding is deliberately not constrained here.
        let (minimum, maximum) = match &self.kind {
            ParameterKind::Float {
                minimum, maximum, ..
            } => (
                ParameterValue::Float(*minimum),
                ParameterValue::Float(*maximum),
            ),
            ParameterKind::Integer {
                minimum, maximum, ..
            } => (
                ParameterValue::Integer(*minimum),
                ParameterValue::Integer(*maximum),
            ),
            ParameterKind::Boolean { .. } => (
                ParameterValue::Boolean(false),
                ParameterValue::Boolean(true),
            ),
            ParameterKind::Choice { options, .. } => (
                ParameterValue::Choice(options[0].id().clone()),
                ParameterValue::Choice(options[options.len() - 1].id().clone()),
            ),
        };
        for value in [minimum, self.default_value(), maximum] {
            let normalized = self.value_to_normalized(&value)?;
            self.normalized_to_value(normalized)?;
        }
        for normalized in [0.0, 1.0] {
            self.normalized_to_value(normalized)?;
        }
        Ok(())
    }

    /// Return this descriptor's non-realtime control policy.
    #[must_use]
    pub fn control_policy(&self) -> &dyn ParameterControlPolicy {
        self.control
            .as_ref()
            .map_or(&DefaultParameterControl, |policy| policy.0.as_ref())
    }

    /// Map a validated typed value to a finite normalized control position.
    ///
    /// # Errors
    ///
    /// Rejects invalid typed inputs and non-finite/out-of-range policy results.
    pub fn value_to_normalized(
        &self,
        value: &ParameterValue,
    ) -> Result<f64, ParameterControlError> {
        self.validate_value(value)
            .map_err(ParameterControlError::Value)?;
        let normalized = self
            .control_policy()
            .value_to_normalized(&self.kind, value)?;
        validate_normalized(normalized)?;
        Ok(normalized)
    }

    /// Map a normalized control position to a validated typed plain-unit value.
    ///
    /// # Errors
    ///
    /// Rejects invalid normalized inputs and typed policy results.
    pub fn normalized_to_value(
        &self,
        normalized: f64,
    ) -> Result<ParameterValue, ParameterControlError> {
        validate_normalized(normalized)?;
        let value = self
            .control_policy()
            .normalized_to_value(&self.kind, normalized)?;
        self.validate_value(&value)
            .map_err(ParameterControlError::Value)?;
        Ok(value)
    }

    /// Format a typed value through the non-realtime presentation policy.
    ///
    /// # Errors
    ///
    /// Rejects invalid typed inputs and propagates formatting/writer failure.
    /// A failing writer may contain partial text; callers must not publish it.
    pub fn format_value(
        &self,
        value: &ParameterValue,
        writer: &mut dyn fmt::Write,
    ) -> Result<(), ParameterControlError> {
        self.validate_value(value)
            .map_err(ParameterControlError::Value)?;
        let mut writer = CheckedWriter {
            writer,
            failed: false,
        };
        let result = self.control_policy().format(&self.kind, value, &mut writer);
        if result.is_err() || writer.failed {
            Err(ParameterControlError::Formatting)
        } else {
            Ok(())
        }
    }

    /// Parse text through the policy, then validate its typed result.
    ///
    /// # Errors
    ///
    /// Rejects unsupported syntax and invalid policy results without changing a
    /// parameter store. Custom parsers own whitespace, units, and quantization.
    pub fn parse_value(&self, text: &str) -> Result<ParameterValue, ParameterControlError> {
        let value = self.control_policy().parse(&self.kind, text)?;
        self.validate_value(&value)
            .map_err(ParameterControlError::Value)?;
        Ok(value)
    }
}

struct CheckedWriter<'a> {
    writer: &'a mut dyn fmt::Write,
    failed: bool,
}
impl fmt::Write for CheckedWriter<'_> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        if self.failed || self.writer.write_str(text).is_err() {
            self.failed = true;
            Err(fmt::Error)
        } else {
            Ok(())
        }
    }
}

fn validate_normalized(value: f64) -> Result<(), ParameterControlError> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(ParameterControlError::InvalidNormalized)
    }
}

#[allow(
    clippy::cast_precision_loss,
    reason = "normalized f64 controls have finite resolution; integer differences remain exact before conversion"
)]
#[expect(
    clippy::float_cmp,
    reason = "endpoint and constant-domain identity must be exact, not an epsilon neighborhood"
)]
fn linear_to_normalized(
    kind: &ParameterKind,
    value: &ParameterValue,
) -> Result<f64, ParameterControlError> {
    let normalized = match (kind, value) {
        (
            ParameterKind::Float {
                minimum, maximum, ..
            },
            ParameterValue::Float(value),
        ) => {
            if minimum == maximum {
                0.0
            } else if value == maximum {
                1.0
            } else {
                let span = maximum - minimum;
                if span.is_finite() {
                    (value - minimum) / span
                } else {
                    (value * 0.5 - minimum * 0.5) / (maximum * 0.5 - minimum * 0.5)
                }
            }
        }
        (
            ParameterKind::Integer {
                minimum, maximum, ..
            },
            ParameterValue::Integer(value),
        ) => {
            let span = i128::from(*maximum) - i128::from(*minimum);
            if span == 0 {
                0.0
            } else {
                (i128::from(*value) - i128::from(*minimum)) as f64 / span as f64
            }
        }
        (ParameterKind::Boolean { .. }, ParameterValue::Boolean(value)) => {
            f64::from(u8::from(*value))
        }
        (ParameterKind::Choice { options, .. }, ParameterValue::Choice(value)) => {
            let index = options
                .iter()
                .position(|option| option.id() == value)
                .ok_or(ParameterControlError::Value(
                    ParameterValueError::UnknownChoice,
                ))?;
            if options.len() == 1 {
                0.0
            } else {
                index as f64 / (options.len() - 1) as f64
            }
        }
        _ => {
            return Err(ParameterControlError::Value(
                ParameterValueError::WrongType {
                    expected: kind.value_type(),
                    actual: value.value_type(),
                },
            ));
        }
    };
    Ok(normalized)
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "finite normalized input rounds a nonnegative bounded offset; endpoints bypass floating-point integer conversion"
)]
#[expect(
    clippy::float_cmp,
    reason = "endpoint and constant-domain identity must be exact, not an epsilon neighborhood"
)]
fn linear_from_normalized(
    kind: &ParameterKind,
    normalized: f64,
) -> Result<ParameterValue, ParameterControlError> {
    Ok(match kind {
        ParameterKind::Float {
            minimum, maximum, ..
        } => {
            let value = if normalized == 0.0 || minimum == maximum {
                *minimum
            } else if normalized == 1.0 {
                *maximum
            } else {
                let span = maximum - minimum;
                if span.is_finite() {
                    normalized.mul_add(span, *minimum)
                } else {
                    // An overflowing span implies opposite signs. The weighted
                    // terms are finite and their sum cannot overflow.
                    (1.0 - normalized) * minimum + normalized * maximum
                }
            };
            ParameterValue::Float(value)
        }
        ParameterKind::Integer {
            minimum, maximum, ..
        } => {
            let value = if normalized == 0.0 {
                *minimum
            } else if normalized == 1.0 {
                *maximum
            } else {
                let span = i128::from(*maximum) - i128::from(*minimum);
                let offset = (normalized * span as f64).round() as i128;
                i64::try_from(i128::from(*minimum) + offset)
                    .map_err(|_| ParameterControlError::InvalidNormalized)?
            };
            ParameterValue::Integer(value)
        }
        ParameterKind::Boolean { .. } => ParameterValue::Boolean(normalized >= 0.5),
        ParameterKind::Choice { options, .. } => {
            let index = (normalized * (options.len() - 1) as f64).round() as usize;
            let option = options
                .get(index)
                .ok_or(ParameterControlError::InvalidNormalized)?;
            ParameterValue::Choice(option.id().clone())
        }
    })
}

fn format_plain(
    kind: &ParameterKind,
    value: &ParameterValue,
    writer: &mut dyn fmt::Write,
) -> fmt::Result {
    match value {
        ParameterValue::Float(value) => write!(writer, "{value}"),
        ParameterValue::Integer(value) => write!(writer, "{value}"),
        ParameterValue::Boolean(value) => write!(writer, "{}", u8::from(*value)),
        ParameterValue::Choice(value) => {
            let ParameterKind::Choice { options, .. } = kind else {
                return Err(fmt::Error);
            };
            let option = options
                .iter()
                .find(|option| option.id() == value)
                .ok_or(fmt::Error)?;
            writer.write_str(option.name())
        }
    }
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "fractional numeric text is explicitly truncated after finite/domain and i64 representability checks; integer text takes the exact path"
)]
fn parse_plain(kind: &ParameterKind, text: &str) -> Result<ParameterValue, ParameterControlError> {
    let text = text.trim();
    let parse_float = || {
        text.parse::<f64>()
            .map_err(|_| ParameterControlError::InvalidText)
    };
    Ok(match kind {
        ParameterKind::Float { .. } => ParameterValue::Float(parse_float()?),
        ParameterKind::Integer {
            minimum, maximum, ..
        } => {
            let value = if let Ok(value) = text.parse::<i64>() {
                value
            } else {
                let value = parse_float()?;
                if !value.is_finite()
                    || value < *minimum as f64
                    || value > *maximum as f64
                    || value.abs() >= 9_007_199_254_740_992.0
                {
                    return Err(ParameterControlError::InvalidText);
                }
                value.trunc() as i64
            };
            ParameterValue::Integer(value)
        }
        ParameterKind::Boolean { .. } => {
            let value = parse_float()?;
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err(ParameterControlError::InvalidText);
            }
            ParameterValue::Boolean(value.trunc() != 0.0)
        }
        ParameterKind::Choice { options, .. } => {
            let mut matches = options.iter().filter(|option| option.name() == text);
            let option = matches.next().ok_or(ParameterControlError::InvalidText)?;
            if matches.next().is_some() {
                return Err(ParameterControlError::InvalidText);
            }
            ParameterValue::Choice(option.id().clone())
        }
    })
}
