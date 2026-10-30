//! Validated, format-independent block-start transport observations.

use core::fmt;

/// A musical time signature, without assuming a particular denominator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeSignature {
    /// Number of denominator units per bar.
    pub numerator: u16,
    /// Note value of one unit (for example, 8 in 7/8).
    pub denominator: u16,
}

/// Loop endpoints in the units specified by the containing field.
///
/// Equal endpoints are permitted, including an unconfigured zero-length loop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoopRange {
    /// Inclusive start position.
    pub start: f64,
    /// End position, not less than the start.
    pub end: f64,
}

/// Candidate block-start transport values supplied by a deployment/application.
///
/// Availability is independent of transport state: stopped is not unknown, and
/// an inactive loop may still have known endpoints. Signed positions permit
/// pre-roll and timeline locations before the project origin. Beats mean
/// quarter notes, independently of the time-signature denominator.
///
/// Validate with [`TransportSnapshot::new`] before publishing to a processor.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct TransportValues {
    /// Whether playback is running.
    pub playing: Option<bool>,
    /// Whether recording is running.
    pub recording: Option<bool>,
    /// Whether the loop/cycle is active.
    pub loop_active: Option<bool>,
    /// Whether the host is in recording pre-roll.
    pub within_pre_roll: Option<bool>,
    /// Tempo at sample zero, in quarter notes per minute.
    pub tempo_bpm: Option<f64>,
    /// Tempo change in BPM per sample, when explicitly supplied by the source.
    ///
    /// Requires an available tempo. Missing does not assert a constant tempo.
    /// This is metadata, not a promise that no later timed update occurs.
    pub tempo_increment: Option<f64>,
    /// Project timeline position in samples, not a monotonic process clock.
    pub sample_position: Option<i64>,
    /// Project timeline position in seconds, independently of sample position.
    pub seconds_position: Option<f64>,
    /// Project timeline position in quarter notes.
    pub beat_position: Option<f64>,
    /// Start of the current bar in quarter notes.
    pub bar_start: Option<f64>,
    /// Zero-based bar number at the project origin, if supplied by the source.
    pub bar_number: Option<i64>,
    /// Musical meter; both components must be nonzero.
    pub time_signature: Option<TimeSignature>,
    /// Loop endpoints in quarter notes.
    pub loop_beats: Option<LoopRange>,
    /// Loop endpoints in seconds.
    pub loop_seconds: Option<LoopRange>,
}

/// Best-known transport values at the start of one processing block.
///
/// Values are observations, not a clock to accumulate across calls. Hosts may
/// seek, loop, stop, or omit information on any call. There is no implied
/// intra-block transport stream or conversion between independent timelines.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransportSnapshot(TransportValues);

impl TransportSnapshot {
    /// Validate a candidate snapshot without allocation or timeline inference.
    ///
    /// # Errors
    ///
    /// Rejects non-finite positions, invalid tempo/slope, zero meter components,
    /// and non-finite or reversed loop endpoints.
    pub fn new(values: TransportValues) -> Result<Self, TransportSnapshotError> {
        if values
            .tempo_bpm
            .is_some_and(|tempo| !tempo.is_finite() || tempo <= 0.0)
        {
            return Err(TransportSnapshotError::InvalidTempo);
        }
        if values
            .tempo_increment
            .is_some_and(|increment| !increment.is_finite() || values.tempo_bpm.is_none())
        {
            return Err(TransportSnapshotError::InvalidTempoIncrement);
        }
        if [
            values.seconds_position,
            values.beat_position,
            values.bar_start,
        ]
        .into_iter()
        .flatten()
        .any(|position| !position.is_finite())
        {
            return Err(TransportSnapshotError::InvalidPosition);
        }
        if values
            .time_signature
            .is_some_and(|signature| signature.numerator == 0 || signature.denominator == 0)
        {
            return Err(TransportSnapshotError::InvalidTimeSignature);
        }
        if [values.loop_beats, values.loop_seconds]
            .into_iter()
            .flatten()
            .any(|range| {
                !range.start.is_finite() || !range.end.is_finite() || range.end < range.start
            })
        {
            return Err(TransportSnapshotError::InvalidLoopRange);
        }
        Ok(Self(values))
    }

    /// Construct a snapshot with no available values.
    pub const UNKNOWN: Self = Self(TransportValues {
        playing: None,
        recording: None,
        loop_active: None,
        within_pre_roll: None,
        tempo_bpm: None,
        tempo_increment: None,
        sample_position: None,
        seconds_position: None,
        beat_position: None,
        bar_start: None,
        bar_number: None,
        time_signature: None,
        loop_beats: None,
        loop_seconds: None,
    });

    /// Borrow the validated values; mutating a copy cannot alter this snapshot.
    #[must_use]
    pub const fn values(&self) -> &TransportValues {
        &self.0
    }
}

/// Invalid transport metadata supplied at a runtime boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportSnapshotError {
    /// An available tempo was non-finite or not positive.
    InvalidTempo,
    /// A tempo increment was non-finite or had no associated tempo.
    InvalidTempoIncrement,
    /// An available floating-point timeline position was non-finite.
    InvalidPosition,
    /// A meter numerator or denominator was zero.
    InvalidTimeSignature,
    /// A loop endpoint was non-finite or the end preceded the start.
    InvalidLoopRange,
}

impl fmt::Display for TransportSnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidTempo => "transport tempo must be finite and positive",
            Self::InvalidTempoIncrement => {
                "transport tempo increment must be finite and have a tempo"
            }
            Self::InvalidPosition => "transport position must be finite",
            Self::InvalidTimeSignature => "transport time signature components must be nonzero",
            Self::InvalidLoopRange => "transport loop endpoints must be finite and ordered",
        })
    }
}

impl std::error::Error for TransportSnapshotError {}

impl Default for TransportSnapshot {
    fn default() -> Self {
        Self::UNKNOWN
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_independent_signed_timelines_and_inactive_loops() {
        let values = TransportValues {
            playing: Some(false),
            recording: None,
            loop_active: Some(false),
            within_pre_roll: Some(true),
            tempo_bpm: Some(123.0),
            tempo_increment: Some(-0.01),
            sample_position: Some(i64::MIN),
            seconds_position: Some(-2.5),
            beat_position: Some(-7.0),
            bar_start: Some(-9.5),
            bar_number: Some(-3),
            time_signature: Some(TimeSignature {
                numerator: 7,
                denominator: 8,
            }),
            loop_beats: Some(LoopRange {
                start: -7.0,
                end: 0.0,
            }),
            loop_seconds: Some(LoopRange {
                start: 0.0,
                end: 0.0,
            }),
        };
        assert_eq!(TransportSnapshot::new(values).unwrap().values(), &values);
        assert_eq!(TransportSnapshot::UNKNOWN, TransportSnapshot::default());
        assert_ne!(
            TransportSnapshot::new(values).unwrap(),
            TransportSnapshot::UNKNOWN
        );
        // A musical timeline need not have a seconds timeline or a tempo.
        assert!(
            TransportSnapshot::new(TransportValues {
                beat_position: Some(42.0),
                ..TransportValues::default()
            })
            .is_ok()
        );
    }

    #[test]
    fn rejects_invalid_available_metadata() {
        for tempo in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(
                TransportSnapshot::new(TransportValues {
                    tempo_bpm: Some(tempo),
                    ..TransportValues::default()
                }),
                Err(TransportSnapshotError::InvalidTempo)
            );
        }
        for increment in [f64::NAN, f64::NEG_INFINITY] {
            assert_eq!(
                TransportSnapshot::new(TransportValues {
                    tempo_bpm: Some(120.0),
                    tempo_increment: Some(increment),
                    ..TransportValues::default()
                }),
                Err(TransportSnapshotError::InvalidTempoIncrement)
            );
        }
        assert_eq!(
            TransportSnapshot::new(TransportValues {
                tempo_increment: Some(0.0),
                ..TransportValues::default()
            }),
            Err(TransportSnapshotError::InvalidTempoIncrement)
        );
        for field in 0..3 {
            let mut values = TransportValues::default();
            match field {
                0 => values.seconds_position = Some(f64::NAN),
                1 => values.beat_position = Some(f64::INFINITY),
                _ => values.bar_start = Some(f64::NEG_INFINITY),
            }
            assert_eq!(
                TransportSnapshot::new(values),
                Err(TransportSnapshotError::InvalidPosition)
            );
        }
        for (numerator, denominator) in [(0, 4), (4, 0)] {
            assert_eq!(
                TransportSnapshot::new(TransportValues {
                    time_signature: Some(TimeSignature {
                        numerator,
                        denominator
                    }),
                    ..TransportValues::default()
                }),
                Err(TransportSnapshotError::InvalidTimeSignature)
            );
        }
        for range in [
            LoopRange {
                start: 2.0,
                end: 1.0,
            },
            LoopRange {
                start: f64::NAN,
                end: 1.0,
            },
            LoopRange {
                start: 0.0,
                end: f64::INFINITY,
            },
        ] {
            for beats in [true, false] {
                let mut values = TransportValues::default();
                if beats {
                    values.loop_beats = Some(range);
                } else {
                    values.loop_seconds = Some(range);
                }
                assert_eq!(
                    TransportSnapshot::new(values),
                    Err(TransportSnapshotError::InvalidLoopRange)
                );
            }
        }
    }
}
