//! Format-independent processor activation and per-call process contracts.

use core::{fmt, num::NonZeroU32};

use crate::{
    audio::AudioIoConfiguration, automation::ParameterEvents, buffer::ChannelBuffer,
    parameters::ParameterAutomationError,
};

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

/// Best-known transport values at the start of one processing block.
///
/// Every field is optional because a backend may not provide it. Values are a
/// snapshot, not a promise of timestamped intra-block transport changes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransportSnapshot {
    playing: Option<bool>,
    recording: Option<bool>,
    tempo_bpm: Option<f64>,
    sample_position: Option<i64>,
}

impl TransportSnapshot {
    /// Construct a snapshot, validating an available tempo.
    ///
    /// # Errors
    ///
    /// Returns [`TransportSnapshotError::InvalidTempo`] when `tempo_bpm` is
    /// non-finite or not positive.
    pub fn new(
        playing: Option<bool>,
        recording: Option<bool>,
        tempo_bpm: Option<f64>,
        sample_position: Option<i64>,
    ) -> Result<Self, TransportSnapshotError> {
        if tempo_bpm.is_some_and(|tempo| !tempo.is_finite() || tempo <= 0.0) {
            return Err(TransportSnapshotError::InvalidTempo);
        }
        Ok(Self {
            playing,
            recording,
            tempo_bpm,
            sample_position,
        })
    }

    /// Construct a snapshot with no available transport values.
    #[must_use]
    pub const fn unknown() -> Self {
        Self {
            playing: None,
            recording: None,
            tempo_bpm: None,
            sample_position: None,
        }
    }

    /// Return the known playing state.
    #[must_use]
    pub const fn playing(self) -> Option<bool> {
        self.playing
    }

    /// Return the known recording state.
    #[must_use]
    pub const fn recording(self) -> Option<bool> {
        self.recording
    }

    /// Return the known tempo in beats per minute.
    #[must_use]
    pub const fn tempo_bpm(self) -> Option<f64> {
        self.tempo_bpm
    }

    /// Return the known absolute sample position.
    #[must_use]
    pub const fn sample_position(self) -> Option<i64> {
        self.sample_position
    }
}

/// Invalid block-start transport metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportSnapshotError {
    /// An available tempo was zero, negative, NaN, or infinite.
    InvalidTempo,
}

impl fmt::Display for TransportSnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTempo => {
                formatter.write_str("transport tempo must be finite and positive")
            }
        }
    }
}

impl std::error::Error for TransportSnapshotError {}

/// Semantic context for one process block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProcessContext<'a> {
    mode: ProcessMode,
    transport: TransportSnapshot,
    parameter_events: ParameterEvents<'a>,
}

impl<'a> ProcessContext<'a> {
    /// Construct process context from backend-normalized values.
    #[must_use]
    pub const fn new(
        mode: ProcessMode,
        transport: TransportSnapshot,
        parameter_events: ParameterEvents<'a>,
    ) -> Self {
        Self {
            mode,
            transport,
            parameter_events,
        }
    }

    /// Return the scheduling/quality mode for this block.
    #[must_use]
    pub const fn mode(self) -> ProcessMode {
        self.mode
    }

    /// Return the block-start transport snapshot.
    #[must_use]
    pub const fn transport(self) -> TransportSnapshot {
        self.transport
    }

    /// Return the validated borrowed parameter events.
    #[must_use]
    pub const fn parameter_events(self) -> ParameterEvents<'a> {
        self.parameter_events
    }
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
    max_parameter_events: u32,
}

impl ProcessConfig {
    /// Construct validated activation/resource bounds.
    ///
    /// `guaranteed_min_frames` is `None` when a backend does not promise a
    /// positive minimum. That is distinct from whether a particular backend may
    /// legally issue a zero-frame process call. `max_parameter_events` is the
    /// activation-owned bound for normalized parameter events in one callback;
    /// zero means that the component/adapter accepts no parameter events.
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
        max_parameter_events: u32,
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
            max_parameter_events,
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

    /// Return the maximum normalized parameter events accepted per callback.
    #[must_use]
    pub const fn max_parameter_events(self) -> u32 {
        self.max_parameter_events
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
            Self::InvalidSampleRate => {
                formatter.write_str("sample rate must be finite and positive")
            }
            Self::InvalidFrameRange => {
                formatter.write_str("guaranteed minimum frame count must not exceed maximum")
            }
        }
    }
}

impl std::error::Error for ProcessConfigError {}

/// Immutable configuration accepted for one processor activation.
///
/// Product code receives this only after framework structural I/O validation.
/// Product-specific I/O policy remains an explicit future extension; this type
/// does not silently infer it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActivationConfig<'a> {
    process: ProcessConfig,
    audio_io: AudioIoConfiguration<'a>,
}

impl<'a> ActivationConfig<'a> {
    pub(crate) const fn new(process: ProcessConfig, audio_io: AudioIoConfiguration<'a>) -> Self {
        Self { process, audio_io }
    }

    /// Return the process resource bounds.
    #[must_use]
    pub const fn process(self) -> ProcessConfig {
        self.process
    }

    /// Return the accepted whole-component audio configuration.
    #[must_use]
    pub const fn audio_io(self) -> AudioIoConfiguration<'a> {
        self.audio_io
    }
}

/// Borrowed realtime process call passed to product DSP.
///
/// Buffer entries already contain safe Rust references whose aliasing has been
/// proved by the adapter/runtime boundary. Construction validates only facts
/// that can vary per callback: frame-count bounds, event bounds/context, and
/// slice lengths. Stable endpoint/schema mapping should be resolved outside the
/// hot path rather than rescanned with allocation or quadratic work every block.
pub struct ProcessBlock<'buffers, 'samples, 'context, S> {
    frame_count: u32,
    context: ProcessContext<'context>,
    buffers: &'buffers mut [ChannelBuffer<'samples, S>],
}

impl<'buffers, 'samples, 'context, S> ProcessBlock<'buffers, 'samples, 'context, S> {
    pub(crate) fn new(
        config: &ActivationConfig<'_>,
        frame_count: u32,
        context: ProcessContext<'context>,
        buffers: &'buffers mut [ChannelBuffer<'samples, S>],
    ) -> Result<Self, ProcessBlockError> {
        if context.parameter_events().frame_count() != frame_count {
            return Err(ProcessBlockError::ParameterEventFrameCountMismatch {
                expected: frame_count,
                actual: context.parameter_events().frame_count(),
            });
        }
        let maximum_parameter_events = usize::try_from(config.process().max_parameter_events())
            .map_err(|_| ProcessBlockError::ParameterEventLimitNotRepresentable)?;
        if context.parameter_events().len() > maximum_parameter_events {
            return Err(ProcessBlockError::ParameterEventCountTooLarge {
                actual: context.parameter_events().len(),
                maximum: config.process().max_parameter_events(),
            });
        }
        if let Some(minimum) = config.process().guaranteed_min_frames()
            && frame_count < minimum.get()
        {
            return Err(ProcessBlockError::BelowGuaranteedMinimum {
                actual: frame_count,
                minimum: minimum.get(),
            });
        }

        if frame_count > config.process().max_frames().get() {
            return Err(ProcessBlockError::ExceedsActivatedMaximum {
                actual: frame_count,
                maximum: config.process().max_frames().get(),
            });
        }

        let expected = usize::try_from(frame_count)
            .map_err(|_| ProcessBlockError::FrameCountNotRepresentable)?;

        for buffer in &*buffers {
            if buffer.frame_count() != expected {
                return Err(ProcessBlockError::BufferFrameCountMismatch {
                    expected,
                    actual: buffer.frame_count(),
                });
            }
        }

        Ok(Self {
            frame_count,
            context,
            buffers,
        })
    }

    /// Return the actual number of frames in this callback.
    #[must_use]
    pub const fn frame_count(&self) -> u32 {
        self.frame_count
    }

    /// Return the scheduling/quality mode for this callback.
    #[must_use]
    pub const fn mode(&self) -> ProcessMode {
        self.context.mode()
    }

    /// Return the complete semantic context for this callback.
    #[must_use]
    pub const fn context(&self) -> ProcessContext<'context> {
        self.context
    }

    /// Return the block-start transport snapshot.
    #[must_use]
    pub const fn transport(&self) -> TransportSnapshot {
        self.context.transport()
    }

    /// Return validated borrowed parameter automation events.
    #[must_use]
    pub const fn parameter_events(&self) -> ParameterEvents<'context> {
        self.context.parameter_events()
    }

    /// Borrow all safe channel views for processing.
    #[must_use]
    pub fn buffers_mut(&mut self) -> &mut [ChannelBuffer<'samples, S>] {
        self.buffers
    }
}

/// Invalid process callback dimensions or semantic context relative to the active configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessBlockError {
    /// A callback was smaller than a positive minimum guaranteed at activation.
    BelowGuaranteedMinimum {
        /// Actual callback frame count.
        actual: u32,
        /// Guaranteed positive minimum.
        minimum: u32,
    },
    /// A callback exceeded the maximum storage provisioned at activation.
    ExceedsActivatedMaximum {
        /// Actual callback frame count.
        actual: u32,
        /// Activated maximum frame count.
        maximum: u32,
    },
    /// The `u32` callback frame count cannot be represented by platform `usize`.
    FrameCountNotRepresentable,
    /// A safe channel view did not match the callback frame count.
    BufferFrameCountMismatch {
        /// Expected sample count for each channel view.
        expected: usize,
        /// Actual sample count exposed by one channel view.
        actual: usize,
    },
    /// Parameter automation did not match the active schema.
    InvalidParameterEvents(ParameterAutomationError),
    /// Parameter events were validated for another block size.
    ParameterEventFrameCountMismatch {
        /// Actual process block size.
        expected: u32,
        /// Block size used to validate the events.
        actual: u32,
    },
    /// The activation event bound cannot be represented by platform `usize`.
    ParameterEventLimitNotRepresentable,
    /// The callback supplied more parameter events than activation allows.
    ParameterEventCountTooLarge {
        /// Actual event count.
        actual: usize,
        /// Activation-owned maximum.
        maximum: u32,
    },
}

impl fmt::Display for ProcessBlockError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BelowGuaranteedMinimum { actual, minimum } => write!(
                formatter,
                "process block has {actual} frames below guaranteed minimum {minimum}"
            ),
            Self::ExceedsActivatedMaximum { actual, maximum } => write!(
                formatter,
                "process block has {actual} frames above activated maximum {maximum}"
            ),
            Self::FrameCountNotRepresentable => {
                formatter.write_str("frame count is not representable on this platform")
            }
            Self::BufferFrameCountMismatch { expected, actual } => write!(
                formatter,
                "channel view has {actual} samples but process block requires {expected}"
            ),
            Self::InvalidParameterEvents(error) => {
                write!(formatter, "invalid parameter events: {error}")
            }
            Self::ParameterEventFrameCountMismatch { expected, actual } => write!(
                formatter,
                "parameter events were validated for {actual} frames but process block has {expected}"
            ),
            Self::ParameterEventLimitNotRepresentable => {
                formatter.write_str("parameter event limit is not representable on this platform")
            }
            Self::ParameterEventCountTooLarge { actual, maximum } => write!(
                formatter,
                "process block has {actual} parameter events but activation allows {maximum}"
            ),
        }
    }
}

impl std::error::Error for ProcessBlockError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidParameterEvents(error) => Some(error),
            Self::BelowGuaranteedMinimum { .. }
            | Self::ExceedsActivatedMaximum { .. }
            | Self::FrameCountNotRepresentable
            | Self::BufferFrameCountMismatch { .. }
            | Self::ParameterEventFrameCountMismatch { .. }
            | Self::ParameterEventLimitNotRepresentable
            | Self::ParameterEventCountTooLarge { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        audio::{DEFAULT_EFFECT_CONFIGURATION, MAIN_INPUT, MAIN_OUTPUT},
        automation::{ParameterEvent, ParameterEventValue},
        buffer::{ChannelBuffer, InputEndpoint, OutputEndpoint},
    };

    fn maximum(value: u32) -> NonZeroU32 {
        NonZeroU32::new(value).expect("test maximum is non-zero")
    }

    #[test]
    fn rejects_invalid_sample_rate() {
        assert_eq!(
            ProcessConfig::new(0.0, None, maximum(512), 0),
            Err(ProcessConfigError::InvalidSampleRate)
        );
    }

    #[test]
    fn rejects_minimum_above_maximum() {
        let minimum = NonZeroU32::new(1024).expect("test minimum is non-zero");

        assert_eq!(
            ProcessConfig::new(48_000.0, Some(minimum), maximum(512), 0),
            Err(ProcessConfigError::InvalidFrameRange)
        );
    }

    #[test]
    fn accepts_unknown_minimum() {
        let config = ProcessConfig::new(48_000.0, None, maximum(2048), 0)
            .expect("test configuration is valid");

        assert!((config.sample_rate() - 48_000.0).abs() <= f64::EPSILON);
        assert_eq!(config.guaranteed_min_frames(), None);
        assert_eq!(config.max_frames(), maximum(2048));
        assert_eq!(config.max_parameter_events(), 0);
    }

    #[test]
    fn process_block_rejects_mismatched_channel_lengths() {
        let process =
            ProcessConfig::new(48_000.0, None, maximum(8), 0).expect("test configuration is valid");
        let activation = ActivationConfig::new(process, DEFAULT_EFFECT_CONFIGURATION);
        let input = [0.0_f32; 3];
        let mut output = [0.0_f32; 3];
        let mut buffers = [ChannelBuffer::separate(
            InputEndpoint::new(MAIN_INPUT, 0),
            &input,
            OutputEndpoint::new(MAIN_OUTPUT, 0),
            &mut output,
            3,
        )
        .expect("test buffers are long enough")];

        let context = ProcessContext::new(
            ProcessMode::Realtime,
            TransportSnapshot::unknown(),
            ParameterEvents::empty_for_block(2),
        );
        assert!(matches!(
            ProcessBlock::new(&activation, 2, context, &mut buffers),
            Err(ProcessBlockError::BufferFrameCountMismatch { .. })
        ));
    }

    #[test]
    fn process_context_carries_optional_transport_and_events() {
        let transport = TransportSnapshot::new(Some(true), Some(false), Some(120.0), Some(12))
            .expect("transport snapshot is valid");
        let context = ProcessContext::new(
            ProcessMode::Offline,
            transport,
            ParameterEvents::empty_for_block(4),
        );

        assert_eq!(context.mode(), ProcessMode::Offline);
        assert_eq!(context.transport().playing(), Some(true));
        assert_eq!(context.transport().recording(), Some(false));
        assert_eq!(context.transport().tempo_bpm(), Some(120.0));
        assert_eq!(context.transport().sample_position(), Some(12));
        assert!(context.parameter_events().is_empty());
        assert_eq!(context.parameter_events().frame_count(), 4);
        assert_eq!(
            TransportSnapshot::new(None, None, Some(0.0), None),
            Err(TransportSnapshotError::InvalidTempo)
        );
    }

    #[test]
    fn process_block_rejects_events_for_another_block_size() {
        let process =
            ProcessConfig::new(48_000.0, None, maximum(8), 0).expect("test configuration is valid");
        let activation = ActivationConfig::new(process, DEFAULT_EFFECT_CONFIGURATION);
        let context = ProcessContext::new(
            ProcessMode::Realtime,
            TransportSnapshot::unknown(),
            ParameterEvents::empty_for_block(1),
        );
        let mut buffers: [ChannelBuffer<'_, f32>; 0] = [];

        assert!(matches!(
            ProcessBlock::new(&activation, 2, context, &mut buffers),
            Err(ProcessBlockError::ParameterEventFrameCountMismatch {
                expected: 2,
                actual: 1,
            })
        ));

        let raw_events = [ParameterEvent::set(
            0,
            "gain",
            ParameterEventValue::Float(0.5),
        )];
        let events = ParameterEvents::new(&raw_events, 2, 1).expect("event shape is valid");
        let context =
            ProcessContext::new(ProcessMode::Realtime, TransportSnapshot::unknown(), events);
        assert!(matches!(
            ProcessBlock::new(&activation, 2, context, &mut buffers),
            Err(ProcessBlockError::ParameterEventCountTooLarge {
                actual: 1,
                maximum: 0,
            })
        ));
    }

    #[test]
    fn process_modes_are_per_call_semantics() {
        assert_ne!(ProcessMode::Realtime, ProcessMode::BufferedRealtime);
        assert_ne!(ProcessMode::BufferedRealtime, ProcessMode::Offline);
    }
}
