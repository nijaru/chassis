//! Prepared float normalization and canonical normalized-value provenance.

use core::{any::Any, fmt};
use std::sync::Arc;

use super::{
    ParameterControlError, ParameterDefinitionError, ParameterDescriptor, ParameterIndex,
    ParameterKind, ParameterStore, ParameterStoreError, ParameterValue, ParameterValueError,
    StoredParameterValue,
};

/// An immutable, already-prepared float mapping, independent of text display.
///
/// Attach a value-comparable configuration with
/// [`ParameterDescriptor::with_float_mapping`]. The same object owns both
/// normalization directions; configuration equality must cover every
/// behavior-affecting setting and participates in frozen-schema validation.
///
/// The forward method has an author-enforced realtime contract, like product
/// DSP. Admission probes do not prove its execution bounds or whole-domain
/// validity. The framework checks every consumed input/output and never clamps
/// an invalid mapped value into apparent validity.
pub trait FloatMapping: fmt::Debug + Send + Sync + 'static {
    /// Admit this prepared configuration against a validated plain domain.
    ///
    /// Called off-thread. Reject unsupported domains or incomplete preparation.
    /// Forward evaluation must be valid over ALL finite coordinates in 0..=1.
    ///
    /// # Errors
    /// Returns an error when this configuration cannot support the domain.
    fn validate(&self, minimum: f64, maximum: f64) -> Result<(), ParameterControlError>;

    /// Map a validated finite 0..=1 coordinate to the admitted plain domain.
    ///
    /// Must be deterministic, bounded, nonblocking and allocation/deallocation
    /// free, without mutable shared state. It must not call presentation hooks.
    /// Use descriptor/store operations to validate results, not this raw hook.
    fn normalized_to_plain(&self, normalized: f64) -> f64;

    /// Return a representative coordinate for a validated plain value.
    ///
    /// This is NON-REALTIME presentation conversion, not reconstruction of an
    /// earlier source coordinate. Non-injective mappings may choose a canonical
    /// representative. It never fills the store's retained-coordinate field.
    ///
    /// # Errors
    /// Returns an error when no supported representative can be supplied.
    fn plain_to_normalized(&self, plain: f64) -> Result<f64, ParameterControlError>;
}

#[derive(Debug, Clone)]
pub(super) struct Mapping(Arc<dyn ErasedMapping>);
trait ErasedMapping: FloatMapping {
    fn as_any(&self) -> &dyn Any;
    fn equal(&self, other: &dyn ErasedMapping) -> bool;
}
impl<T: FloatMapping + PartialEq> ErasedMapping for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn equal(&self, other: &dyn ErasedMapping) -> bool {
        other
            .as_any()
            .downcast_ref::<Self>()
            .is_some_and(|other| self == other)
    }
}
impl PartialEq for Mapping {
    fn eq(&self, other: &Self) -> bool {
        self.0.equal(other.0.as_ref())
    }
}
impl Mapping {
    pub(super) fn linear(minimum: f64, maximum: f64) -> Self {
        Self(Arc::new(LinearMapping { minimum, maximum }))
    }
}

#[derive(Debug)]
struct LinearMapping {
    minimum: f64,
    maximum: f64,
}
impl PartialEq for LinearMapping {
    fn eq(&self, other: &Self) -> bool {
        self.minimum.to_bits() == other.minimum.to_bits()
            && self.maximum.to_bits() == other.maximum.to_bits()
    }
}
impl FloatMapping for LinearMapping {
    fn validate(&self, minimum: f64, maximum: f64) -> Result<(), ParameterControlError> {
        if self.minimum.to_bits() == minimum.to_bits()
            && self.maximum.to_bits() == maximum.to_bits()
        {
            Ok(())
        } else {
            Err(ParameterControlError::InvalidPolicy(
                "linear mapping domain changed",
            ))
        }
    }
    fn normalized_to_plain(&self, normalized: f64) -> f64 {
        let ParameterValue::Float(value) = linear_from_normalized(
            &ParameterKind::Float {
                minimum: self.minimum,
                maximum: self.maximum,
                default: self.minimum,
            },
            normalized,
        )
        .expect("validated float mapping has a finite domain and normalized input") else {
            unreachable!("linear float mapping returns a float")
        };
        value
    }
    fn plain_to_normalized(&self, plain: f64) -> Result<f64, ParameterControlError> {
        linear_to_normalized(
            &ParameterKind::Float {
                minimum: self.minimum,
                maximum: self.maximum,
                default: self.minimum,
            },
            &ParameterValue::Float(plain),
        )
    }
}

/// Failure to read or establish a normalized float base using dense identity.
///
/// This contains no allocated key. Rejection leaves both base and provenance
/// unchanged; it does not request callback recovery after product DSP failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterMappingError {
    /// Dense index does not identify a parameter in this schema.
    UnknownParameter(ParameterIndex),
    /// Domain, coordinate, or mapping result was invalid.
    Mapping(ParameterControlError),
}
impl fmt::Display for ParameterMappingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownParameter(index) => {
                write!(formatter, "unknown parameter index {}", index.get())
            }
            Self::Mapping(error) => error.fmt(formatter),
        }
    }
}
impl std::error::Error for ParameterMappingError {}

impl ParameterDescriptor {
    /// Replace this float's prepared normalization capability.
    ///
    /// Both presentation conversion directions and normalized base edits use
    /// this mapping. Text policy remains separate. No preparation or owning
    /// mapping clone/drop occurs during dense normalized mutation.
    ///
    /// # Errors
    /// Rejects non-floats, incompatible configuration and invalid admission probes.
    pub fn with_float_mapping<M: FloatMapping + PartialEq>(
        mut self,
        mapping: M,
    ) -> Result<Self, ParameterDefinitionError> {
        if self.mapping.is_none() {
            return Err(ParameterDefinitionError::InvalidControlPolicy {
                parameter: self.key.clone(),
                reason: ParameterControlError::InvalidPolicy(
                    "float mapping requires a float parameter",
                ),
            });
        }
        self.mapping = Some(Mapping(Arc::new(mapping)));
        self.validate()?;
        Ok(self)
    }

    pub(super) fn validate_mapping(&self) -> Result<(), ParameterControlError> {
        if let ParameterKind::Float {
            minimum, maximum, ..
        } = self.kind
        {
            self.mapping
                .as_ref()
                .expect("float descriptors own a prepared mapping")
                .0
                .validate(minimum, maximum)?;
        }
        // Probe basic usability, not whole-domain validity, inverse identity,
        // monotonicity, or the author's realtime execution bounds.
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

    /// Convert a plain value to a representative normalized control position.
    ///
    /// Non-realtime. This does not recover retained source history. For the
    /// current float store value, prefer [`ParameterStore::normalized_value`].
    ///
    /// # Errors
    /// Rejects invalid plain inputs and invalid mapping results.
    pub fn value_to_normalized(
        &self,
        value: &ParameterValue,
    ) -> Result<f64, ParameterControlError> {
        self.validate_value(value)
            .map_err(ParameterControlError::Value)?;
        let normalized = match (&self.mapping, value) {
            (Some(mapping), ParameterValue::Float(value)) => {
                mapping.0.plain_to_normalized(*value)?
            }
            _ => linear_to_normalized(&self.kind, value)?,
        };
        validate_normalized(normalized)?;
        Ok(normalized)
    }

    /// Convert a normalized position to a validated typed plain value.
    ///
    /// Discrete domains retain built-in nearest-step mapping (ties upward).
    /// This general control method is non-realtime: choice conversion can own
    /// text. Dense float mutation uses only the prepared forward capability.
    ///
    /// # Errors
    /// Rejects invalid coordinates and results outside the descriptor domain.
    pub fn normalized_to_value(
        &self,
        normalized: f64,
    ) -> Result<ParameterValue, ParameterControlError> {
        validate_normalized(normalized)?;
        let value = if self.mapping.is_some() {
            ParameterValue::Float(self.mapped_float(normalized)?)
        } else {
            linear_from_normalized(&self.kind, normalized)?
        };
        self.validate_value(&value)
            .map_err(ParameterControlError::Value)?;
        Ok(value)
    }

    fn mapped_float(&self, normalized: f64) -> Result<f64, ParameterControlError> {
        validate_normalized(normalized)?;
        let mapping = self
            .mapping
            .as_ref()
            .ok_or(ParameterControlError::InvalidPolicy(
                "normalized source coordinates require a float parameter",
            ))?;
        let plain = mapping.0.normalized_to_plain(normalized);
        self.validate_value(&ParameterValue::Float(plain))
            .map_err(ParameterControlError::Value)?;
        Ok(plain)
    }
}

impl ParameterStore {
    /// Replace a float base from a normalized control coordinate by stable key.
    ///
    /// This is a non-realtime convenience. Use [`Self::set_normalized_index`]
    /// for dense, allocation-free access.
    ///
    /// # Errors
    /// Rejects unknown keys, non-floats, invalid coordinates or mapped results.
    pub fn set_normalized(
        &mut self,
        key: impl AsRef<str>,
        normalized: f64,
    ) -> Result<(), ParameterStoreError> {
        let key = key.as_ref();
        let position = self
            .position_for_key(key)
            .ok_or_else(|| ParameterStoreError::UnknownParameter(key.to_owned()))?;
        self.replace_normalized(position, normalized)
            .map_err(|error| ParameterStoreError::InvalidMapping {
                parameter: key.to_owned(),
                error,
            })
    }

    /// Atomically establish mapped plain base and exact normalized provenance.
    ///
    /// Failure-atomic under exclusive access, not cross-thread atomic publication.
    /// The accepted scalar edit establishes the next base immediately; it is not
    /// timed automation completion and does not await a DSP callback.
    /// Both success and rejection avoid framework allocation/deallocation.
    ///
    /// # Errors
    /// Rejects unknown indices, non-floats and invalid mapping inputs/results.
    pub fn set_normalized_index(
        &mut self,
        index: ParameterIndex,
        normalized: f64,
    ) -> Result<(), ParameterMappingError> {
        let position = self.mapping_position(index)?;
        self.replace_normalized(position, normalized)
            .map_err(ParameterMappingError::Mapping)
    }

    /// Read exact normalized provenance without invoking inverse conversion.
    ///
    /// `None` means a default/plain edit or successful plain state replacement
    /// established this base. An equal-value plain replacement still clears it.
    /// This read is allocation-free and does not invoke any author hook.
    ///
    /// # Errors
    /// Rejects unknown indices and non-float parameters.
    pub fn normalized_coordinate(
        &self,
        index: ParameterIndex,
    ) -> Result<Option<f64>, ParameterMappingError> {
        let position = self.mapping_position(index)?;
        Ok(self.values[position].source_coordinate)
    }

    /// Present the current float position, preferring exact retained provenance.
    ///
    /// NON-REALTIME: a plain-origin value uses the mapper's representative
    /// inverse. That conversion never changes the store or reconstructs history.
    ///
    /// # Errors
    /// Rejects unknown indices, non-floats and failed inverse conversion.
    pub fn normalized_value(&self, index: ParameterIndex) -> Result<f64, ParameterMappingError> {
        let position = self.mapping_position(index)?;
        match self.values[position].source_coordinate {
            Some(coordinate) => Ok(coordinate),
            None => self.descriptors[position]
                .value_to_normalized(&self.values[position].value)
                .map_err(ParameterMappingError::Mapping),
        }
    }

    fn mapping_position(&self, index: ParameterIndex) -> Result<usize, ParameterMappingError> {
        let position = usize::try_from(index.get())
            .map_err(|_| ParameterMappingError::UnknownParameter(index))?;
        let descriptor = self
            .descriptors
            .get(position)
            .ok_or(ParameterMappingError::UnknownParameter(index))?;
        if descriptor.mapping.is_none() {
            return Err(ParameterMappingError::Mapping(
                ParameterControlError::InvalidPolicy(
                    "normalized source coordinates require a float parameter",
                ),
            ));
        }
        Ok(position)
    }

    fn replace_normalized(
        &mut self,
        position: usize,
        normalized: f64,
    ) -> Result<(), ParameterControlError> {
        let plain = self.descriptors[position].mapped_float(normalized)?;
        self.values[position] = StoredParameterValue {
            value: ParameterValue::Float(plain),
            choice_index: None,
            source_coordinate: Some(normalized),
        };
        Ok(())
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
            let offset = nearest_step(normalized, minimum.abs_diff(*maximum));
            let value = i64::try_from(i128::from(*minimum) + i128::from(offset))
                .map_err(|_| ParameterControlError::InvalidNormalized)?;
            ParameterValue::Integer(value)
        }
        ParameterKind::Boolean { .. } => ParameterValue::Boolean(normalized >= 0.5),
        ParameterKind::Choice { options, .. } => {
            let span = u64::try_from(options.len() - 1)
                .map_err(|_| ParameterControlError::InvalidPolicy("too many choices"))?;
            let index = usize::try_from(nearest_step(normalized, span))
                .expect("normalized choice offset is bounded by its option count");
            let option = options
                .get(index)
                .ok_or(ParameterControlError::InvalidNormalized)?;
            ParameterValue::Choice(option.id().clone())
        }
    })
}

// Round the exact binary control coordinate times an integer span. Converting
// the span to f64 first loses low bits and can reverse a wide-domain half tie.
// Validated coordinates are in 0..=1, so the denominator shift is at least 52
// and the product fits in 117 bits even for the full-width i64 domain.
fn nearest_step(normalized: f64, span: u64) -> u64 {
    let bits = normalized.to_bits();
    let exponent = (bits >> 52) & 0x7ff;
    let fraction = bits & ((1_u64 << 52) - 1);
    let (significand, shift) = if exponent == 0 {
        (fraction, 1074)
    } else {
        (fraction | (1_u64 << 52), 1075 - exponent)
    };
    if shift >= 128 {
        return 0;
    }
    let product = u128::from(significand) * u128::from(span);
    let rounded = (product >> shift) + ((product >> (shift - 1)) & 1);
    u64::try_from(rounded).expect("normalized offset is bounded by its integer span")
}
