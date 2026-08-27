//! Format-independent processor activation configuration.

use core::fmt;

/// Whether a processing run is constrained by realtime delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProcessMode {
    /// The caller has a realtime deadline.
    Realtime,
    /// The caller permits non-realtime/offline rendering behavior.
    Offline,
}

/// Host/runtime configuration supplied when a processor is activated.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProcessConfig {
    sample_rate: f64,
    min_frames: u32,
    max_frames: u32,
    mode: ProcessMode,
}

impl ProcessConfig {
    /// Construct a validated activation configuration.
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
        mode: ProcessMode,
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
            mode,
        })
    }

    /// Return the activation sample rate in Hz.
    #[must_use]
    pub const fn sample_rate(self) -> f64 {
        self.sample_rate
    }

    /// Return the minimum frame count the runtime advertises for this activation.
    #[must_use]
    pub const fn min_frames(self) -> u32 {
        self.min_frames
    }

    /// Return the maximum frame count provisioned for this activation.
    #[must_use]
    pub const fn max_frames(self) -> u32 {
        self.max_frames
    }

    /// Return the realtime/offline mode.
    #[must_use]
    pub const fn mode(self) -> ProcessMode {
        self.mode
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
            ProcessConfig::new(0.0, 0, 512, ProcessMode::Realtime),
            Err(ProcessConfigError::InvalidSampleRate)
        );
        assert_eq!(
            ProcessConfig::new(48_000.0, 1024, 512, ProcessMode::Realtime),
            Err(ProcessConfigError::InvalidFrameRange)
        );
    }

    #[test]
    fn accepts_variable_block_sizes() {
        let config = ProcessConfig::new(48_000.0, 1, 2048, ProcessMode::Offline)
            .expect("test configuration is valid");

        assert_eq!(config.sample_rate(), 48_000.0);
        assert_eq!(config.min_frames(), 1);
        assert_eq!(config.max_frames(), 2048);
        assert_eq!(config.mode(), ProcessMode::Offline);
    }
}
