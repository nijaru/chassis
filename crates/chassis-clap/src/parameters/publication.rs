use std::{
    fmt,
    hint::spin_loop,
    sync::atomic::{AtomicU8, AtomicU64, Ordering},
    vec::Vec,
};

const SNAPSHOT_RETRIES: usize = 8;
const WRITER_COUNT: u64 = 2;
// A source owns its gate before reserving a ticket. At most two reservations
// can pass the fault check concurrently, including rejected reservations. Leave
// that much headroom so fetch_add cannot wrap before the fault becomes visible.
const LAST_REVISION: u64 = u64::MAX - 2 * WRITER_COUNT - 1;
// Sticky flags: interruption must forbid snapshots even after exhaustion.
const EXHAUSTED: u8 = 1;
const INTERRUPTED: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PublicationError {
    Busy,
    GenerationExhausted,
    Interrupted,
    InvalidValueCount,
}

impl fmt::Display for PublicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Busy => "parameter publication source is already in use",
            Self::GenerationExhausted => "parameter publication revision is exhausted",
            Self::Interrupted => "parameter publication was interrupted",
            Self::InvalidValueCount => "parameter publication has the wrong value count",
        })
    }
}

impl std::error::Error for PublicationError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ParameterGeneration {
    control: u64,
    ingress: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Snapshot {
    pub generation: ParameterGeneration,
    pub custom_generation: u64,
}

struct IngressValue {
    value: AtomicU64,
    revision: AtomicU64,
}

// One desired-state view, with independently serialized publication sources.
// A full control replacement supplies the base and a cutoff revision. Ingress
// registers retain newer untimed host sets or completed DSP values. Readers
// validate BOTH source sequences; they never drain or clear these registers.
// Custom ownership stays behind the projection's non-RT mutex.
pub(super) struct StatePublication {
    values: Vec<AtomicU64>,
    replacement_revision: AtomicU64,
    custom_generation: AtomicU64,
    control_sequence: AtomicU64,
    ingress: Vec<IngressValue>,
    ingress_sequence: AtomicU64,
    revision: AtomicU64,
    fault: AtomicU8,
}

struct SourceWrite<'a> {
    sequence: &'a AtomicU64,
    fault: &'a AtomicU8,
    previous: u64,
    dirty: bool,
    finished: bool,
}

impl SourceWrite<'_> {
    fn finish(mut self) -> u64 {
        let completed = self.previous + 2;
        self.sequence.store(completed, Ordering::Release);
        self.finished = true;
        completed
    }
}

impl Drop for SourceWrite<'_> {
    fn drop(&mut self) {
        if !self.finished {
            if self.dirty {
                // Containment cannot make a partially written operation safe to
                // continue. Release readers, but permanently disable processing.
                // Interruption dominates exhaustion: a concurrent failed
                // reservation must not make partial payload saveable.
                self.fault.fetch_or(INTERRUPTED, Ordering::AcqRel);
            }
            self.sequence.store(
                self.previous + if self.dirty { 2 } else { 0 },
                Ordering::Release,
            );
        }
    }
}

impl StatePublication {
    pub(super) fn new(initial_values: &[f64]) -> Self {
        Self {
            values: initial_values
                .iter()
                .map(|value| AtomicU64::new(value.to_bits()))
                .collect(),
            replacement_revision: AtomicU64::new(0),
            custom_generation: AtomicU64::new(0),
            control_sequence: AtomicU64::new(0),
            ingress: initial_values
                .iter()
                .map(|value| IngressValue {
                    value: AtomicU64::new(value.to_bits()),
                    revision: AtomicU64::new(0),
                })
                .collect(),
            ingress_sequence: AtomicU64::new(0),
            revision: AtomicU64::new(0),
            fault: AtomicU8::new(0),
        }
    }

    pub(super) fn check_healthy(&self) -> Result<(), PublicationError> {
        match self.fault.load(Ordering::Acquire) {
            0 => Ok(()),
            EXHAUSTED => Err(PublicationError::GenerationExhausted),
            _ => Err(PublicationError::Interrupted),
        }
    }

    pub(super) fn completed_generation(&self) -> Option<ParameterGeneration> {
        let control = self.control_sequence.load(Ordering::Acquire);
        let ingress = self.ingress_sequence.load(Ordering::Acquire);
        (control.is_multiple_of(2) && ingress.is_multiple_of(2))
            .then_some(ParameterGeneration { control, ingress })
    }

    pub(super) fn len(&self) -> usize {
        self.values.len()
    }

    // CLAP get_value is main-thread only. No source gate spans foreign calls,
    // so a reentrant query cannot wait for its own suspended publication.
    pub(super) fn value_control(&self, index: usize) -> f64 {
        loop {
            let ingress = self.ingress_sequence.load(Ordering::Acquire);
            let control = self.control_sequence.load(Ordering::Acquire);
            if ingress.is_multiple_of(2) && control.is_multiple_of(2) {
                let cutoff = self.replacement_revision.load(Ordering::Acquire);
                let value = self.read_value(index, cutoff);
                if self.control_sequence.load(Ordering::Acquire) == control
                    && self.ingress_sequence.load(Ordering::Acquire) == ingress
                {
                    return value;
                }
            }
            spin_loop();
        }
    }

    fn read_value(&self, index: usize, cutoff: u64) -> f64 {
        let base = self.values[index].load(Ordering::Acquire);
        let pending = self.ingress[index].value.load(Ordering::Acquire);
        let revision = self.ingress[index].revision.load(Ordering::Acquire);
        f64::from_bits(if revision > cutoff { pending } else { base })
    }

    fn begin_source<'a>(
        &'a self,
        sequence: &'a AtomicU64,
        expected: u64,
    ) -> Result<SourceWrite<'a>, PublicationError> {
        self.check_healthy()?;
        if !expected.is_multiple_of(2) {
            return Err(PublicationError::Busy);
        }
        sequence
            .compare_exchange(expected, expected + 1, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| PublicationError::Busy)?;
        Ok(SourceWrite {
            sequence,
            fault: &self.fault,
            previous: expected,
            dirty: false,
            finished: false,
        })
    }

    fn reserve_revision(&self) -> Result<u64, PublicationError> {
        self.check_healthy()?;
        let previous = self.revision.fetch_add(2, Ordering::AcqRel);
        if previous >= LAST_REVISION {
            self.fault.fetch_or(EXHAUSTED, Ordering::AcqRel);
            return Err(PublicationError::GenerationExhausted);
        }
        Ok(previous + 2)
    }

    fn begin_control(&self) -> Result<(SourceWrite<'_>, u64), PublicationError> {
        loop {
            let expected = self.control_sequence.load(Ordering::Acquire);
            match self.begin_source(&self.control_sequence, expected) {
                Ok(write) => return Ok((write, self.reserve_revision()?)),
                Err(PublicationError::Busy) => spin_loop(),
                Err(error) => return Err(error),
            }
        }
    }

    fn write_ingress(&self, index: usize, value: f64, revision: u64) {
        self.ingress[index]
            .value
            .store(value.to_bits(), Ordering::Release);
        self.ingress[index]
            .revision
            .store(revision, Ordering::Release);
    }

    pub(super) fn try_publish_value(
        &self,
        index: usize,
        value: f64,
    ) -> Result<(), PublicationError> {
        let expected = self.ingress_sequence.load(Ordering::Acquire);
        let mut write = self.begin_source(&self.ingress_sequence, expected)?;
        let revision = self.reserve_revision()?;
        write.dirty = true;
        self.write_ingress(index, value, revision);
        write.finish();
        Ok(())
    }

    pub(super) fn publish_values_control(
        &self,
        values: &[f64],
        custom_changed: bool,
    ) -> Result<(), PublicationError> {
        if values.len() != self.values.len() {
            return Err(PublicationError::InvalidValueCount);
        }
        let (mut write, revision) = self.begin_control()?;
        write.dirty = true;
        for (slot, value) in self.values.iter().zip(values) {
            slot.store(value.to_bits(), Ordering::Release);
        }
        self.replacement_revision.store(revision, Ordering::Release);
        if custom_changed {
            // Reversion must not make an old activation current again.
            self.custom_generation.store(revision, Ordering::Release);
        }
        write.finish();
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn publish_value_control(
        &self,
        index: usize,
        value: f64,
    ) -> Result<(), PublicationError> {
        let mut values = vec![0.0; self.len()];
        let (mut write, revision) = self.begin_control()?;
        // Reserve BEFORE capturing ingress: earlier sets must be preserved;
        // later sets retain higher tickets and win regardless of this capture.
        loop {
            let start = self.ingress_sequence.load(Ordering::Acquire);
            if start.is_multiple_of(2) {
                let cutoff = self.replacement_revision.load(Ordering::Acquire);
                for (index, target) in values.iter_mut().enumerate() {
                    *target = self.read_value(index, cutoff);
                }
                if start == self.ingress_sequence.load(Ordering::Acquire) {
                    break;
                }
            }
            spin_loop();
        }
        values[index] = value;
        write.dirty = true;
        for (slot, value) in self.values.iter().zip(values) {
            slot.store(value.to_bits(), Ordering::Release);
        }
        self.replacement_revision.store(revision, Ordering::Release);
        write.finish();
        Ok(())
    }

    pub(super) fn try_publish_values_from(
        &self,
        expected: ParameterGeneration,
        values: &[f64],
    ) -> Result<Option<ParameterGeneration>, PublicationError> {
        if values.len() != self.values.len() {
            return Err(PublicationError::InvalidValueCount);
        }
        let mut write = match self.begin_source(&self.ingress_sequence, expected.ingress) {
            Ok(write) => write,
            Err(PublicationError::Busy) => return Ok(None),
            Err(error) => return Err(error),
        };
        if self.control_sequence.load(Ordering::Acquire) != expected.control {
            return Ok(None);
        }
        let revision = self.reserve_revision()?;
        // A load can begin between the first check and reservation. Never give
        // old DSP values a ticket newer than that replacement. A load starting
        // after this reservation gets a higher ticket and supersedes us instead.
        if self.control_sequence.load(Ordering::Acquire) != expected.control {
            return Ok(None);
        }
        write.dirty = true;
        for (index, value) in values.iter().copied().enumerate() {
            self.write_ingress(index, value, revision);
        }
        let ingress = write.finish();
        Ok(
            (self.control_sequence.load(Ordering::Acquire) == expected.control).then_some(
                ParameterGeneration {
                    control: expected.control,
                    ingress,
                },
            ),
        )
    }

    pub(super) fn try_snapshot_into(&self, output: &mut [f64]) -> Option<Snapshot> {
        if output.len() != self.values.len() {
            return None;
        }
        for _ in 0..SNAPSHOT_RETRIES {
            let ingress = self.ingress_sequence.load(Ordering::Acquire);
            let control = self.control_sequence.load(Ordering::Acquire);
            if !ingress.is_multiple_of(2) || !control.is_multiple_of(2) {
                spin_loop();
                continue;
            }
            let cutoff = self.replacement_revision.load(Ordering::Acquire);
            for (index, target) in output.iter_mut().enumerate() {
                *target = self.read_value(index, cutoff);
            }
            let custom_generation = self.custom_generation.load(Ordering::Acquire);
            if self.control_sequence.load(Ordering::Acquire) == control
                && self.ingress_sequence.load(Ordering::Acquire) == ingress
                && self.fault.load(Ordering::Acquire) & INTERRUPTED == 0
            {
                return Some(Snapshot {
                    generation: ParameterGeneration { control, ingress },
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
            if self.fault.load(Ordering::Acquire) & INTERRUPTED != 0 {
                return Err(PublicationError::Interrupted);
            }
            if let Some(generation) = self.try_snapshot_into(output) {
                return Ok(generation);
            }
            spin_loop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(publication: &StatePublication) -> (Snapshot, Vec<f64>) {
        let mut values = vec![0.0; publication.len()];
        (
            publication.snapshot_control_into(&mut values).unwrap(),
            values,
        )
    }

    #[test]
    fn flush_value_survives_an_in_progress_control_publication() {
        use crate::parameters::ClapStateProjection;
        use chassis_runtime::{
            parameters::{ParameterDescriptor, ParameterStore, ParameterValue},
            schema::{ComponentId, StateIdentity, StateSchemaVersion},
            state::{StateDocument, StateEntry, StateLimits, StateValue},
        };
        use clack_plugin::events::{Pckn, event_types::ParamValueEvent, io::InputEvents};
        use clack_plugin::utils::ClapId;

        let descriptors = [
            ParameterDescriptor::float("gain", "Gain", 0.0, 1.0, 0.5).unwrap(),
            ParameterDescriptor::float("other", "Other", 0.0, 4.0, 0.0).unwrap(),
        ];
        let state = ClapStateProjection::new(&descriptors, &[("gain", 1), ("other", 2)]).unwrap();
        let publication = &state.publication;
        let (mut control, revision) = publication.begin_control().unwrap();
        let mut scratch = [0.0; 2];
        assert!(publication.try_snapshot_into(&mut scratch).is_none());
        let events = [
            ParamValueEvent::new(0, ClapId::new(1), Pckn::match_all(), 0.75),
            ParamValueEvent::new(0, ClapId::new(1), Pckn::match_all(), 0.25),
        ];
        state
            .apply_input(&InputEvents::from_buffer(&events))
            .unwrap();
        control.dirty = true;
        publication.values[0].store(1.0_f64.to_bits(), Ordering::Release);
        publication.values[1].store(4.0_f64.to_bits(), Ordering::Release);
        publication
            .replacement_revision
            .store(revision, Ordering::Release);
        control.finish();

        let (_, values) = snapshot(publication);
        assert_eq!(bits(&values), [0.25_f64.to_bits(), 4.0_f64.to_bits()]);
        assert_eq!(state.value_control(0).to_bits(), 0.25_f64.to_bits());
        let mut store = ParameterStore::new(&descriptors).unwrap();
        state
            .sync_into(store.values_mut(), &mut scratch, None, 0)
            .unwrap();
        assert_eq!(store.get("gain"), Some(&ParameterValue::Float(0.25)));
        assert_eq!(store.get("other"), Some(&ParameterValue::Float(4.0)));
        let identity = StateIdentity::new(
            ComponentId::new("test.ingress").unwrap(),
            StateSchemaVersion::new(1),
        );
        let mut expected = StateDocument::new("test.ingress", 1).unwrap();
        expected
            .insert(StateEntry::parameter("gain", StateValue::Float(0.25)))
            .unwrap();
        expected
            .insert(StateEntry::parameter("other", StateValue::Float(4.0)))
            .unwrap();
        let saved = state
            .encode_state(&identity, StateLimits::default())
            .unwrap();
        assert_eq!(StateDocument::decode(&saved).unwrap(), expected);
    }

    fn bits(values: &[f64]) -> Vec<u64> {
        values.iter().map(|value| value.to_bits()).collect()
    }

    #[test]
    fn replacement_supersedes_old_ingress_without_resurrection() {
        let publication = StatePublication::new(&[0.5, 0.0]);
        publication.try_publish_value(0, 0.75).unwrap();
        publication
            .publish_values_control(&[0.25, 4.0], true)
            .unwrap();
        let (loaded, values) = snapshot(&publication);
        assert_eq!(bits(&values), [0.25_f64.to_bits(), 4.0_f64.to_bits()]);
        publication.try_publish_value(1, 3.0).unwrap();
        let (edited, values) = snapshot(&publication);
        assert_eq!(bits(&values), [0.25_f64.to_bits(), 3.0_f64.to_bits()]);
        assert_eq!(edited.custom_generation, loaded.custom_generation);
    }

    #[test]
    fn unrelated_control_edit_preserves_accepted_ingress() {
        let publication = StatePublication::new(&[0.5, 0.0]);
        publication.try_publish_value(0, 0.75).unwrap();
        publication.publish_value_control(1, 4.0).unwrap();
        assert_eq!(
            bits(&snapshot(&publication).1),
            [0.75_f64.to_bits(), 4.0_f64.to_bits()]
        );
    }

    #[test]
    fn stale_completion_cannot_overwrite_either_source() {
        let publication = StatePublication::new(&[0.5, 0.0]);
        let baseline = snapshot(&publication).0.generation;
        publication.try_publish_value(0, 0.75).unwrap();
        assert_eq!(
            publication.try_publish_values_from(baseline, &[0.25, 2.0]),
            Ok(None)
        );
        let baseline = snapshot(&publication).0.generation;
        publication
            .publish_values_control(&[0.5, 4.0], false)
            .unwrap();
        assert_eq!(
            publication.try_publish_values_from(baseline, &[0.25, 2.0]),
            Ok(None)
        );
        assert_eq!(
            bits(&snapshot(&publication).1),
            [0.5_f64.to_bits(), 4.0_f64.to_bits()]
        );
    }

    #[test]
    fn completion_establishes_combined_cache_token_and_saveable_values() {
        let publication = StatePublication::new(&[0.5, 0.0]);
        let baseline = snapshot(&publication).0.generation;
        let completed = publication
            .try_publish_values_from(baseline, &[0.25, 4.0])
            .unwrap()
            .unwrap();
        let (saved, values) = snapshot(&publication);
        assert_eq!(saved.generation, completed);
        assert_ne!(completed, baseline);
        assert_eq!(saved.custom_generation, 0);
        assert_eq!(bits(&values), [0.25_f64.to_bits(), 4.0_f64.to_bits()]);
    }

    #[test]
    fn readers_are_bounded_and_other_ingress_writers_are_explicitly_busy() {
        let publication = StatePublication::new(&[0.5]);
        let expected = publication.ingress_sequence.load(Ordering::Acquire);
        let writer = publication
            .begin_source(&publication.ingress_sequence, expected)
            .unwrap();
        assert_eq!(
            publication.try_publish_value(0, 0.25),
            Err(PublicationError::Busy)
        );
        assert!(publication.try_snapshot_into(&mut [0.0]).is_none());
        drop(writer);
        assert_eq!(
            publication.completed_generation(),
            Some(ParameterGeneration {
                control: 0,
                ingress: 0
            })
        );
        assert_eq!(publication.check_healthy(), Ok(()));
    }

    #[test]
    fn interrupted_dirty_writer_is_terminal_instead_of_certifying_old_data() {
        let publication = StatePublication::new(&[0.5]);
        let mut writer = publication
            .begin_source(&publication.ingress_sequence, 0)
            .unwrap();
        let revision = publication.reserve_revision().unwrap();
        writer.dirty = true;
        publication.write_ingress(0, 0.75, revision);
        drop(writer);
        assert_eq!(
            publication.check_healthy(),
            Err(PublicationError::Interrupted)
        );
        assert_eq!(
            publication.try_publish_value(0, 0.25),
            Err(PublicationError::Interrupted)
        );
        assert!(publication.try_snapshot_into(&mut [0.0]).is_none());
        assert_eq!(
            publication.snapshot_control_into(&mut [0.0]),
            Err(PublicationError::Interrupted)
        );
    }

    #[test]
    fn interruption_overrides_concurrent_exhaustion_and_rejects_partial_save() {
        let publication = StatePublication::new(&[0.5, 0.0]);
        publication
            .revision
            .store(LAST_REVISION - 2, Ordering::Release);
        let mut writer = publication
            .begin_source(&publication.ingress_sequence, 0)
            .unwrap();
        let revision = publication.reserve_revision().unwrap();
        writer.dirty = true;
        publication.write_ingress(0, 0.75, revision);
        assert_eq!(
            publication.publish_values_control(&[1.0, 1.0], false),
            Err(PublicationError::GenerationExhausted)
        );
        drop(writer);
        assert_eq!(
            publication.snapshot_control_into(&mut [0.0; 2]),
            Err(PublicationError::Interrupted)
        );
        assert_eq!(
            publication.check_healthy(),
            Err(PublicationError::Interrupted)
        );
    }

    #[test]
    fn revision_exhaustion_is_terminal_but_final_values_remain_readable() {
        let publication = StatePublication::new(&[0.5]);
        publication
            .revision
            .store(LAST_REVISION - 2, Ordering::Release);
        publication.try_publish_value(0, 0.75).unwrap();
        let last = snapshot(&publication).0;
        assert_eq!(
            publication.try_publish_value(0, 0.25),
            Err(PublicationError::GenerationExhausted)
        );
        assert_eq!(
            publication.publish_values_control(&[0.25], false),
            Err(PublicationError::GenerationExhausted)
        );
        assert_eq!(snapshot(&publication).0, last);
        assert_eq!(publication.value_control(0).to_bits(), 0.75_f64.to_bits());
    }

    #[test]
    fn both_sources_have_headroom_to_reject_exhausted_reservations() {
        let publication = StatePublication::new(&[0.5]);
        let control = publication
            .begin_source(&publication.control_sequence, 0)
            .unwrap();
        let ingress = publication
            .begin_source(&publication.ingress_sequence, 0)
            .unwrap();
        publication.revision.store(LAST_REVISION, Ordering::Release);
        // Simulate both writers having passed check_healthy before either sets
        // the terminal fault: these are the only possible outstanding tickets.
        for _ in 0..WRITER_COUNT {
            let previous = publication.revision.fetch_add(2, Ordering::AcqRel);
            assert!(previous >= LAST_REVISION);
            assert!(previous.checked_add(2).is_some());
        }
        publication.fault.store(EXHAUSTED, Ordering::Release);
        drop((control, ingress));
        let final_revision = publication.revision.load(Ordering::Acquire);
        assert_eq!(
            publication.try_publish_value(0, 0.75),
            Err(PublicationError::GenerationExhausted)
        );
        assert_eq!(publication.revision.load(Ordering::Acquire), final_revision);
    }

    #[test]
    fn exhausted_complete_publication_retains_custom_payload_and_epoch() {
        use crate::parameters::ClapStateProjection;
        use chassis_runtime::{
            parameters::ParameterDescriptor,
            schema::{ComponentId, StateIdentity, StateSchemaVersion},
            state::{StateDocument, StateEntry, StateValue},
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
            .revision
            .store(LAST_REVISION, Ordering::Release);
        let before = state.state_document(&identity).unwrap();
        let mut replacement = StateDocument::new("test.exhaustion", 1).unwrap();
        replacement
            .insert(StateEntry::new("parameter/gain", StateValue::Float(0.75)))
            .unwrap();
        replacement
            .insert(StateEntry::new("state/bytes", StateValue::Bytes(vec![9])))
            .unwrap();
        assert!(state.apply_state(&replacement, &identity).is_err());
        assert_eq!(state.state_document(&identity).unwrap(), before);
    }

    #[test]
    fn terminal_completion_reports_failure_and_retains_the_final_snapshot() {
        use crate::parameters::{ClapStateProjection, ParameterSyncError};
        use chassis_runtime::parameters::{ParameterDescriptor, ParameterStore, ParameterValue};
        let descriptors = [ParameterDescriptor::float("gain", "Gain", 0.0, 1.0, 0.5).unwrap()];
        let state = ClapStateProjection::new(&descriptors, &[("gain", 1)]).unwrap();
        let mut store = ParameterStore::new(&descriptors).unwrap();
        let mut scratch = [0.0];
        let baseline = state
            .sync_into(store.values_mut(), &mut scratch, None, 0)
            .unwrap();
        state
            .publication
            .revision
            .store(LAST_REVISION, Ordering::Release);
        store.set("gain", ParameterValue::Float(0.75)).unwrap();
        assert_eq!(
            state.publish_parameters(&store, baseline, &mut scratch),
            Err(ParameterSyncError::Publication(
                PublicationError::GenerationExhausted
            ))
        );
        assert_eq!(
            state.sync_into(store.values_mut(), &mut scratch, None, 0),
            Err(ParameterSyncError::Publication(
                PublicationError::GenerationExhausted
            ))
        );
        assert_eq!(state.value_control(0).to_bits(), 0.5_f64.to_bits());
    }

    #[test]
    fn control_operations_reject_wrong_value_counts() {
        let publication = StatePublication::new(&[0.5, 0.0]);
        assert_eq!(
            publication.publish_values_control(&[0.25], false),
            Err(PublicationError::InvalidValueCount)
        );
        assert_eq!(
            publication.snapshot_control_into(&mut [0.0]),
            Err(PublicationError::InvalidValueCount)
        );
    }
}
