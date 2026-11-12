//! Non-realtime text presentation, independent of prepared float mapping.

use core::{any::Any, fmt};
use std::sync::Arc;

use super::{
    ParameterDefinitionError, ParameterDescriptor, ParameterKind, ParameterValue,
    ParameterValueError,
};

/// Failure to map or present a parameter.
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

/// Immutable author-defined units, precision, and text syntax.
///
/// Attach a value-comparable configuration with
/// [`ParameterDescriptor::with_control_policy`]. Use the descriptor's methods,
/// not these hooks directly: they validate typed inputs and outputs. Policies
/// must be deterministic and immutable; equality must compare every
/// configuration value that changes behavior.
///
/// These are non-realtime hooks. Float normalization belongs exclusively to
/// [`super::FloatMapping`], not this presentation policy. Display rounding may
/// be lossy; authors define precision and supported text round trips together.
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

/// Default plain-unit numeric text and choice names.
///
/// Numeric stepped text truncates toward zero after checking the unrounded
/// domain. Exact integer text preserves the whole `i64` range; fractional text
/// requires magnitude below `2^53`. Choice text trims outer whitespace and
/// rejects ambiguous names.
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
        self.control_policy().validate(&self.kind)
    }

    /// Return this descriptor's non-realtime control policy.
    #[must_use]
    pub fn control_policy(&self) -> &dyn ParameterControlPolicy {
        self.control
            .as_ref()
            .map_or(&DefaultParameterControl, |policy| policy.0.as_ref())
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
