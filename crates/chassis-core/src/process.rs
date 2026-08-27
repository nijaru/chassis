//! Format-independent processor activation and per-call scheduling contracts.

use core::fmt;

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
    min_frames: u32,
    max_frames: u32,
}

impl ProcessConfig {
    /// Construct validated activation/resource bounds.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessConfigError::InvalidSampleRate`] when `sample_rate` is
    /// non-finite or not positive. Returns
    /// [`ProcessConfigError::InvalidFrameRange`] when `max_frames` is zero or
    /// `min_frames` is greater than `max_frames`.
    pub fn new(
        sample_rate: f64,
        min_frames: u32,
        max_frames: u32,
    ) -> Result<Self, ProcessConfigError> {
        if !sample_rate.is_finite() || sample_rate <= 0.0 {
            return Err(ProcessConfigError::InvalidSampleRate);
        }
        if max_frames == 0 || min_frames > max_frames {
            return Err(ProcessConfigError::InvalidFrameRange);
        }

        Ok(Self {
            sample_rate,
            min_frames,
            max_frames,
        })
    }

    /// Return the activation sample rate in Hz.
    #[must_use]
    pub const fn sample_rate(self) -> f64 {
        self.sample_rate
    }

    /// Return the minimum frame count advertised for this activation.
    ///
    /// Zero is valid when a backend cannot promise a positive minimum or may
    /// issue a legal zero-frame callback.
    #[must_use]
    pub const fn min_frames(self) -> u32 {
        self.min_frames
    }

    /// Return the maximum frame count provisioned for this activation.
    #[must_use]
    pub const fn max_frames(self) -> u32 {
        self.max_frames
    }
}

/// Invalid processor activation configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessConfigError {
    /// Sample rate was zero, negative, NaN, or infinite.
    InvalidSampleRate,
    /// Frame bounds were inconsistent or had a zero maximum.
    InvalidFrameRange,
}

impl fmt::Display for ProcessConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSampleRate => formatter.write_str("sample rate must be finite and positive"),
            Self::InvalidFrameRange => {
                formatter.write_str("frame range must have max > 0 and min <= max")
            }
        }
    }
}

impl std::error::Error for ProcessConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_values() {
        assert_eq!(
            ProcessConfig::new(0.0, 0, 512),
            Err(ProcessConfigError::InvalidSampleRate)
        );
        assert_eq!(
            ProcessConfig::new(48_000.0, 1024, 512),
            Err(ProcessConfigError::InvalidFrameRange)
        );
    }

    #[test]
    fn accepts_zero_minimum_and_variable_blocks() {
        let config = ProcessConfig::new(48_000.0, 0, 2048)
            .expect("test configuration is valid");

        assert_eq!(config.sample_rate(), 48_000.0);
        assert_eq!(config.min_frames(), 0);
        assert_eq!(config.max_frames(), 2048);
    }

    #[test]
    fn process_modes_are_per_call_semantics() {
        assert_ne!(ProcessMode::Realtime, ProcessMode::BufferedRealtime);
        assert_ne!(ProcessMode::BufferedRealtime, ProcessMode::Offline);
    }
}
