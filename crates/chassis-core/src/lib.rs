#![forbid(unsafe_code)]

//! Format-independent core contracts for Chassis.
//!
//! This crate intentionally contains only types that are meaningful without a
//! particular plugin format, GUI toolkit, or product DSP architecture.

use core::{fmt, num::NonZeroU16};

/// Stable identifier for an audio port within one component definition.
///
/// Chassis port IDs are unique across both input and output ports. Format
/// adapters may translate them when a backend uses separate input/output ID
/// namespaces.
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

/// Non-zero number of audio channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChannelCount(NonZeroU16);

impl ChannelCount {
    #[must_use]
    pub const fn new(value: u16) -> Option<Self> {
        let Some(value) = NonZeroU16::new(value) else {
            return None;
        };
        Some(Self(value))
    }

    #[must_use]
    pub const fn get(self) -> u16 {
        self.0.get()
    }
}

/// Semantic channel layout for one active audio port.
///
/// Only the first production layout families are represented here. Surround,
/// ambisonic, and richer speaker layouts will be added only after their
/// format-independent semantics are settled. `Discrete` means that only the
/// number of channels is known; adapters must not silently treat it as a
/// labeled surround layout.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelLayout {
    Mono,
    Stereo,
    Discrete(ChannelCount),
}

impl ChannelLayout {
    #[must_use]
    pub const fn channel_count(&self) -> u16 {
        match self {
            Self::Mono => 1,
            Self::Stereo => 2,
            Self::Discrete(channels) => channels.get(),
        }
    }
}

/// Stable metadata for an audio port independent of its active layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioPortDescriptor<'a> {
    pub id: PortId,
    pub name: &'a str,
    pub direction: PortDirection,
    pub role: PortRole,
    /// Whether the component can operate without this port active.
    pub optional: bool,
}

/// One active port within an I/O configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfiguredAudioPort {
    pub id: PortId,
    pub layout: ChannelLayout,
}

/// Coherent active audio I/O configuration for a component.
///
/// Configuration changes occur while processing is inactive and are validated
/// as a whole. Optional ports that are not active are omitted from `ports`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioIoConfiguration<'a> {
    ports: &'a [ConfiguredAudioPort],
}

impl<'a> AudioIoConfiguration<'a> {
    #[must_use]
    pub const fn new(ports: &'a [ConfiguredAudioPort]) -> Self {
        Self { ports }
    }

    #[must_use]
    pub const fn ports(self) -> &'a [ConfiguredAudioPort] {
        self.ports
    }

    /// Validate this active configuration against the component's stable port
    /// descriptors.
    ///
    /// Validation is allocation-free. The expected number of plugin ports is
    /// small, so the current implementation intentionally uses simple scans
    /// rather than introducing a map/set dependency into the core contract.
    ///
    /// # Errors
    ///
    /// Returns [`AudioIoConfigurationError`] when descriptor IDs are not
    /// unique, an active port is unknown or duplicated, or a required port is
    /// absent from the active configuration.
    pub fn validate(
        self,
        descriptors: &[AudioPortDescriptor<'_>],
    ) -> Result<(), AudioIoConfigurationError> {
        for (index, descriptor) in descriptors.iter().enumerate() {
            if descriptors[..index]
                .iter()
                .any(|previous| previous.id == descriptor.id)
            {
                return Err(AudioIoConfigurationError::DuplicateDescriptorPort(
                    descriptor.id,
                ));
            }
        }

        for (index, configured) in self.ports.iter().enumerate() {
            if !descriptors
                .iter()
                .any(|descriptor| descriptor.id == configured.id)
            {
                return Err(AudioIoConfigurationError::UnknownConfiguredPort(
                    configured.id,
                ));
            }

            if self.ports[..index]
                .iter()
                .any(|previous| previous.id == configured.id)
            {
                return Err(AudioIoConfigurationError::DuplicateConfiguredPort(
                    configured.id,
                ));
            }
        }

        for descriptor in descriptors.iter().filter(|descriptor| !descriptor.optional) {
            if !self.ports.iter().any(|port| port.id == descriptor.id) {
                return Err(AudioIoConfigurationError::MissingRequiredPort(
                    descriptor.id,
                ));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioIoConfigurationError {
    DuplicateDescriptorPort(PortId),
    UnknownConfiguredPort(PortId),
    DuplicateConfiguredPort(PortId),
    MissingRequiredPort(PortId),
}

impl fmt::Display for AudioIoConfigurationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateDescriptorPort(id) => {
                write!(formatter, "audio port descriptor ID {} is duplicated", id.get())
            }
            Self::UnknownConfiguredPort(id) => {
                write!(formatter, "configured audio port ID {} is unknown", id.get())
            }
            Self::DuplicateConfiguredPort(id) => {
                write!(formatter, "configured audio port ID {} is duplicated", id.get())
            }
            Self::MissingRequiredPort(id) => {
                write!(formatter, "required audio port ID {} is not active", id.get())
            }
        }
    }
}

impl std::error::Error for AudioIoConfigurationError {}

/// Conventional ports for an effect that does not specify otherwise.
pub const DEFAULT_EFFECT_PORTS: [AudioPortDescriptor<'static>; 3] = [
    AudioPortDescriptor {
        id: PortId::new(0),
        name: "Main Input",
        direction: PortDirection::Input,
        role: PortRole::Main,
        optional: false,
    },
    AudioPortDescriptor {
        id: PortId::new(1),
        name: "Main Output",
        direction: PortDirection::Output,
        role: PortRole::Main,
        optional: false,
    },
    AudioPortDescriptor {
        id: PortId::new(2),
        name: "Sidechain",
        direction: PortDirection::Input,
        role: PortRole::Sidechain,
        optional: true,
    },
];

/// Default active effect configuration: stereo main input and output.
///
/// The conventional sidechain port exists but is inactive until requested.
pub static DEFAULT_EFFECT_CONFIGURATION_PORTS: [ConfiguredAudioPort; 2] = [
    ConfiguredAudioPort {
        id: PortId::new(0),
        layout: ChannelLayout::Stereo,
    },
    ConfiguredAudioPort {
        id: PortId::new(1),
        layout: ChannelLayout::Stereo,
    },
];

pub const DEFAULT_EFFECT_CONFIGURATION: AudioIoConfiguration<'static> =
    AudioIoConfiguration::new(&DEFAULT_EFFECT_CONFIGURATION_PORTS);

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
    /// Construct a processing configuration.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessConfigError::InvalidSampleRate`] if `sample_rate` is
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
        assert!(!DEFAULT_EFFECT_PORTS[0].optional);
        assert!(!DEFAULT_EFFECT_PORTS[1].optional);
        assert!(DEFAULT_EFFECT_PORTS[2].optional);

        let configured = DEFAULT_EFFECT_CONFIGURATION.ports();
        assert_eq!(configured.len(), 2);
        assert_eq!(configured[0].layout, ChannelLayout::Stereo);
        assert_eq!(configured[1].layout, ChannelLayout::Stereo);
        assert_eq!(
            DEFAULT_EFFECT_CONFIGURATION.validate(&DEFAULT_EFFECT_PORTS),
            Ok(())
        );
    }

    #[test]
    fn discrete_layouts_are_not_limited_to_stereo() {
        let channels = ChannelCount::new(12).expect("non-zero channel count");
        let layout = ChannelLayout::Discrete(channels);
        assert_eq!(layout.channel_count(), 12);
    }

    #[test]
    fn configuration_rejects_unknown_ports() {
        let ports = [ConfiguredAudioPort {
            id: PortId::new(99),
            layout: ChannelLayout::Stereo,
        }];
        let configuration = AudioIoConfiguration::new(&ports);

        assert_eq!(
            configuration.validate(&DEFAULT_EFFECT_PORTS),
            Err(AudioIoConfigurationError::UnknownConfiguredPort(
                PortId::new(99)
            ))
        );
    }

    #[test]
    fn configuration_requires_non_optional_ports() {
        let ports = [ConfiguredAudioPort {
            id: PortId::new(0),
            layout: ChannelLayout::Stereo,
        }];
        let configuration = AudioIoConfiguration::new(&ports);

        assert_eq!(
            configuration.validate(&DEFAULT_EFFECT_PORTS),
            Err(AudioIoConfigurationError::MissingRequiredPort(PortId::new(1)))
        );
    }

    #[test]
    fn configuration_rejects_duplicate_active_ports() {
        let ports = [
            ConfiguredAudioPort {
                id: PortId::new(0),
                layout: ChannelLayout::Stereo,
            },
            ConfiguredAudioPort {
                id: PortId::new(0),
                layout: ChannelLayout::Stereo,
            },
        ];
        let configuration = AudioIoConfiguration::new(&ports);

        assert_eq!(
            configuration.validate(&DEFAULT_EFFECT_PORTS),
            Err(AudioIoConfigurationError::DuplicateConfiguredPort(
                PortId::new(0)
            ))
        );
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
