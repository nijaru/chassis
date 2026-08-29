use std::{
    hint::spin_loop,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    vec::Vec,
};

const SNAPSHOT_RETRIES: usize = 8;
const LAST_STABLE_GENERATION: u64 = u64::MAX - 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PublicationError {
    GenerationExhausted,
    InvalidValueCount,
}

pub(crate) struct ScalarPublication {
    values: Vec<AtomicU64>,
    generation: AtomicU64,
    pending: AtomicBool,
}

impl ScalarPublication {
    pub(crate) fn new(initial_values: &[f64]) -> Self {
        Self {
            values: initial_values
                .iter()
                .map(|value| AtomicU64::new(value.to_bits()))
                .collect(),
            generation: AtomicU64::new(0),
            pending: AtomicBool::new(false),
        }
    }

    pub(crate) fn value_count(&self) -> usize {
        self.values.len()
    }

    pub(crate) fn value(&self, index: usize) -> f64 {
        f64::from_bits(self.values[index].load(Ordering::Acquire))
    }

    pub(crate) fn request_sync(&self) {
        self.pending.store(true, Ordering::Release);
    }

    pub(crate) fn take_pending(&self) -> bool {
        self.pending.swap(false, Ordering::AcqRel)
    }

    fn try_begin_write(&self, expected: u64) -> Option<u64> {
        if !expected.is_multiple_of(2) || expected >= LAST_STABLE_GENERATION {
            return None;
        }
        let completed = expected + 2;
        self.generation
            .compare_exchange(
                expected,
                expected + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .ok()?;
        Some(completed)
    }

    fn finish_write(&self, completed: u64) {
        debug_assert!(completed.is_multiple_of(2));
        debug_assert!(completed <= LAST_STABLE_GENERATION);
        self.generation.store(completed, Ordering::Release);
        self.pending.store(true, Ordering::Release);
    }

    #[cfg(test)]
    pub(crate) fn publish_value_control(
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

    pub(crate) fn try_publish_value(&self, index: usize, value: f64) -> bool {
        let expected = self.generation.load(Ordering::Acquire);
        let Some(completed) = self.try_begin_write(expected) else {
            return false;
        };
        self.values[index].store(value.to_bits(), Ordering::Release);
        self.finish_write(completed);
        true
    }

    pub(crate) fn publish_values_control(&self, values: &[f64]) -> Result<(), PublicationError> {
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
            self.finish_write(completed);
            return Ok(());
        }
    }

    pub(crate) fn try_publish_values_from(&self, expected: u64, values: &[f64]) -> bool {
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

    pub(crate) fn try_snapshot_into(&self, output: &mut [f64]) -> Option<u64> {
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
            let end = self.generation.load(Ordering::Acquire);
            if start == end && end.is_multiple_of(2) {
                return Some(end);
            }
        }
        None
    }

    pub(crate) fn snapshot_control_into(
        &self,
        output: &mut [f64],
    ) -> Result<u64, PublicationError> {
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
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Writer {
        First,
        Second,
    }

    impl Writer {
        const fn index(self) -> usize {
            match self {
                Self::First => 0,
                Self::Second => 1,
            }
        }
    }

    #[derive(Clone, Copy)]
    struct WriterModel {
        generation: u8,
        owner: Option<Writer>,
        acquired: [bool; 2],
        steps: [usize; 2],
    }

    impl WriterModel {
        const fn new() -> Self {
            Self {
                generation: 0,
                owner: None,
                acquired: [false; 2],
                steps: [0; 2],
            }
        }

        fn step(&mut self, writer: Writer) {
            let index = writer.index();
            match self.steps[index] {
                0 => {
                    if self.generation == 0 {
                        assert!(self.owner.is_none());
                        self.generation = 1;
                        self.owner = Some(writer);
                        self.acquired[index] = true;
                    }
                }
                1 => {
                    if self.acquired[index] {
                        assert_eq!(self.owner, Some(writer));
                        self.generation = 2;
                        self.owner = None;
                    }
                }
                _ => unreachable!("model writer has two steps"),
            }
            self.steps[index] += 1;
        }

        fn assert_serialized(self) {
            assert_eq!(self.acquired.into_iter().filter(|acquired| *acquired).count(), 1);
            assert_eq!(self.owner, None);
            assert_eq!(self.generation, 2);
        }
    }

    fn enumerate_writer_interleavings(first_left: usize, second_left: usize, model: WriterModel) {
        if first_left == 0 && second_left == 0 {
            model.assert_serialized();
            return;
        }
        if first_left != 0 {
            let mut next = model;
            next.step(Writer::First);
            enumerate_writer_interleavings(first_left - 1, second_left, next);
        }
        if second_left != 0 {
            let mut next = model;
            next.step(Writer::Second);
            enumerate_writer_interleavings(first_left, second_left - 1, next);
        }
    }

    #[derive(Clone, Copy)]
    enum Actor {
        Reader,
        Writer,
    }

    #[derive(Clone, Copy)]
    struct SnapshotModel {
        generation: u8,
        values: [u8; 2],
        reader_start: Option<u8>,
        reader_values: [u8; 2],
        reader_end: Option<u8>,
        reader_step: usize,
        writer_step: usize,
    }

    impl SnapshotModel {
        const fn new() -> Self {
            Self {
                generation: 0,
                values: [0, 0],
                reader_start: None,
                reader_values: [0, 0],
                reader_end: None,
                reader_step: 0,
                writer_step: 0,
            }
        }

        fn step(&mut self, actor: Actor) {
            match actor {
                Actor::Reader => {
                    match self.reader_step {
                        0 => self.reader_start = Some(self.generation),
                        1 => self.reader_values[0] = self.values[0],
                        2 => self.reader_values[1] = self.values[1],
                        3 => self.reader_end = Some(self.generation),
                        _ => unreachable!("model reader has four steps"),
                    }
                    self.reader_step += 1;
                }
                Actor::Writer => {
                    match self.writer_step {
                        0 => {
                            assert_eq!(self.generation, 0);
                            self.generation = 1;
                        }
                        1 => self.values[0] = 1,
                        2 => self.values[1] = 1,
                        3 => self.generation = 2,
                        _ => unreachable!("model writer has four steps"),
                    }
                    self.writer_step += 1;
                }
            }
        }

        fn assert_snapshot_coherent(self) {
            let start = self.reader_start.expect("reader started");
            let end = self.reader_end.expect("reader finished");
            if start == end && end.is_multiple_of(2) {
                assert!(matches!(self.reader_values, [0, 0] | [1, 1]));
            }
        }
    }

    fn enumerate_snapshot_interleavings(
        readers_left: usize,
        writers_left: usize,
        model: SnapshotModel,
    ) {
        if readers_left == 0 && writers_left == 0 {
            model.assert_snapshot_coherent();
            return;
        }
        if readers_left != 0 {
            let mut next = model;
            next.step(Actor::Reader);
            enumerate_snapshot_interleavings(readers_left - 1, writers_left, next);
        }
        if writers_left != 0 {
            let mut next = model;
            next.step(Actor::Writer);
            enumerate_snapshot_interleavings(readers_left, writers_left - 1, next);
        }
    }

    #[test]
    fn abstract_same_generation_writers_are_serialized() {
        enumerate_writer_interleavings(2, 2, WriterModel::new());
    }

    #[test]
    fn abstract_snapshot_interleavings_never_accept_mixed_values() {
        enumerate_snapshot_interleavings(4, 4, SnapshotModel::new());
    }

    #[test]
    fn stale_generation_cannot_publish_after_control_write() {
        let publication = ScalarPublication::new(&[0.5, 0.0]);
        let mut snapshot = [0.0; 2];
        let generation = publication
            .try_snapshot_into(&mut snapshot)
            .expect("initial snapshot is coherent");

        publication
            .publish_values_control(&[0.5, 4.0])
            .expect("control publication succeeds");
        snapshot[0] = 0.25;

        assert!(!publication.try_publish_values_from(generation, &snapshot));
        assert_eq!(publication.value(0).to_bits(), 0.5_f64.to_bits());
        assert_eq!(publication.value(1).to_bits(), 4.0_f64.to_bits());
    }

    #[test]
    fn realtime_attempts_are_bounded_while_a_writer_is_active() {
        let publication = ScalarPublication::new(&[0.5]);
        publication.generation.store(1, Ordering::Release);
        let mut snapshot = [0.0];

        assert!(!publication.try_publish_value(0, 0.25));
        assert!(publication.try_snapshot_into(&mut snapshot).is_none());
    }

    #[test]
    fn generation_exhaustion_is_terminal_instead_of_wrapping() {
        let publication = ScalarPublication::new(&[0.5]);
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
            publication.publish_values_control(&[0.25]),
            Err(PublicationError::GenerationExhausted)
        );

        let mut snapshot = [0.0];
        assert_eq!(
            publication.try_snapshot_into(&mut snapshot),
            Some(LAST_STABLE_GENERATION)
        );
        assert_eq!(snapshot[0].to_bits(), 0.75_f64.to_bits());
    }

    #[test]
    fn control_operations_reject_wrong_value_counts() {
        let publication = ScalarPublication::new(&[0.5, 0.0]);
        let mut snapshot = [0.0];

        assert_eq!(
            publication.publish_values_control(&[0.25]),
            Err(PublicationError::InvalidValueCount)
        );
        assert_eq!(
            publication.snapshot_control_into(&mut snapshot),
            Err(PublicationError::InvalidValueCount)
        );
    }
}
