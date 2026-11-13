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

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ControlSnapshot {
    pub plain: f64,
    pub coordinate: Option<f64>,
}

impl ControlSnapshot {
    pub(crate) const fn plain(plain: f64) -> Self {
        Self {
            plain,
            coordinate: None,
        }
    }
}

// A NaN sentinel preserves every finite coordinate's bits, including -0.
const NO_COORDINATE: u64 = f64::NAN.to_bits();

struct ValueCell {
    value: AtomicU64,
    coordinate: AtomicU64,
    revision: AtomicU64,
}

impl ValueCell {
    fn new(value: f64) -> Self {
        Self {
            value: AtomicU64::new(value.to_bits()),
            coordinate: AtomicU64::new(NO_COORDINATE),
            revision: AtomicU64::new(0),
        }
    }

    fn read(&self) -> (ControlSnapshot, u64) {
        let plain = f64::from_bits(self.value.load(Ordering::Acquire));
        let coordinate = self.coordinate.load(Ordering::Acquire);
        let revision = self.revision.load(Ordering::Acquire);
        (
            ControlSnapshot {
                plain,
                coordinate: (coordinate != NO_COORDINATE).then(|| f64::from_bits(coordinate)),
            },
            revision,
        )
    }

    fn write(&self, value: ControlSnapshot, revision: u64) {
        self.value.store(value.plain.to_bits(), Ordering::Release);
        self.coordinate.store(
            value.coordinate.map_or(NO_COORDINATE, f64::to_bits),
            Ordering::Release,
        );
        self.revision.store(revision, Ordering::Release);
    }
}

// One desired-state view, with independently serialized publication sources.
// Each root cell has its own cutoff; only full loads advance the global
// replacement revision used by queued notifications. Ingress
// registers retain newer untimed host sets or completed DSP values. Readers
// validate BOTH source sequences; they never drain or clear these registers.
// Custom ownership stays behind the projection's non-RT mutex.
pub(super) struct StatePublication {
    values: Vec<ValueCell>,
    replacement_revision: AtomicU64,
    custom_generation: AtomicU64,
    control_sequence: AtomicU64,
    ingress: Vec<ValueCell>,
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
                .map(|value| ValueCell::new(*value))
                .collect(),
            replacement_revision: AtomicU64::new(0),
            custom_generation: AtomicU64::new(0),
            control_sequence: AtomicU64::new(0),
            ingress: initial_values
                .iter()
                .map(|value| ValueCell::new(*value))
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
    pub(super) fn value_control(&self, index: usize) -> Result<f64, PublicationError> {
        self.control_snapshot(index).map(|snapshot| snapshot.plain)
    }

    pub(super) fn control_snapshot(
        &self,
        index: usize,
    ) -> Result<ControlSnapshot, PublicationError> {
        loop {
            if self.fault.load(Ordering::Acquire) & INTERRUPTED != 0 {
                return Err(PublicationError::Interrupted);
            }
            let mut value = ControlSnapshot::plain(0.0);
            if self
                .try_snapshot(1, |_, target| value = target, |_| index)
                .is_some()
            {
                return Ok(value);
            }
            spin_loop();
        }
    }

    fn read_value(&self, index: usize) -> ControlSnapshot {
        // Read every candidate field regardless of selection, then validate both
        // source epochs. No gate, mapper, or foreign call belongs here.
        let (base, cutoff) = self.values[index].read();
        let (pending, revision) = self.ingress[index].read();
        if revision > cutoff { pending } else { base }
    }

    pub(super) fn notification_superseded(&self, index: usize, revision: u64) -> bool {
        self.fault.load(Ordering::Acquire) & INTERRUPTED != 0
            || self.replacement_revision.load(Ordering::Acquire) > revision
            || self.ingress[index].revision.load(Ordering::Acquire) > revision
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

    fn write_ingress(&self, index: usize, value: ControlSnapshot, revision: u64) {
        self.ingress[index].write(value, revision);
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
        self.write_ingress(index, ControlSnapshot::plain(value), revision);
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
            slot.write(ControlSnapshot::plain(*value), revision);
        }
        self.replacement_revision.store(revision, Ordering::Release);
        if custom_changed {
            // Reversion must not make an old activation current again.
            self.custom_generation.store(revision, Ordering::Release);
        }
        write.finish();
        Ok(())
    }

    pub(super) fn publish_control_value(
        &self,
        index: usize,
        value: ControlSnapshot,
    ) -> Result<u64, PublicationError> {
        let (mut write, revision) = self.begin_control()?;
        write.dirty = true;
        self.values[index].write(value, revision);
        write.finish();
        Ok(revision)
    }

    pub(super) fn try_publish_values_from(
        &self,
        expected: ParameterGeneration,
        values: &[ControlSnapshot],
        changed: &[bool],
    ) -> Result<Option<ParameterGeneration>, PublicationError> {
        if values.len() != self.values.len() || changed.len() != self.values.len() {
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
            if changed[index] {
                self.write_ingress(index, value, revision);
            }
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
        if output.len() != self.len() {
            return None;
        }
        self.try_snapshot(
            output.len(),
            |index, value| output[index] = value.plain,
            |index| index,
        )
    }

    pub(super) fn try_snapshot_controls_into(
        &self,
        output: &mut [ControlSnapshot],
    ) -> Option<Snapshot> {
        if output.len() != self.len() {
            return None;
        }
        self.try_snapshot(
            output.len(),
            |index, value| output[index] = value,
            |index| index,
        )
    }

    fn try_snapshot(
        &self,
        count: usize,
        mut assign: impl FnMut(usize, ControlSnapshot),
        index_for: impl Fn(usize) -> usize,
    ) -> Option<Snapshot> {
        for _ in 0..SNAPSHOT_RETRIES {
            let ingress = self.ingress_sequence.load(Ordering::Acquire);
            let control = self.control_sequence.load(Ordering::Acquire);
            if !ingress.is_multiple_of(2) || !control.is_multiple_of(2) {
                spin_loop();
                continue;
            }
            for index in 0..count {
                assign(index, self.read_value(index_for(index)));
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
        let mut scratch = [ControlSnapshot::plain(0.0); 2];
        assert!(
            publication
                .try_snapshot_controls_into(&mut scratch)
                .is_none()
        );
        let events = [
            ParamValueEvent::new(0, ClapId::new(1), Pckn::match_all(), 0.75),
            ParamValueEvent::new(0, ClapId::new(1), Pckn::match_all(), 0.25),
        ];
        state
            .apply_input(&InputEvents::from_buffer(&events))
            .unwrap();
        control.dirty = true;
        publication.values[0].write(ControlSnapshot::plain(1.0), revision);
        publication.values[1].write(ControlSnapshot::plain(4.0), revision);
        publication
            .replacement_revision
            .store(revision, Ordering::Release);
        control.finish();

        let (_, values) = snapshot(publication);
        assert_eq!(bits(&values), [0.25_f64.to_bits(), 4.0_f64.to_bits()]);
        assert_eq!(
            state.value_control(0).unwrap().to_bits(),
            0.25_f64.to_bits()
        );
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
        publication
            .publish_control_value(1, ControlSnapshot::plain(4.0))
            .unwrap();
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
            publication.try_publish_values_from(
                baseline,
                &[ControlSnapshot::plain(0.25), ControlSnapshot::plain(2.0)],
                &[true; 2]
            ),
            Ok(None)
        );
        let baseline = snapshot(&publication).0.generation;
        publication
            .publish_values_control(&[0.5, 4.0], false)
            .unwrap();
        assert_eq!(
            publication.try_publish_values_from(
                baseline,
                &[ControlSnapshot::plain(0.25), ControlSnapshot::plain(2.0)],
                &[true; 2]
            ),
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
            .try_publish_values_from(
                baseline,
                &[ControlSnapshot::plain(0.25), ControlSnapshot::plain(4.0)],
                &[true; 2],
            )
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
        publication.write_ingress(0, ControlSnapshot::plain(0.75), revision);
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
        publication.write_ingress(0, ControlSnapshot::plain(0.75), revision);
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
        assert_eq!(
            publication.value_control(0).unwrap().to_bits(),
            0.75_f64.to_bits()
        );
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
        let mut scratch = [ControlSnapshot::plain(0.0)];
        let baseline = state
            .sync_into(store.values_mut(), &mut scratch, None, 0)
            .unwrap();
        state
            .publication
            .revision
            .store(LAST_REVISION, Ordering::Release);
        store.set("gain", ParameterValue::Float(0.75)).unwrap();
        assert_eq!(
            state.publish_parameters(&store, baseline, &mut scratch, &[true]),
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
        assert_eq!(state.value_control(0).unwrap().to_bits(), 0.5_f64.to_bits());
    }

    #[test]
    fn scalar_root_does_not_erase_other_ingress_or_resurrect_preload_values() {
        let publication = StatePublication::new(&[0.5, 0.0]);
        publication.try_publish_value(0, 0.75).unwrap();
        publication
            .publish_values_control(&[0.25, 1.0], true)
            .unwrap();
        let epoch = snapshot(&publication).0.custom_generation;
        let (mut root, revision) = publication.begin_control().unwrap();
        publication.try_publish_value(0, 0.5).unwrap();
        root.dirty = true;
        publication.values[1].write(ControlSnapshot::plain(2.0), revision);
        root.finish();
        assert_eq!(bits(&snapshot(&publication).1), bits(&[0.5, 2.0]));
        publication
            .publish_control_value(1, ControlSnapshot::plain(3.0))
            .unwrap();
        assert_eq!(bits(&snapshot(&publication).1), bits(&[0.5, 3.0]));
        assert_eq!(snapshot(&publication).0.custom_generation, epoch);
        // Updating A itself now must use its own root cutoff, without reviving
        // A's pre-load ingress or erasing any later B point.
        publication.try_publish_value(1, 4.0).unwrap();
        publication
            .publish_control_value(0, ControlSnapshot::plain(0.0))
            .unwrap();
        assert_eq!(bits(&snapshot(&publication).1), bits(&[0.0, 4.0]));
    }

    #[test]
    fn notification_cuts_are_external_and_parameter_specific() {
        let publication = StatePublication::new(&[0.5, 0.0]);
        let first = publication
            .publish_control_value(0, ControlSnapshot::plain(0.25))
            .unwrap();
        let second = publication
            .publish_control_value(0, ControlSnapshot::plain(0.75))
            .unwrap();
        publication
            .publish_control_value(1, ControlSnapshot::plain(1.0))
            .unwrap();
        assert!(!publication.notification_superseded(0, first));
        assert!(!publication.notification_superseded(0, second));
        publication.try_publish_value(1, 2.0).unwrap();
        assert!(!publication.notification_superseded(0, first));
        assert!(publication.notification_superseded(1, first));
        publication.try_publish_value(0, 0.5).unwrap();
        assert!(publication.notification_superseded(0, first));
        assert!(publication.notification_superseded(0, second));
    }

    #[test]
    fn identical_full_load_clears_pairs_and_supersedes_every_older_notification() {
        let publication = StatePublication::new(&[0.5, 0.0]);
        let a = publication
            .publish_control_value(
                0,
                ControlSnapshot {
                    plain: 0.5,
                    coordinate: Some(-0.0),
                },
            )
            .unwrap();
        let b = publication
            .publish_control_value(
                1,
                ControlSnapshot {
                    plain: 0.0,
                    coordinate: Some(0.75),
                },
            )
            .unwrap();
        assert_eq!(
            publication
                .control_snapshot(0)
                .unwrap()
                .coordinate
                .unwrap()
                .to_bits(),
            (-0.0_f64).to_bits()
        );
        publication
            .publish_values_control(&[0.5, 0.0], true)
            .unwrap();
        let before = snapshot(&publication).0;
        publication
            .publish_values_control(&[0.5, 0.0], false)
            .unwrap();
        let after = snapshot(&publication).0;
        assert_ne!(before.generation, after.generation);
        assert_eq!(before.custom_generation, after.custom_generation);
        assert_eq!(
            publication.control_snapshot(0).unwrap(),
            ControlSnapshot::plain(0.5)
        );
        assert_eq!(
            publication.control_snapshot(1).unwrap(),
            ControlSnapshot::plain(0.0)
        );
        assert!(publication.notification_superseded(0, a));
        assert!(publication.notification_superseded(1, b));
    }

    #[test]
    fn partial_completion_preserves_untouched_ingress_revision_and_coordinate() {
        let publication = StatePublication::new(&[0.5, 0.0]);
        let pair = ControlSnapshot {
            plain: 0.5,
            coordinate: Some(0.75),
        };
        let first = publication.publish_control_value(0, pair).unwrap();
        let baseline = snapshot(&publication).0.generation;
        let completed = publication
            .try_publish_values_from(
                baseline,
                &[ControlSnapshot::plain(999.0), ControlSnapshot::plain(2.0)],
                &[false, true],
            )
            .unwrap()
            .unwrap();
        assert_eq!(publication.control_snapshot(0).unwrap(), pair);
        assert_eq!(publication.ingress[0].revision.load(Ordering::Acquire), 0);
        assert!(!publication.notification_superseded(0, first));
        assert_eq!(snapshot(&publication).0.generation, completed);
        assert_eq!(
            publication.control_snapshot(1).unwrap(),
            ControlSnapshot::plain(2.0)
        );
        // A later local edit to any parameter defeats this entire old baseline.
        publication.publish_control_value(0, pair).unwrap();
        assert_eq!(
            publication.try_publish_values_from(
                completed,
                &[pair, ControlSnapshot::plain(3.0)],
                &[false, true],
            ),
            Ok(None)
        );
    }

    #[test]
    fn interrupted_pair_query_fails_without_panicking_or_spinning() {
        let publication = StatePublication::new(&[0.5]);
        let mut writer = publication
            .begin_source(&publication.ingress_sequence, 0)
            .unwrap();
        let revision = publication.reserve_revision().unwrap();
        writer.dirty = true;
        publication.ingress[0]
            .value
            .store(0.75_f64.to_bits(), Ordering::Release);
        publication.ingress[0]
            .revision
            .store(revision, Ordering::Release);
        drop(writer);
        assert!(publication.notification_superseded(0, 0));
        assert_eq!(
            publication.control_snapshot(0),
            Err(PublicationError::Interrupted)
        );
    }

    #[test]
    #[allow(unsafe_code)]
    fn audio_fault_requests_main_service_even_when_no_records_are_queued() {
        use crate::{
            ChassisShared, RenderState, controls::ControlOutbox, event_ports::ClapEventPorts,
            parameters::ClapStateProjection,
        };
        use chassis_runtime::{
            parameters::ParameterDescriptor,
            schema::{ComponentId, ComponentSchema, StateSchemaVersion},
        };
        use clack_plugin::host::HostSharedHandle;
        use clap_sys::host::clap_host;
        use std::{
            ptr::NonNull,
            sync::{
                Arc,
                atomic::{AtomicBool, AtomicU32, AtomicUsize},
            },
        };
        struct Counts {
            callback: AtomicUsize,
            restart: AtomicUsize,
        }
        unsafe extern "C" fn callback(host: *const clap_host) {
            // SAFETY: this test lends its live Counts through the stable host table.
            let counts = unsafe { &*(*host).host_data.cast::<Counts>() };
            counts.callback.fetch_add(1, Ordering::Relaxed);
        }
        unsafe extern "C" fn restart(host: *const clap_host) {
            // SAFETY: the same live Counts loan applies to this callback.
            let counts = unsafe { &*(*host).host_data.cast::<Counts>() };
            counts.restart.fetch_add(1, Ordering::Relaxed);
        }
        let counts = Counts {
            callback: AtomicUsize::new(0),
            restart: AtomicUsize::new(0),
        };
        let raw_host = clap_host {
            clap_version: clap_sys::version::CLAP_VERSION,
            host_data: std::ptr::from_ref(&counts).cast_mut().cast(),
            name: c"test".as_ptr(),
            vendor: c"".as_ptr(),
            url: c"".as_ptr(),
            version: c"1".as_ptr(),
            get_extension: None,
            request_restart: Some(restart),
            request_process: None,
            request_callback: Some(callback),
        };
        // SAFETY: this initialized test host and every table/context field remain
        // live for all handle use; callbacks are thread-safe and retain no handle.
        let host = unsafe { HostSharedHandle::from_raw(NonNull::from(&raw_host)) };
        let schema = ComponentSchema::stereo_effect_with_state(
            ComponentId::new("test.control-fault").unwrap(),
            StateSchemaVersion::new(1),
            vec![ParameterDescriptor::float("gain", "Gain", 0.0, 1.0, 0.5).unwrap()],
        )
        .unwrap();
        let shared = ChassisShared {
            parameters: Arc::new(
                ClapStateProjection::new(schema.parameters(), &[("gain", 1)]).unwrap(),
            ),
            notes: Arc::new(ClapEventPorts::new(schema.event_ports(), &[]).unwrap()),
            schema: Arc::new(schema),
            render: Arc::new(RenderState::default()),
            tail: AtomicU32::new(0),
            tail_notification_pending: AtomicBool::new(false),
            outbox: Some(Arc::new(ControlOutbox::new(3, 3).unwrap())),
        };
        shared.report_audio_publication_fault(host);
        assert_eq!(counts.callback.load(Ordering::Relaxed), 0);
        shared
            .parameters
            .publication
            .fault
            .store(EXHAUSTED, Ordering::Release);
        assert!(!shared.outbox.as_ref().unwrap().has_pending());
        shared.report_audio_publication_fault(host);
        assert_eq!(counts.callback.load(Ordering::Relaxed), 1);
        assert_eq!(counts.restart.load(Ordering::Relaxed), 1);
        assert!(!shared.outbox.as_ref().unwrap().has_pending());
    }

    #[test]
    fn exhausted_final_notification_is_not_interrupted_or_superseded() {
        let publication = StatePublication::new(&[0.5]);
        publication
            .revision
            .store(LAST_REVISION - 2, Ordering::Release);
        let final_revision = publication
            .publish_control_value(0, ControlSnapshot::plain(0.75))
            .unwrap();
        assert_eq!(
            publication.try_publish_value(0, 0.25),
            Err(PublicationError::GenerationExhausted)
        );
        assert!(!publication.notification_superseded(0, final_revision));
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
