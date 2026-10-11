//! Safe, format-independent process-buffer views.
//!
//! Adapters are responsible for proving raw host-pointer aliasing before they
//! construct these types. Once ordinary Rust references exist, this module
//! preserves those proven relationships without additional unsafe code.

use core::fmt;

use crate::audio::AudioPortIndex;

/// Dense process-time address of one input channel within an active audio port.
///
/// Stable [`PortKey`](crate::audio::PortKey) identity is resolved before
/// processing. Callback data carries only this schema-local dense index and the
/// zero-based channel index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InputEndpoint {
    port: AudioPortIndex,
    channel: u32,
}

impl InputEndpoint {
    /// Construct an input endpoint from a setup-resolved dense port index.
    #[must_use]
    pub const fn new(port: AudioPortIndex, channel: u32) -> Self {
        Self { port, channel }
    }

    /// Return the setup-resolved dense port index.
    #[must_use]
    pub const fn port_index(self) -> AudioPortIndex {
        self.port
    }

    /// Return the zero-based channel index.
    #[must_use]
    pub const fn channel(self) -> u32 {
        self.channel
    }
}

/// Dense process-time address of one output channel within an active audio port.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OutputEndpoint {
    port: AudioPortIndex,
    channel: u32,
}

impl OutputEndpoint {
    /// Construct an output endpoint from a setup-resolved dense port index.
    #[must_use]
    pub const fn new(port: AudioPortIndex, channel: u32) -> Self {
        Self { port, channel }
    }

    /// Return the setup-resolved dense port index.
    #[must_use]
    pub const fn port_index(self) -> AudioPortIndex {
        self.port
    }

    /// Return the zero-based channel index.
    #[must_use]
    pub const fn channel(self) -> u32 {
        self.channel
    }
}

/// Proven relationship between the host buffers represented by one channel view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BufferRelationship {
    /// Input and output are the same exact memory range and are represented by one mutable slice.
    InPlace,
    /// Input and output are disjoint memory ranges.
    Separate,
    /// The channel has only an input endpoint.
    InputOnly,
    /// The channel has only an output endpoint.
    OutputOnly,
}

/// Safe process-time view of one channel or one paired input/output channel.
///
/// `InPlace` contains exactly one mutable reference, so product code can never
/// observe simultaneous shared and mutable Rust references to aliased host
/// memory. `Separate` can only be constructed from already-disjoint safe Rust
/// references; any raw-pointer proof needed to create those references belongs
/// in the adapter.
pub enum ChannelBuffer<'a, S> {
    /// Exact in-place input/output alias.
    InPlace {
        /// Semantic input endpoint.
        input: InputEndpoint,
        /// Semantic output endpoint.
        output: OutputEndpoint,
        /// The sole mutable view of the aliased sample range.
        samples: &'a mut [S],
    },
    /// Disjoint input/output sample ranges.
    Separate {
        /// Semantic input endpoint.
        input: InputEndpoint,
        /// Read-only input samples.
        input_samples: &'a [S],
        /// Semantic output endpoint.
        output: OutputEndpoint,
        /// Mutable output samples.
        output_samples: &'a mut [S],
    },
    /// Input channel with no paired output.
    InputOnly {
        /// Semantic input endpoint.
        input: InputEndpoint,
        /// Read-only input samples.
        samples: &'a [S],
    },
    /// Output channel with no paired input.
    OutputOnly {
        /// Semantic output endpoint.
        output: OutputEndpoint,
        /// Mutable output samples.
        samples: &'a mut [S],
    },
}

impl<'a, S> ChannelBuffer<'a, S> {
    /// Construct an exact in-place channel view trimmed to `frame_count` samples.
    ///
    /// # Errors
    ///
    /// Returns [`ChannelBufferError::FrameCountNotRepresentable`] if the frame
    /// count cannot be represented by the current platform's `usize`, or
    /// [`ChannelBufferError::InPlaceBufferTooShort`] if `samples` is shorter
    /// than the requested frame count.
    pub fn in_place(
        input: InputEndpoint,
        output: OutputEndpoint,
        samples: &'a mut [S],
        frame_count: u32,
    ) -> Result<Self, ChannelBufferError> {
        let frames = frame_count_to_usize(frame_count)?;
        if samples.len() < frames {
            return Err(ChannelBufferError::InPlaceBufferTooShort {
                available: samples.len(),
                required: frames,
            });
        }

        Ok(Self::InPlace {
            input,
            output,
            samples: &mut samples[..frames],
        })
    }

    /// Construct a disjoint input/output channel view trimmed to `frame_count` samples.
    ///
    /// Safe Rust borrowing guarantees that the supplied shared input reference
    /// and mutable output reference do not overlap. Adapters creating those
    /// references from host pointers must prove that fact before this call.
    ///
    /// # Errors
    ///
    /// Returns a [`ChannelBufferError`] when the frame count is not representable
    /// or either sample slice is shorter than requested.
    pub fn separate(
        input: InputEndpoint,
        input_samples: &'a [S],
        output: OutputEndpoint,
        output_samples: &'a mut [S],
        frame_count: u32,
    ) -> Result<Self, ChannelBufferError> {
        let frames = frame_count_to_usize(frame_count)?;
        if input_samples.len() < frames {
            return Err(ChannelBufferError::InputBufferTooShort {
                available: input_samples.len(),
                required: frames,
            });
        }
        if output_samples.len() < frames {
            return Err(ChannelBufferError::OutputBufferTooShort {
                available: output_samples.len(),
                required: frames,
            });
        }

        Ok(Self::Separate {
            input,
            input_samples: &input_samples[..frames],
            output,
            output_samples: &mut output_samples[..frames],
        })
    }

    /// Construct an input-only channel view trimmed to `frame_count` samples.
    ///
    /// # Errors
    ///
    /// Returns a [`ChannelBufferError`] when the frame count is not representable
    /// or the sample slice is shorter than requested.
    pub fn input_only(
        input: InputEndpoint,
        samples: &'a [S],
        frame_count: u32,
    ) -> Result<Self, ChannelBufferError> {
        let frames = frame_count_to_usize(frame_count)?;
        if samples.len() < frames {
            return Err(ChannelBufferError::InputBufferTooShort {
                available: samples.len(),
                required: frames,
            });
        }

        Ok(Self::InputOnly {
            input,
            samples: &samples[..frames],
        })
    }

    /// Construct an output-only channel view trimmed to `frame_count` samples.
    ///
    /// # Errors
    ///
    /// Returns a [`ChannelBufferError`] when the frame count is not representable
    /// or the sample slice is shorter than requested.
    pub fn output_only(
        output: OutputEndpoint,
        samples: &'a mut [S],
        frame_count: u32,
    ) -> Result<Self, ChannelBufferError> {
        let frames = frame_count_to_usize(frame_count)?;
        if samples.len() < frames {
            return Err(ChannelBufferError::OutputBufferTooShort {
                available: samples.len(),
                required: frames,
            });
        }

        Ok(Self::OutputOnly {
            output,
            samples: &mut samples[..frames],
        })
    }

    /// Return the proven buffer relationship.
    #[must_use]
    pub const fn relationship(&self) -> BufferRelationship {
        match self {
            Self::InPlace { .. } => BufferRelationship::InPlace,
            Self::Separate { .. } => BufferRelationship::Separate,
            Self::InputOnly { .. } => BufferRelationship::InputOnly,
            Self::OutputOnly { .. } => BufferRelationship::OutputOnly,
        }
    }

    /// Return the input endpoint when one exists.
    #[must_use]
    pub const fn input_endpoint(&self) -> Option<InputEndpoint> {
        match self {
            Self::InPlace { input, .. }
            | Self::Separate { input, .. }
            | Self::InputOnly { input, .. } => Some(*input),
            Self::OutputOnly { .. } => None,
        }
    }

    /// Return the output endpoint when one exists.
    #[must_use]
    pub const fn output_endpoint(&self) -> Option<OutputEndpoint> {
        match self {
            Self::InPlace { output, .. }
            | Self::Separate { output, .. }
            | Self::OutputOnly { output, .. } => Some(*output),
            Self::InputOnly { .. } => None,
        }
    }

    /// Return the number of samples exposed by this channel view.
    #[must_use]
    pub fn frame_count(&self) -> usize {
        match self {
            Self::Separate { input_samples, .. } => input_samples.len(),
            Self::InputOnly { samples, .. } => samples.len(),
            Self::InPlace { samples, .. } | Self::OutputOnly { samples, .. } => samples.len(),
        }
    }

    /// Borrow readable input samples when an input endpoint exists.
    #[must_use]
    pub fn input(&self) -> Option<&[S]> {
        match self {
            Self::InPlace { samples, .. } => Some(&**samples),
            Self::Separate { input_samples, .. } => Some(*input_samples),
            Self::InputOnly { samples, .. } => Some(*samples),
            Self::OutputOnly { .. } => None,
        }
    }

    /// Borrow writable output samples when an output endpoint exists.
    #[must_use]
    pub fn output_mut(&mut self) -> Option<&mut [S]> {
        match self {
            Self::InPlace { samples, .. } | Self::OutputOnly { samples, .. } => {
                Some(&mut **samples)
            }
            Self::Separate { output_samples, .. } => Some(&mut **output_samples),
            Self::InputOnly { .. } => None,
        }
    }

    /// Return a writable in-place processing view, copying separate input to output once.
    ///
    /// Exact in-place buffers are returned directly. Separate buffers perform
    /// one bounded `copy_from_slice` into the output. Input-only and output-only
    /// channels do not define an input-to-output transform and are rejected.
    ///
    /// # Errors
    ///
    /// Returns [`BufferAccessError::MissingOutput`] for input-only channels or
    /// [`BufferAccessError::MissingInput`] for output-only channels, or
    /// [`BufferAccessError::FrameCountMismatch`] for directly constructed separate
    /// buffers with unequal lengths.
    pub fn make_in_place(&mut self) -> Result<&mut [S], BufferAccessError>
    where
        S: Copy,
    {
        match self {
            Self::InPlace { samples, .. } => Ok(&mut **samples),
            Self::Separate {
                input_samples,
                output_samples,
                ..
            } => {
                if input_samples.len() != output_samples.len() {
                    return Err(BufferAccessError::FrameCountMismatch {
                        input: input_samples.len(),
                        output: output_samples.len(),
                    });
                }
                output_samples.copy_from_slice(input_samples);
                Ok(&mut **output_samples)
            }
            Self::InputOnly { .. } => Err(BufferAccessError::MissingOutput),
            Self::OutputOnly { .. } => Err(BufferAccessError::MissingInput),
        }
    }
}

/// Invalid safe channel-buffer construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelBufferError {
    /// The requested `u32` frame count cannot be represented by platform `usize`.
    FrameCountNotRepresentable,
    /// Exact in-place storage was shorter than requested.
    InPlaceBufferTooShort {
        /// Available sample count.
        available: usize,
        /// Required sample count.
        required: usize,
    },
    /// Input storage was shorter than requested.
    InputBufferTooShort {
        /// Available sample count.
        available: usize,
        /// Required sample count.
        required: usize,
    },
    /// Output storage was shorter than requested.
    OutputBufferTooShort {
        /// Available sample count.
        available: usize,
        /// Required sample count.
        required: usize,
    },
}

impl fmt::Display for ChannelBufferError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FrameCountNotRepresentable => {
                formatter.write_str("frame count is not representable on this platform")
            }
            Self::InPlaceBufferTooShort {
                available,
                required,
            } => write!(
                formatter,
                "in-place buffer has {available} samples but {required} are required"
            ),
            Self::InputBufferTooShort {
                available,
                required,
            } => write!(
                formatter,
                "input buffer has {available} samples but {required} are required"
            ),
            Self::OutputBufferTooShort {
                available,
                required,
            } => write!(
                formatter,
                "output buffer has {available} samples but {required} are required"
            ),
        }
    }
}

impl std::error::Error for ChannelBufferError {}

/// Invalid convenience access for a channel relationship.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferAccessError {
    /// Separate input and output slices have different lengths.
    FrameCountMismatch {
        /// Input sample count.
        input: usize,
        /// Output sample count.
        output: usize,
    },
    /// An input-only channel has no writable output.
    MissingOutput,
    /// An output-only channel has no readable input to copy/process in place.
    MissingInput,
}

impl fmt::Display for BufferAccessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FrameCountMismatch { input, output } => write!(
                formatter,
                "input buffer has {input} samples but output buffer has {output}"
            ),
            Self::MissingOutput => formatter.write_str("channel has no output buffer"),
            Self::MissingInput => formatter.write_str("channel has no input buffer"),
        }
    }
}

impl std::error::Error for BufferAccessError {}

fn frame_count_to_usize(frame_count: u32) -> Result<usize, ChannelBufferError> {
    usize::try_from(frame_count).map_err(|_| ChannelBufferError::FrameCountNotRepresentable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_alias_uses_one_mutable_slice() {
        let mut samples = [1.0_f32, 2.0, 3.0];
        {
            let mut buffer = ChannelBuffer::in_place(
                InputEndpoint::new(AudioPortIndex::new(0), 0),
                OutputEndpoint::new(AudioPortIndex::new(1), 0),
                &mut samples,
                3,
            )
            .expect("test buffer is long enough");

            assert_eq!(buffer.relationship(), BufferRelationship::InPlace);
            buffer
                .make_in_place()
                .expect("paired buffer has input and output")[1] = 4.0;
        }

        assert!((samples[1] - 4.0).abs() <= f32::EPSILON);
    }

    #[test]
    fn separate_make_in_place_copies_once_to_output() {
        let input = [1_u32, 2, 3];
        let mut output = [0_u32; 3];
        let mut buffer = ChannelBuffer::separate(
            InputEndpoint::new(AudioPortIndex::new(0), 0),
            &input,
            OutputEndpoint::new(AudioPortIndex::new(1), 0),
            &mut output,
            3,
        )
        .expect("test buffers are long enough");

        let output_view = buffer
            .make_in_place()
            .expect("paired buffer has input and output");
        assert_eq!(output_view, input);
    }

    #[test]
    fn direct_separate_rejects_unequal_lengths_without_copying() {
        let input = [1_u32, 2];
        let mut output = [9_u32];
        let mut buffer = ChannelBuffer::Separate {
            input: InputEndpoint::new(AudioPortIndex::new(0), 0),
            input_samples: &input,
            output: OutputEndpoint::new(AudioPortIndex::new(1), 0),
            output_samples: &mut output,
        };
        assert!(buffer.make_in_place().is_err());
        assert_eq!(output, [9]);
    }

    #[test]
    fn constructors_reject_short_storage() {
        let input = [1_u32; 2];
        let mut output = [0_u32; 1];

        assert!(matches!(
            ChannelBuffer::separate(
                InputEndpoint::new(AudioPortIndex::new(0), 0),
                &input,
                OutputEndpoint::new(AudioPortIndex::new(1), 0),
                &mut output,
                2,
            ),
            Err(ChannelBufferError::OutputBufferTooShort { .. })
        ));
    }

    #[test]
    fn resolved_endpoints_retain_dense_schema_indices() {
        let input = InputEndpoint::new(AudioPortIndex::new(0), 1);
        let output = OutputEndpoint::new(AudioPortIndex::new(1), 1);
        assert_eq!(input.port_index(), AudioPortIndex::new(0));
        assert_eq!(output.port_index(), AudioPortIndex::new(1));
        assert_eq!(input.channel(), 1);
        assert_eq!(output.channel(), 1);
    }
}
