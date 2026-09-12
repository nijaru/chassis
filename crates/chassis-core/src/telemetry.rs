//! Bounded realtime-to-control telemetry publication.
//!
//! Telemetry is observational state such as meters, source-activity values, or
//! analyzer summaries. It is deliberately separate from durable parameters and
//! persistent state: losing one observation must not change product behavior.

use std::{
    hint::spin_loop,
    sync::atomic::{AtomicU32, AtomicU64, Ordering},
    vec::Vec,
};

const SNAPSHOT_RETRIES: usize = 8;
const LAST_STABLE_SEQUENCE: u64 = u64::MAX - 1;

/// One successfully completed telemetry publication.
///
/// Generation zero is the initial snapshot supplied at construction. Each
/// completed publication advances the generation by one.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TelemetryGeneration(u64);

impl TelemetryGeneration {
    /// Return the logical publication generation.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    const fn from_completed_sequence(sequence: u64) -> Self {
        debug_assert!(sequence.is_multiple_of(2));
        Self(sequence / 2)
    }
}

/// Failure to publish one telemetry snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelemetryPublishError {
    /// The supplied snapshot does not match the channel width.
    InvalidValueCount,
    /// Another publisher currently owns the publication generation.
    Busy,
    /// The monotonic publication generation has reached its terminal value.
    GenerationExhausted,
}

/// Failure to read one coherent telemetry snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelemetryReadError {
    /// The supplied destination does not match the channel width.
    InvalidValueCount,
    /// A coherent snapshot was not observed within the bounded retry budget.
    Contended,
}

/// A fixed-width collection of latest-value `f32` telemetry.
///
/// Construction allocates the atomic slots once. [`Self::try_publish`] performs
/// no allocation, blocking, locking, or reclamation and makes at most one
/// attempt to acquire the publication generation, so it is suitable for a
/// deterministic realtime callback when the product accepts coalescing/dropping
/// an observation under publisher contention.
///
/// Readers retry a bounded number of times to obtain one coherent generation.
/// Telemetry is observational: callers should retain the previous display value
/// when [`TelemetryReadError::Contended`] is returned rather than affecting DSP.
///
/// This primitive is intended for scalar and modest fixed-shape evidence such as
/// meters and compact analyzer summaries. Very large/high-rate payloads may
/// justify a different bounded transport once a real client demonstrates that
/// requirement.
pub struct F32Telemetry {
    values: Vec<AtomicU32>,
    sequence: AtomicU64,
}

impl F32Telemetry {
    /// Create telemetry initialized from `initial_values`.
    ///
    /// Allocation happens only during construction.
    #[must_use]
    pub fn new(initial_values: &[f32]) -> Self {
        Self {
            values: initial_values
                .iter()
                .map(|value| AtomicU32::new(value.to_bits()))
                .collect(),
            sequence: AtomicU64::new(0),
        }
    }

    /// Return the number of values in one snapshot.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Return whether the snapshot contains no values.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Return the most recently completed generation when no publisher is active.
    #[must_use]
    pub fn completed_generation(&self) -> Option<TelemetryGeneration> {
        let sequence = self.sequence.load(Ordering::Acquire);
        sequence
            .is_multiple_of(2)
            .then(|| TelemetryGeneration::from_completed_sequence(sequence))
    }

    /// Attempt to publish one complete snapshot without waiting.
    ///
    /// A competing publisher causes [`TelemetryPublishError::Busy`] rather than
    /// spinning. Readers never make a publisher busy; ordinary DSP -> UI use with
    /// one realtime publisher therefore does not drop observations because the UI
    /// happens to be reading.
    ///
    /// # Errors
    ///
    /// Returns an error when the snapshot width is wrong, another publisher owns
    /// the publication generation, or the generation counter is exhausted.
    pub fn try_publish(
        &self,
        values: &[f32],
    ) -> Result<TelemetryGeneration, TelemetryPublishError> {
        if values.len() != self.values.len() {
            return Err(TelemetryPublishError::InvalidValueCount);
        }

        let expected = self.sequence.load(Ordering::Acquire);
        if !expected.is_multiple_of(2) {
            return Err(TelemetryPublishError::Busy);
        }
        if expected >= LAST_STABLE_SEQUENCE {
            return Err(TelemetryPublishError::GenerationExhausted);
        }

        let completed = expected + 2;
        if self
            .sequence
            .compare_exchange(expected, expected + 1, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(TelemetryPublishError::Busy);
        }

        for (slot, value) in self.values.iter().zip(values) {
            slot.store(value.to_bits(), Ordering::Release);
        }
        self.sequence.store(completed, Ordering::Release);

        Ok(TelemetryGeneration::from_completed_sequence(completed))
    }

    /// Attempt to copy one coherent latest snapshot into `output`.
    ///
    /// The read performs a fixed number of retries and never waits for a writer.
    ///
    /// # Errors
    ///
    /// Returns [`TelemetryReadError::InvalidValueCount`] when `output` has the
    /// wrong width, or [`TelemetryReadError::Contended`] when a coherent snapshot
    /// is not observed within the bounded retry budget.
    pub fn try_snapshot_into(
        &self,
        output: &mut [f32],
    ) -> Result<TelemetryGeneration, TelemetryReadError> {
        if output.len() != self.values.len() {
            return Err(TelemetryReadError::InvalidValueCount);
        }

        for _ in 0..SNAPSHOT_RETRIES {
            let start = self.sequence.load(Ordering::Acquire);
            if !start.is_multiple_of(2) {
                spin_loop();
                continue;
            }

            for (slot, value) in self.values.iter().zip(output.iter_mut()) {
                *value = f32::from_bits(slot.load(Ordering::Acquire));
            }

            let end = self.sequence.load(Ordering::Acquire);
            if start == end && end.is_multiple_of(2) {
                return Ok(TelemetryGeneration::from_completed_sequence(end));
            }
            spin_loop();
        }

        Err(TelemetryReadError::Contended)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::Arc,
        thread::{self, yield_now},
    };

    use super::*;

    fn assert_same_bits(actual: &[f32], expected: &[f32]) {
        assert_eq!(actual.len(), expected.len());
        assert!(
            actual
                .iter()
                .zip(expected)
                .all(|(actual, expected)| actual.to_bits() == expected.to_bits())
        );
    }

    #[test]
    fn initial_snapshot_is_generation_zero() {
        let telemetry = F32Telemetry::new(&[1.0, -2.0, 3.5]);
        let mut output = [0.0; 3];

        let generation = telemetry
            .try_snapshot_into(&mut output)
            .expect("initial telemetry should be readable");

        assert_eq!(generation, TelemetryGeneration(0));
        assert_same_bits(&output, &[1.0, -2.0, 3.5]);
    }

    #[test]
    fn publication_advances_generation_and_replaces_the_snapshot() {
        let telemetry = F32Telemetry::new(&[0.0, 0.0]);

        let generation = telemetry
            .try_publish(&[0.25, 0.75])
            .expect("single publisher should acquire generation");
        assert_eq!(generation, TelemetryGeneration(1));

        let mut output = [0.0; 2];
        let observed = telemetry
            .try_snapshot_into(&mut output)
            .expect("completed telemetry should be readable");
        assert_eq!(observed, generation);
        assert_same_bits(&output, &[0.25, 0.75]);
    }

    #[test]
    fn width_mismatch_is_rejected_before_publication() {
        let telemetry = F32Telemetry::new(&[0.0, 0.0]);

        assert_eq!(
            telemetry.try_publish(&[1.0]),
            Err(TelemetryPublishError::InvalidValueCount)
        );

        let mut output = [0.0; 1];
        assert_eq!(
            telemetry.try_snapshot_into(&mut output),
            Err(TelemetryReadError::InvalidValueCount)
        );
        assert_eq!(
            telemetry.completed_generation(),
            Some(TelemetryGeneration(0))
        );
    }

    #[test]
    fn concurrent_reads_never_accept_a_mixed_generation() {
        const LAST: u16 = 4_000;
        let telemetry = Arc::new(F32Telemetry::new(&[0.0; 4]));
        let publisher = Arc::clone(&telemetry);

        let writer = thread::spawn(move || {
            for index in 1..=LAST {
                let value = f32::from(index);
                loop {
                    match publisher.try_publish(&[value; 4]) {
                        Ok(_) => break,
                        Err(TelemetryPublishError::Busy) => yield_now(),
                        Err(error) => panic!("unexpected telemetry publication failure: {error:?}"),
                    }
                }
            }
        });

        let mut output = [0.0; 4];
        while telemetry
            .completed_generation()
            .is_none_or(|generation| generation.get() < u64::from(LAST))
        {
            if telemetry.try_snapshot_into(&mut output).is_ok() {
                let expected = output[0].to_bits();
                assert!(output.iter().all(|value| value.to_bits() == expected));
            }
            yield_now();
        }

        writer.join().expect("telemetry publisher should complete");
        let generation = telemetry
            .try_snapshot_into(&mut output)
            .expect("final telemetry should be readable");
        assert_eq!(generation.get(), u64::from(LAST));
        assert_same_bits(&output, &[f32::from(LAST); 4]);
    }
}
