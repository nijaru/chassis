#![forbid(unsafe_code)]

//! Format-independent core contracts for Chassis.
//!
//! This crate intentionally contains only types that are meaningful without a
//! particular plugin format, GUI toolkit, or product DSP architecture.

use core::fmt;

/// Stable identifier for an audio port within one component definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PortId(u32);

impl PortId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Direction of an audio port from the component's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PortDirection {
    Input,
    Output,
}

/// Conventional role of an audio port.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PortRole {
    Main,
    Sidechain,
    Auxiliary,
}

/// Semantic identity of one audio channel.
///
/// The list is intentionally extensible. Format adapters are responsible for
/// mapping these identities into their host-specific channel-layout model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Channel {
    Mono,
    Left,
    Right,
    Center,
    Lfe,
    LeftSurround,
    RightSurround,
    LeftRearSurround,
    RightRearSurround,
    TopFrontLeft,
    TopFrontRight,
    TopMiddleLeft,
    TopMiddleRight,
    TopRearLeft,
    TopRearRight,
    /// A discrete channel with no stronger semantic identity.
    Discrete(u16),
}

pub static MONO_CHANNELS: [Channel; 1] = [Channel::Mono];
pub static STEREO_CHANNELS: [Channel; 2] = [Channel::Left, Channel::Right];

/// Ordered semantic channel layout for one audio port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelLayout<'a> {
    channels: &'a [Channel],
}

impl<'a> ChannelLayout<'a> {
    #[must_use]
    pub const fn new(channels: &'a [Channel]) -> Self {
        Self { channels }
    }

    #[must_use]
    pub const fn channels(self) -> &'a [Channel] {
        self.channels
    }

    #[must_use]
    pub const fn len(self) -> usize {
        self.channels.len()
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.channels.is_empty()
    }
}

impl ChannelLayout<'static> {
    pub const MONO: Self = Self::new(&MONO_CHANNELS);
    pub const STEREO: Self = Self::new(&STEREO_CHANNELS);
}

/// Metadata describing an audio input or output port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioPortDescriptor<'a> {
    pub id: PortId,
    pub name: &'a str,
    pub direction: PortDirection,
    pub role: PortRole,
    pub layout: ChannelLayout<'a>,
    pub optional: bool,
}

/// Conventional effect layout used when a product does not specify otherwise.
///
/// Stereo is a convenience here, not a limitation of [`AudioPortDescriptor`]
/// or [`ChannelLayout`].
pub const DEFAULT_EFFECT_PORTS: [AudioPortDescriptor<'static>; 3] = [
    AudioPortDescriptor {
        id: PortId::new(0),
        name: "Main Input",
        direction: PortDirection::Input,
        role: PortRole::Main,
        layout: ChannelLayout::STEREO,
        optional: false,
    },
    AudioPortDescriptor {
        id: PortId::new(1),
        name: "Main Output",
        direction: PortDirection::Output,
        role: PortRole::Main,
        layout: ChannelLayout::STEREO,
        optional: false,
    },
    AudioPortDescriptor {
        id: PortId::new(2),
        name: "Sidechain",
        direction: PortDirection::Input,
        role: PortRole::Sidechain,
        layout: ChannelLayout::STEREO,
        optional: true,
    },
];

/// Whether processing is constrained by realtime delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProcessMode {
    Realtime,
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

    #[must_use]
    pub const fn sample_rate(self) -> f64 {
        self.sample_rate
    }

    #[must_use]
    pub const fn min_frames(self) -> u32 {
        self.min_frames
    }

    #[must_use]
    pub const fn max_frames(self) -> u32 {
        self.max_frames
    }

    #[must_use]
    pub const fn mode(self) -> ProcessMode {
        self.mode
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessConfigError {
    InvalidSampleRate,
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
    fn default_effect_convention_is_stereo_with_optional_sidechain() {
        assert_eq!(DEFAULT_EFFECT_PORTS.len(), 3);
        assert_eq!(DEFAULT_EFFECT_PORTS[0].layout, ChannelLayout::STEREO);
        assert_eq!(DEFAULT_EFFECT_PORTS[1].layout, ChannelLayout::STEREO);
        assert_eq!(DEFAULT_EFFECT_PORTS[2].layout, ChannelLayout::STEREO);
        assert!(!DEFAULT_EFFECT_PORTS[0].optional);
        assert!(!DEFAULT_EFFECT_PORTS[1].optional);
        assert!(DEFAULT_EFFECT_PORTS[2].optional);
    }

    #[test]
    fn arbitrary_layouts_are_not_limited_to_stereo() {
        let channels = [
            Channel::Left,
            Channel::Right,
            Channel::Center,
            Channel::Lfe,
            Channel::LeftSurround,
            Channel::RightSurround,
        ];
        let layout = ChannelLayout::new(&channels);
        assert_eq!(layout.len(), 6);
    }

    #[test]
    fn process_config_rejects_invalid_values() {
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
    fn process_config_accepts_variable_block_sizes() {
        let config = ProcessConfig::new(48_000.0, 1, 2048, ProcessMode::Offline)
            .expect("valid process configuration");
        assert_eq!(config.sample_rate(), 48_000.0);
        assert_eq!(config.min_frames(), 1);
        assert_eq!(config.max_frames(), 2048);
        assert_eq!(config.mode(), ProcessMode::Offline);
    }
}
