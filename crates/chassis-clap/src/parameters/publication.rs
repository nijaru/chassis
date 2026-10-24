use std::{
    hint::spin_loop,
    sync::atomic::{AtomicU64, Ordering},
    vec::Vec,
};

const SNAPSHOT_RETRIES: usize = 8;
const LAST_STABLE_GENERATION: u64 = u64::MAX - 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PublicationError {
    GenerationExhausted,
    InvalidValueCount,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Snapshot {
    pub generation: u64,
    pub custom_generation: u64,
}

// Custom payloads stay behind a control-only mutex. The fixed-width epoch joins
// the scalar transaction without adopting or reclaiming objects on RT.
pub(super) struct StatePublication {
    values: Vec<AtomicU64>,
    generation: AtomicU64,
    custom_generation: AtomicU64,
}

impl StatePublication {
    pub(super) fn new(initial_values: &[f64]) -> Self {
        Self {
            values: initial_values
                .iter()
                .map(|value| AtomicU64::new(value.to_bits()))
                .collect(),
            generation: AtomicU64::new(0),
            custom_generation: AtomicU64::new(0),
        }
    }

    pub(super) fn completed_generation(&self) -> Option<u64> {
        let generation = self.generation.load(Ordering::Acquire);
        generation.is_multiple_of(2).then_some(generation)
    }

    pub(super) fn len(&self) -> usize {
        self.values.len()
    }

    pub(super) fn value(&self, index: usize) -> f64 {
        f64::from_bits(self.values[index].load(Ordering::Acquire))
    }

    fn try_begin_write(&self, expected: u64) -> Option<u64> {
        if !expected.is_multiple_of(2) || expected >= LAST_STABLE_GENERATION {
            return None;
        }
        let completed = expected + 2;
        self.generation
            .compare_exchange(expected, expected + 1, Ordering::AcqRel, Ordering::Acquire)
            .ok()?;
        Some(completed)
    }

    fn finish_write(&self, completed: u64) {
        debug_assert!(completed.is_multiple_of(2));
        debug_assert!(completed <= LAST_STABLE_GENERATION);
        self.generation.store(completed, Ordering::Release);
    }

    #[cfg(test)]
    pub(super) fn publish_value_control(
        &self,
        index: usize,
        value: f64,
    ) -> Result<(), PublicationError> {
        loop {
            let expected = self.generation.load(Ordering::Acquire);
            if expected == LAST_STABLE_GENERATION {
                return Err(PublicationError::GenerationExhausted);
            }
            let Some(completed) = self.try_begin_write(expected) else {
                spin_loop();
                continue;
            };
            self.values[index].store(value.to_bits(), Ordering::Release);
            self.finish_write(completed);
            return Ok(());
        }
    }

    pub(super) fn try_publish_value(&self, index: usize, value: f64) -> bool {
        let expected = self.generation.load(Ordering::Acquire);
        let Some(completed) = self.try_begin_write(expected) else {
            return false;
        };
        self.values[index].store(value.to_bits(), Ordering::Release);
        self.finish_write(completed);
        true
    }

    pub(super) fn publish_values_control(
        &self,
        values: &[f64],
        custom_changed: bool,
    ) -> Result<(), PublicationError> {
        if values.len() != self.values.len() {
            return Err(PublicationError::InvalidValueCount);
        }
        loop {
            let expected = self.generation.load(Ordering::Acquire);
            if expected == LAST_STABLE_GENERATION {
                return Err(PublicationError::GenerationExhausted);
            }
            let Some(completed) = self.try_begin_write(expected) else {
                spin_loop();
                continue;
            };
            for (slot, value) in self.values.iter().zip(values) {
                slot.store(value.to_bits(), Ordering::Release);
            }
            if custom_changed {
                // Reversion must not make an old activation current again.
                self.custom_generation.store(completed, Ordering::Release);
            }
            self.finish_write(completed);
            return Ok(());
        }
    }

    pub(super) fn try_publish_values_from(&self, expected: u64, values: &[f64]) -> bool {
        if values.len() != self.values.len() {
            return false;
        }
        let Some(completed) = self.try_begin_write(expected) else {
            return false;
        };
        for (slot, value) in self.values.iter().zip(values) {
            slot.store(value.to_bits(), Ordering::Release);
        }
        self.finish_write(completed);
        true
    }

    pub(super) fn try_snapshot_into(&self, output: &mut [f64]) -> Option<Snapshot> {
        if output.len() != self.values.len() {
            return None;
        }
        for _ in 0..SNAPSHOT_RETRIES {
            let start = self.generation.load(Ordering::Acquire);
            if !start.is_multiple_of(2) {
                spin_loop();
                continue;
            }
            for (slot, value) in self.values.iter().zip(output.iter_mut()) {
                *value = f64::from_bits(slot.load(Ordering::Acquire));
            }
            let custom_generation = self.custom_generation.load(Ordering::Acquire);
            let end = self.generation.load(Ordering::Acquire);
            if start == end && end.is_multiple_of(2) {
                return Some(Snapshot {
                    generation: end,
                    custom_generation,
                });
            }
        }
        None
    }

    pub(super) fn snapshot_control_into(
        &self,
        output: &mut [f64],
    ) -> Result<Snapshot, PublicationError> {
        if output.len() != self.values.len() {
            return Err(PublicationError::InvalidValueCount);
        }
        loop {
            if let Some(generation) = self.try_snapshot_into(output) {
                return Ok(generation);
            }
            spin_loop();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Barrier,
        atomic::{AtomicBool, Ordering as AtomicOrdering},
    };

    use super::*;

    #[test]
    fn stale_generation_cannot_publish_after_control_write() {
        let publication = StatePublication::new(&[0.5, 0.0]);
        let mut snapshot = [0.0; 2];
        let generation = publication
            .try_snapshot_into(&mut snapshot)
            .expect("initial snapshot is coherent");

        publication
            .publish_values_control(&[0.5, 4.0], false)
            .expect("control publication succeeds");
        snapshot[0] = 0.25;

        assert!(!publication.try_publish_values_from(generation.generation, &snapshot));
        assert_eq!(publication.value(0).to_bits(), 0.5_f64.to_bits());
        assert_eq!(publication.value(1).to_bits(), 4.0_f64.to_bits());
    }

    #[test]
    fn realtime_attempts_are_bounded_while_a_writer_is_active() {
        let publication = StatePublication::new(&[0.5]);
        publication.generation.store(1, Ordering::Release);
        let mut snapshot = [0.0];

        assert!(!publication.try_publish_value(0, 0.25));
        assert!(publication.try_snapshot_into(&mut snapshot).is_none());
    }

    #[test]
    fn control_snapshot_after_realtime_publication_observes_completed_generation() {
        let publication = StatePublication::new(&[0.5, 0.0]);
        let mut realtime = [0.0; 2];
        let generation = publication
            .try_snapshot_into(&mut realtime)
            .expect("initial realtime snapshot is coherent");
        realtime.copy_from_slice(&[0.25, 4.0]);
        assert!(publication.try_publish_values_from(generation.generation, &realtime));

        let mut saved = [0.0; 2];
        let saved_generation = publication
            .snapshot_control_into(&mut saved)
            .expect("control save obtains a coherent completed generation");

        assert_eq!(saved_generation.generation, generation.generation + 2);
        assert_eq!(saved_generation.custom_generation, 0);
        assert_eq!(saved[0].to_bits(), 0.25_f64.to_bits());
        assert_eq!(saved[1].to_bits(), 4.0_f64.to_bits());
    }

    #[test]
    fn control_snapshot_waits_out_an_in_progress_multi_value_write() {
        let publication = Arc::new(StatePublication::new(&[0.0, 0.0]));
        let completed = publication
            .try_begin_write(0)
            .expect("test writer acquires generation zero");
        publication.values[0].store(1.0_f64.to_bits(), Ordering::Release);

        let reader_started = Arc::new(AtomicBool::new(false));
        let release_writer = Arc::new(Barrier::new(2));
        let reader = {
            let publication = Arc::clone(&publication);
            let reader_started = Arc::clone(&reader_started);
            let release_writer = Arc::clone(&release_writer);
            std::thread::spawn(move || {
                reader_started.store(true, AtomicOrdering::Release);
                release_writer.wait();
                let mut saved = [f64::NAN; 2];
                let generation = publication
                    .snapshot_control_into(&mut saved)
                    .expect("control snapshot succeeds after writer completion");
                (generation, saved)
            })
        };

        while !reader_started.load(AtomicOrdering::Acquire) {
            std::hint::spin_loop();
        }
        publication.values[1].store(1.0_f64.to_bits(), Ordering::Release);
        publication.finish_write(completed);
        release_writer.wait();

        let (generation, saved) = reader.join().expect("snapshot thread joins");
        assert_eq!(generation.generation, completed);
        assert_eq!(saved[0].to_bits(), 1.0_f64.to_bits());
        assert_eq!(saved[1].to_bits(), 1.0_f64.to_bits());
    }

    #[test]
    fn generation_exhaustion_is_terminal_instead_of_wrapping() {
        let publication = StatePublication::new(&[0.5]);
        publication
            .generation
            .store(LAST_STABLE_GENERATION - 2, Ordering::Release);

        assert!(publication.try_publish_value(0, 0.75));
        assert_eq!(
            publication.generation.load(Ordering::Acquire),
            LAST_STABLE_GENERATION
        );
        assert!(!publication.try_publish_value(0, 0.25));
        assert_eq!(
            publication.publish_values_control(&[0.25], false),
            Err(PublicationError::GenerationExhausted)
        );

        let mut snapshot = [0.0];
        assert_eq!(
            publication.try_snapshot_into(&mut snapshot),
            Some(Snapshot {
                generation: LAST_STABLE_GENERATION,
                custom_generation: 0
            })
        );
        assert_eq!(snapshot[0].to_bits(), 0.75_f64.to_bits());
    }

    #[test]
    fn rejected_terminal_completion_invalidates_the_store_cache() {
        use super::super::ClapStateProjection;
        use chassis_runtime::parameters::{ParameterDescriptor, ParameterStore, ParameterValue};

        let descriptors = [ParameterDescriptor::float("gain", "Gain", 0.0, 1.0, 0.5).unwrap()];
        let state = ClapStateProjection::new(&descriptors, &[("gain", 1)]).unwrap();
        state
            .publication
            .generation
            .store(LAST_STABLE_GENERATION, Ordering::Release);
        let mut store = ParameterStore::new(&descriptors).unwrap();
        let mut scratch = [0.0];
        let baseline = state
            .sync_into(store.values_mut(), &mut scratch, None, 0)
            .unwrap();
        assert_eq!(baseline, Some(LAST_STABLE_GENERATION));

        store.set("gain", ParameterValue::Float(0.75)).unwrap();
        let rejected = state
            .publish_parameters(&store, baseline, &mut scratch)
            .unwrap();
        assert!(rejected.is_none());
        let restored = state
            .sync_into(store.values_mut(), &mut scratch, rejected, 0)
            .unwrap();
        assert_eq!(restored, baseline);
        assert_eq!(store.get("gain"), Some(&ParameterValue::Float(0.5)));
    }

    #[test]
    fn exhausted_complete_publication_retains_custom_payload_and_epoch() {
        use crate::parameters::ClapStateProjection;
        use chassis_runtime::{
            parameters::ParameterDescriptor,
            schema::{ComponentId, StateIdentity, StateSchemaVersion},
            state::{StateEntry, StateValue},
        };
        let state = ClapStateProjection::new(
            &[ParameterDescriptor::float("gain", "Gain", 0.0, 1.0, 0.5).unwrap()],
            &[("gain", 1)],
        )
        .unwrap();
        let identity = StateIdentity::new(
            ComponentId::new("test.exhaustion").unwrap(),
            StateSchemaVersion::new(1),
        );
        let (mut document, _) = state.state_document(&identity).unwrap();
        document
            .insert(StateEntry::new("state/bytes", StateValue::Bytes(vec![7])))
            .unwrap();
        state.apply_state(&document, &identity).unwrap();
        state
            .publication
            .generation
            .store(LAST_STABLE_GENERATION, Ordering::Release);
        let before = state.state_document(&identity).unwrap();
        let mut document =
            chassis_runtime::state::StateDocument::new("test.exhaustion", 1).unwrap();
        document
            .insert(StateEntry::new("parameter/gain", StateValue::Float(0.75)))
            .unwrap();
        document
            .insert(StateEntry::new("state/bytes", StateValue::Bytes(vec![9])))
            .unwrap();
        assert!(state.apply_state(&document, &identity).is_err());
        assert_eq!(state.state_document(&identity).unwrap(), before);
    }

    #[test]
    fn control_operations_reject_wrong_value_counts() {
        let publication = StatePublication::new(&[0.5, 0.0]);
        let mut snapshot = [0.0];

        assert_eq!(
            publication.publish_values_control(&[0.25], false),
            Err(PublicationError::InvalidValueCount)
        );
        assert_eq!(
            publication.snapshot_control_into(&mut snapshot),
            Err(PublicationError::InvalidValueCount)
        );
    }
}
