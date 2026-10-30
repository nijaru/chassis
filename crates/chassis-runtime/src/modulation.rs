//! Explicit global additive float modulation, independent of durable base values.

use crate::automation::{FloatParameterCursor, ParameterCursorError, ParameterEvents};
use crate::parameters::ParameterIndex;

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

    /// Compose a validated base trajectory with a held modulation trajectory.
    ///
    /// Initial values come from the consumed [`ProcessBlock`](crate::process::ProcessBlock),
    /// not a separately sampled desired control view. This allocates nothing.
    ///
    /// # Errors
    ///
    /// Returns [`ParameterCursorError::NonFiniteInitial`] for non-finite initial values.
    pub fn cursor(
        self,
        events: ParameterEvents<'_>,
        parameter: ParameterIndex,
        initial_base: f64,
        initial_amount: f64,
    ) -> Result<ModulatedFloatCursor<'_>, ParameterCursorError> {
        Ok(ModulatedFloatCursor {
            base: events.float_cursor(parameter, initial_base)?,
            amount: Some(events.modulation_cursor(parameter, initial_amount)?),
            domain: self,
        })
    }

    pub(crate) fn base_cursor(
        self,
        events: ParameterEvents<'_>,
        parameter: ParameterIndex,
        initial: f64,
    ) -> Result<ModulatedFloatCursor<'_>, ParameterCursorError> {
        Ok(ModulatedFloatCursor {
            base: events.float_cursor(parameter, initial)?,
            amount: None,
            domain: self,
        })
    }
}

/// Lazy effective-value cursor over independent base and modulation controls.
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
