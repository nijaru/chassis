//! Format-independent processor activation and per-call scheduling contracts.

use core::{fmt, num::NonZeroU32};

/// Scheduling/quality context for one processing call.
///
/// This is semantic rather than format-specific. Backends emit only modes they
/// can represent. VST3 prefetch maps to [`Self::BufferedRealtime`]; CLAP core
/// render mode maps realtime/offline and does not emit the buffered mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProcessMode {
    /// The caller has a realtime delivery deadline.
    Realtime,
    /// Processing may be scheduled ahead/irregularly but must retain
    /// realtime-quality/nonblocking behavior rather than wait for wall-clock
    /// realtime or choose an offline-only algorithm.
    BufferedRealtime,
    /// Non-realtime rendering where a product may deliberately choose a more
    /// expensive offline-quality path if it supports one.
    Offline,
}

/// Resource bounds/configuration supplied when a processor is activated.
///
/// Per-call scheduling mode is intentionally separate: some formats can change
/// realtime/prefetch scheduling without reactivation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProcessConfig {
    sample_rate: f64,
    guaranteed_min_frames: Option<NonZeroU32>,
    max_frames: NonZeroU32,
}

impl ProcessConfig {
    /// Construct validated activation/resource bounds.
    ///
    /// `guaranteed_min_frames` is `None` when a backend does not promise a
    /// positive minimum. That is distinct from whether a particular backend may
    /// legally issue a zero-frame process call.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessConfigError::InvalidSampleRate`] when `sample_rate` is
    /// non-finite or not positive. Returns
    /// [`ProcessConfigError::InvalidFrameRange`] when a guaranteed minimum is
    /// greater than `max_frames`.
    pub fn new(
        sample_rate: f64,
        guaranteed_min_frames: Option<NonZeroU32>,
        max_frames: NonZeroU32,
    ) -> Result<Self, ProcessConfigError> {
        if !sample_rate.is_finite() || sample_rate <= 0.0 {
            return Err(ProcessConfigError::InvalidSampleRate);
        }
        if guaranteed_min_frames.is_some_and(|minimum| minimum > max_frames) {
            return Err(ProcessConfigError::InvalidFrameRange);
        }

        Ok(Self {
            sample_rate,
            guaranteed_min_frames,
            max_frames,
        })
    }

    /// Return the activation sample rate in Hz.
    #[must_use]
    pub const fn sample_rate(self) -> f64 {
        self.sample_rate
    }

    /// Return a positive lower block-size guarantee when the backend has one.
    ///
    /// `None` means no positive minimum is promised; it does not by itself say
    /// whether a zero-frame process call is legal.
    #[must_use]
    pub const fn guaranteed_min_frames(self) -> Option<NonZeroU32> {
        self.guaranteed_min_frames
    }

    /// Return the non-zero maximum frame count provisioned for this activation.
    #[must_use]
    pub const fn max_frames(self) -> NonZeroU32 {
        self.max_frames
    }
}

/// Invalid processor activation configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessConfigError {
    /// Sample rate was zero, negative, NaN, or infinite.
    InvalidSampleRate,
    /// A known minimum block size exceeded the maximum.
    InvalidFrameRange,
}

impl fmt::Display for ProcessConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSampleRate => formatter.write_str("sample rate must be finite and positive"),
            Self::InvalidFrameRange => {
                formatter.write_str("guaranteed minimum frame count must not exceed maximum")
            }
        }
    }
}

impl std::error::Error for ProcessConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_sample_rate() {
        let max_frames = NonZeroU32::new(512).expect("test maximum is non-zero");
        assert_eq!(
            ProcessConfig::new(0.0, None, max_frames),
            Err(ProcessConfigError::InvalidSampleRate)
        );
    }

    #[test]
    fn rejects_minimum_above_maximum() {
        let minimum = NonZeroU32::new(1024).expect("test minimum is non-zero");
        let maximum = NonZeroU32::new(512).expect("test maximum is non-zero");

        assert_eq!(
            ProcessConfig::new(48_000.0, Some(minimum), maximum),
            Err(ProcessConfigError::InvalidFrameRange)
        );
    }

    #[test]
    fn accepts_unknown_minimum() {
        let maximum = NonZeroU32::new(2048).expect("test maximum is non-zero");
        let config = ProcessConfig::new(48_000.0, None, maximum)
            .expect("test configuration is valid");

        assert_eq!(config.sample_rate(), 48_000.0);
        assert_eq!(config.guaranteed_min_frames(), None);
        assert_eq!(config.max_frames(), maximum);
    }

    #[test]
    fn process_modes_are_per_call_semantics() {
        assert_ne!(ProcessMode::Realtime, ProcessMode::BufferedRealtime);
        assert_ne!(ProcessMode::BufferedRealtime, ProcessMode::Offline);
    }
}
