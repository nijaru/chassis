//! Explicit global additive float modulation, independent of durable base values.

use crate::automation::{FloatParameterCursor, ParameterCursorError};

/// Setup-resolved global modulation semantics for an opted-in float parameter.
///
/// Obtain this from [`ParameterDescriptor::global_modulation`](crate::parameters::ParameterDescriptor::global_modulation)
/// and cache it with the parameter's dense index before activation completes.
/// Amounts are finite plain-unit deltas. Only the effective sum is clamped;
/// neither the base trajectory nor the held amount is changed by composition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlobalFloatModulation {
    minimum: f64,
    maximum: f64,
}

impl GlobalFloatModulation {
    pub(crate) const fn new(minimum: f64, maximum: f64) -> Self {
        Self { minimum, maximum }
    }

    /// Compose already-constructed plain-base and held-modulation cursors.
    ///
    /// Construct base from the consumed store so normalized interpolation is
    /// mapped before addition. Explicit low-level callers own cursor/schema
    /// agreement; typed float handles couple those inputs automatically.
    #[must_use]
    pub fn cursor<'a>(
        self,
        base: FloatParameterCursor<'a>,
        amount: FloatParameterCursor<'a>,
    ) -> ModulatedFloatCursor<'a> {
        ModulatedFloatCursor {
            base,
            amount: Some(amount),
            domain: self,
        }
    }

    pub(crate) fn base_cursor(self, base: FloatParameterCursor<'_>) -> ModulatedFloatCursor<'_> {
        ModulatedFloatCursor {
            base,
            amount: None,
            domain: self,
        }
    }
}

/// Lazy effective-value cursor over independent base and modulation controls.
#[derive(Clone)]
pub struct ModulatedFloatCursor<'events> {
    base: FloatParameterCursor<'events>,
    amount: Option<FloatParameterCursor<'events>>,
    domain: GlobalFloatModulation,
}

impl ModulatedFloatCursor<'_> {
    /// Evaluate `clamp(base + amount)` at a nondecreasing sample offset.
    ///
    /// Finite operands that overflow on addition saturate to a finite endpoint.
    /// Explicit base ramps are preserved without smoothing modulation steps.
    ///
    /// # Errors
    ///
    /// Returns [`ParameterCursorError`] for invalid queries or non-float base events.
    pub fn value_at(&mut self, offset: u32) -> Result<f64, ParameterCursorError> {
        let base = self.base.value_at(offset)?;
        let amount = match &mut self.amount {
            Some(cursor) => cursor.value_at(offset)?,
            None => 0.0,
        };
        Ok((base + amount).clamp(self.domain.minimum, self.domain.maximum))
    }
}
