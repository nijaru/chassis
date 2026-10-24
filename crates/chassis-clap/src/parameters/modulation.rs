//! Instance-lifetime external input, deliberately outside state publication.

use std::sync::atomic::{AtomicU64, Ordering};

use chassis_runtime::{
    automation::{ParameterEvent, ParameterEventChange},
    parameters::{ParameterEventValueError, ParameterIndex},
    runtime::{InstanceRuntime, Processor},
};

use super::ClapParameterBinding;

pub(crate) struct ModulationProjection {
    amounts: Box<[AtomicU64]>,
    indices: Box<[ParameterIndex]>,
}

impl ModulationProjection {
    pub(super) fn new(bindings: &[ClapParameterBinding]) -> Self {
        Self {
            amounts: bindings
                .iter()
                .map(|_| AtomicU64::new(0.0_f64.to_bits()))
                .collect(),
            indices: bindings
                .iter()
                .filter(|binding| binding.descriptor().global_modulation().is_some())
                .map(ClapParameterBinding::index)
                .collect(),
        }
    }

    // CLAP serializes process, flush, and activation, including transfers between
    // physical audio threads. State save/load never access these scalars. There
    // is no payload or cross-field transaction to publish: relaxed is sufficient.
    pub(super) fn set(&self, parameter: ParameterIndex, amount: f64) {
        let index = usize::try_from(parameter.get()).expect("validated dense index fits usize");
        self.amounts[index].store(amount.to_bits(), Ordering::Relaxed);
    }

    pub(crate) fn sync_into<P: Processor>(
        &self,
        runtime: &mut InstanceRuntime<P>,
    ) -> Result<(), ParameterEventValueError> {
        for &parameter in &self.indices {
            let index = usize::try_from(parameter.get()).expect("validated dense index fits usize");
            let amount = f64::from_bits(self.amounts[index].load(Ordering::Relaxed));
            runtime.set_modulation(parameter, amount)?;
        }
        Ok(())
    }

    pub(crate) fn retain_endpoints(&self, events: &[ParameterEvent]) {
        for event in events {
            if let ParameterEventChange::Modulate { amount } = event.change() {
                self.set(event.parameter(), amount);
            }
        }
    }
}
