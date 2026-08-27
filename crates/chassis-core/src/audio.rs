//! Format-independent audio port and layout contracts.

use core::{fmt, num::NonZeroU32};

/// Stable author-facing identity for an audio port.
///
/// Port keys are product compatibility identifiers. Backend numeric IDs and
/// activation-time dense indices are derived separately and must not replace
/// this identity in persisted product metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PortKey(&'static str);

impl PortKey {
    /// Construct a port key from static product metadata.
    #[must_use]
    pub const fn new(value: &'static str) -> Self {
        Self(value)
    }

    /// Return the canonical string form.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for PortKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

/// Direction of an audio port from the component's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PortDirection {
    /// Audio enters the component through this port.
    Input,
    /// Audio leaves the component through this port.
    Output,
}

/// Conventional semantic role of an audio port.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PortRole {
    /// Primary product input or output.
    Main,
    /// Detector/control sidechain input.
    Sidechain,
    /// Additional product input or output.
    Auxiliary,
}

/// Non-zero number of audio channels.
///
/// A 32-bit domain avoids imposing a smaller arbitrary framework limit than
/// the target plugin APIs. Actual memory/work remains bounded by the accepted
/// activation configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChannelCount(NonZeroU32);

impl ChannelCount {
    /// Construct a non-zero channel count.
    #[must_use]
    pub const fn new(value: u32) -> Option<Self> {
        match NonZeroU32::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Return the channel count.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

/// Semantic channel layout for one active audio port.
///
/// Only the first implementation families are represented here. Labeled
/// surround and ambisonic layouts will be added after their cross-format
/// semantics are settled. `Discrete` carries no speaker-position meaning.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelLayout {
    /// One unlabeled/centered channel.
    Mono,
    /// Conventional left/right stereo.
    Stereo,
    /// N channels with no stronger speaker-position semantics.
    Discrete(ChannelCount),
}

impl ChannelLayout {
    /// Return the number of channels represented by this layout.
    #[must_use]
    pub const fn channel_count(self) -> u32 {
        match self {
            Self::Mono => 1,
            Self::Stereo => 2,
            Self::Discrete(channels) => channels.get(),
        }
    }
}

/// Stable metadata for an audio port, independent of its active layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioPortDescriptor {
    /// Persistent product key.
    pub key: PortKey,
    /// Human-readable display name. This is not persistent identity.
    pub name: &'static str,
    /// Input/output direction.
    pub direction: PortDirection,
    /// Conventional semantic role.
    pub role: PortRole,
    /// Whether the component can operate while this port is inactive.
    pub optional: bool,
}

/// One active audio port within a whole-component I/O configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfiguredAudioPort {
    /// Stable key of the active port.
    pub key: PortKey,
    /// Accepted semantic layout.
    pub layout: ChannelLayout,
}

/// Coherent active audio I/O configuration for a component.
///
/// Optional ports that are inactive are omitted. Configuration policy beyond
/// structural validity is evaluated separately while processing is inactive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioIoConfiguration<'a> {
    ports: &'a [ConfiguredAudioPort],
}

impl<'a> AudioIoConfiguration<'a> {
    /// Construct a borrowed configuration view.
    #[must_use]
    pub const fn new(ports: &'a [ConfiguredAudioPort]) -> Self {
        Self { ports }
    }

    /// Return all active ports.
    #[must_use]
    pub const fn ports(self) -> &'a [ConfiguredAudioPort] {
        self.ports
    }

    /// Validate structural invariants against the component's stable schema.
    ///
    /// This performs no allocation. Product-specific whole-configuration
    /// policy is a separate inactive/control-domain step.
    ///
    /// # Errors
    ///
    /// Returns [`AudioIoConfigurationError`] for invalid schema keys, duplicate
    /// descriptors/active ports, unknown active ports, or missing required
    /// ports.
    pub fn validate(
        self,
        descriptors: &[AudioPortDescriptor],
    ) -> Result<(), AudioIoConfigurationError> {
        for (index, descriptor) in descriptors.iter().enumerate() {
            if descriptor.key.as_str().is_empty() {
                return Err(AudioIoConfigurationError::EmptyDescriptorKey);
            }

            if descriptors[..index]
                .iter()
                .any(|previous| previous.key == descriptor.key)
            {
                return Err(AudioIoConfigurationError::DuplicateDescriptorPort(
                    descriptor.key,
                ));
            }
        }

        for (index, configured) in self.ports.iter().enumerate() {
            if !descriptors
                .iter()
                .any(|descriptor| descriptor.key == configured.key)
            {
                return Err(AudioIoConfigurationError::UnknownConfiguredPort(
                    configured.key,
                ));
            }

            if self.ports[..index]
                .iter()
                .any(|previous| previous.key == configured.key)
            {
                return Err(AudioIoConfigurationError::DuplicateConfiguredPort(
                    configured.key,
                ));
            }
        }

        for descriptor in descriptors.iter().filter(|descriptor| !descriptor.optional) {
            if !self.ports.iter().any(|port| port.key == descriptor.key) {
                return Err(AudioIoConfigurationError::MissingRequiredPort(
                    descriptor.key,
                ));
            }
        }

        Ok(())
    }
}

/// Structural audio-I/O validation error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioIoConfigurationError {
    /// A descriptor used an empty compatibility key.
    EmptyDescriptorKey,
    /// Two descriptors declared the same stable key.
    DuplicateDescriptorPort(PortKey),
    /// The active configuration refers to no declared port.
    UnknownConfiguredPort(PortKey),
    /// The active configuration contains the same port more than once.
    DuplicateConfiguredPort(PortKey),
    /// A non-optional descriptor is absent from the active configuration.
    MissingRequiredPort(PortKey),
}

impl fmt::Display for AudioIoConfigurationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyDescriptorKey => formatter.write_str("audio port key must not be empty"),
            Self::DuplicateDescriptorPort(key) => {
                write!(formatter, "audio port descriptor key {key:?} is duplicated")
            }
            Self::UnknownConfiguredPort(key) => {
                write!(formatter, "configured audio port key {key:?} is unknown")
            }
            Self::DuplicateConfiguredPort(key) => {
                write!(formatter, "configured audio port key {key:?} is duplicated")
            }
            Self::MissingRequiredPort(key) => {
                write!(formatter, "required audio port key {key:?} is not active")
            }
        }
    }
}

impl std::error::Error for AudioIoConfigurationError {}

/// Canonical main-input key for the default effect convention.
pub const MAIN_INPUT: PortKey = PortKey::new("audio.main.in");
/// Canonical main-output key for the default effect convention.
pub const MAIN_OUTPUT: PortKey = PortKey::new("audio.main.out");
/// Canonical sidechain key for the default effect convention.
pub const SIDECHAIN_INPUT: PortKey = PortKey::new("audio.sidechain");

/// Conventional port schema for an effect that does not specify otherwise.
pub const DEFAULT_EFFECT_PORTS: [AudioPortDescriptor; 3] = [
    AudioPortDescriptor {
        key: MAIN_INPUT,
        name: "Main Input",
        direction: PortDirection::Input,
        role: PortRole::Main,
        optional: false,
    },
    AudioPortDescriptor {
        key: MAIN_OUTPUT,
        name: "Main Output",
        direction: PortDirection::Output,
        role: PortRole::Main,
        optional: false,
    },
    AudioPortDescriptor {
        key: SIDECHAIN_INPUT,
        name: "Sidechain",
        direction: PortDirection::Input,
        role: PortRole::Sidechain,
        optional: true,
    },
];

/// Default active effect configuration: stereo main input and output.
pub static DEFAULT_EFFECT_CONFIGURATION_PORTS: [ConfiguredAudioPort; 2] = [
    ConfiguredAudioPort {
        key: MAIN_INPUT,
        layout: ChannelLayout::Stereo,
    },
    ConfiguredAudioPort {
        key: MAIN_OUTPUT,
        layout: ChannelLayout::Stereo,
    },
];

/// Borrowed default effect configuration.
pub const DEFAULT_EFFECT_CONFIGURATION: AudioIoConfiguration<'static> =
    AudioIoConfiguration::new(&DEFAULT_EFFECT_CONFIGURATION_PORTS);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_effect_convention_is_stereo_with_optional_sidechain() {
        assert_eq!(DEFAULT_EFFECT_PORTS.len(), 3);
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
    fn discrete_layouts_use_the_full_u32_domain_type() {
        let channels = ChannelCount::new(100_000).expect("non-zero channel count");
        assert_eq!(ChannelLayout::Discrete(channels).channel_count(), 100_000);
    }

    #[test]
    fn configuration_rejects_unknown_ports() {
        const UNKNOWN: PortKey = PortKey::new("audio.unknown");
        let ports = [ConfiguredAudioPort {
            key: UNKNOWN,
            layout: ChannelLayout::Stereo,
        }];

        assert_eq!(
            AudioIoConfiguration::new(&ports).validate(&DEFAULT_EFFECT_PORTS),
            Err(AudioIoConfigurationError::UnknownConfiguredPort(UNKNOWN))
        );
    }

    #[test]
    fn configuration_requires_non_optional_ports() {
        let ports = [ConfiguredAudioPort {
            key: MAIN_INPUT,
            layout: ChannelLayout::Stereo,
        }];

        assert_eq!(
            AudioIoConfiguration::new(&ports).validate(&DEFAULT_EFFECT_PORTS),
            Err(AudioIoConfigurationError::MissingRequiredPort(MAIN_OUTPUT))
        );
    }

    #[test]
    fn configuration_rejects_duplicate_active_ports() {
        let ports = [
            ConfiguredAudioPort {
                key: MAIN_INPUT,
                layout: ChannelLayout::Stereo,
            },
            ConfiguredAudioPort {
                key: MAIN_INPUT,
                layout: ChannelLayout::Stereo,
            },
        ];

        assert_eq!(
            AudioIoConfiguration::new(&ports).validate(&DEFAULT_EFFECT_PORTS),
            Err(AudioIoConfigurationError::DuplicateConfiguredPort(
                MAIN_INPUT
            ))
        );
    }
}
