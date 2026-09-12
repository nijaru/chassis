//! Format-independent audio port and layout contracts.

use core::{fmt, num::NonZeroU32};
use std::{borrow::Cow, string::String, vec::Vec};

/// Stable author-facing identity for an audio port.
///
/// Port keys are product compatibility identifiers. Backend numeric IDs and
/// activation-time dense indices are derived separately and must not replace
/// this identity in persisted product metadata.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PortKey(Cow<'static, str>);

impl PortKey {
    /// Construct a zero-allocation port key from static product metadata.
    #[must_use]
    pub const fn new(value: &'static str) -> Self {
        Self(Cow::Borrowed(value))
    }

    /// Construct a port key owned by setup/control-domain metadata.
    #[must_use]
    pub fn owned(value: impl Into<String>) -> Self {
        Self(Cow::Owned(value.into()))
    }

    /// Return the canonical string form.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

impl AsRef<str> for PortKey {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for PortKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Dense schema-local identity for an audio port.
///
/// An `AudioPortIndex` is meaningful only relative to one validated immutable
/// audio-port schema. It is never persistent identity and must not be serialized
/// or treated as a backend port ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AudioPortIndex(u32);

impl AudioPortIndex {
    /// Construct an audio-port index from validated setup mapping.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Return the dense numeric index.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioPortDescriptor {
    /// Persistent product key.
    pub key: PortKey,
    /// Human-readable display name. This is not persistent identity.
    pub name: Cow<'static, str>,
    /// Input/output direction.
    pub direction: PortDirection,
    /// Conventional semantic role.
    pub role: PortRole,
    /// Whether the component can operate while this port is inactive.
    pub optional: bool,
}

impl AudioPortDescriptor {
    /// Construct static product metadata without allocating.
    #[must_use]
    pub const fn new(
        key: PortKey,
        name: &'static str,
        direction: PortDirection,
        role: PortRole,
        optional: bool,
    ) -> Self {
        Self {
            key,
            name: Cow::Borrowed(name),
            direction,
            role,
            optional,
        }
    }

    /// Construct runtime-owned port metadata for hosted/dynamic components.
    #[must_use]
    pub fn owned(
        key: impl Into<String>,
        name: impl Into<String>,
        direction: PortDirection,
        role: PortRole,
        optional: bool,
    ) -> Self {
        Self {
            key: PortKey::owned(key),
            name: Cow::Owned(name.into()),
            direction,
            role,
            optional,
        }
    }
}

/// Validate the immutable audio-port schema without requiring an active layout.
///
/// # Errors
///
/// Returns [`AudioIoConfigurationError::EmptyDescriptorKey`] when a stable key
/// is empty or [`AudioIoConfigurationError::DuplicateDescriptorPort`] when a
/// key is repeated.
pub fn validate_audio_port_schema(
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
                descriptor.key.clone(),
            ));
        }
    }
    Ok(())
}

/// Resolve a stable audio-port key to its dense schema-local index.
#[must_use]
pub fn audio_port_index(
    descriptors: &[AudioPortDescriptor],
    key: &PortKey,
) -> Option<AudioPortIndex> {
    descriptors
        .iter()
        .position(|descriptor| &descriptor.key == key)
        .and_then(|index| u32::try_from(index).ok())
        .map(AudioPortIndex::new)
}

/// One requested active audio port within a whole-component I/O configuration.
///
/// This setup-facing form uses stable identity because hosts/applications select
/// layouts before an activation has resolved the component schema to dense indices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfiguredAudioPort {
    /// Stable key of the active port.
    pub key: PortKey,
    /// Accepted semantic layout.
    pub layout: ChannelLayout,
}

impl ConfiguredAudioPort {
    /// Construct a setup-facing active port selection.
    #[must_use]
    pub const fn new(key: PortKey, layout: ChannelLayout) -> Self {
        Self { key, layout }
    }
}

/// One owned allowed whole-component audio configuration.
///
/// Policy specifications live entirely in setup/control-domain metadata. Stable
/// keys are resolved to dense indices only after one proposed configuration is
/// accepted for activation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioIoConfigurationSpec {
    ports: Vec<ConfiguredAudioPort>,
}

impl AudioIoConfigurationSpec {
    /// Construct one allowed whole-component configuration.
    #[must_use]
    pub fn new(ports: Vec<ConfiguredAudioPort>) -> Self {
        Self { ports }
    }

    /// Return the active ports required by this configuration.
    #[must_use]
    pub fn ports(&self) -> &[ConfiguredAudioPort] {
        &self.ports
    }

    fn matches(&self, configuration: AudioIoConfiguration<'_>) -> bool {
        same_audio_configuration(&self.ports, configuration.ports())
    }
}

/// Whole-component policy for accepted audio I/O configurations.
///
/// Structural port validity and product configuration policy are distinct:
/// descriptors say which ports exist, while this policy says which complete
/// active-port/layout combinations are semantically supported.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioIoPolicy {
    /// Accept every structurally valid configuration described by the port schema.
    ///
    /// This must be chosen explicitly for dynamic/embedded components whose
    /// product semantics genuinely impose no stronger cross-port constraints.
    AnyStructurallyValid,
    /// Accept exactly one of the listed whole-component configurations.
    Enumerated(Vec<AudioIoConfigurationSpec>),
}

impl AudioIoPolicy {
    /// Explicitly accept any structurally valid configuration.
    #[must_use]
    pub const fn any_structurally_valid() -> Self {
        Self::AnyStructurallyValid
    }

    /// Accept exactly one of the supplied whole-component configurations.
    #[must_use]
    pub fn enumerated(configurations: Vec<AudioIoConfigurationSpec>) -> Self {
        Self::Enumerated(configurations)
    }

    /// Conventional effect policy: stereo main input/output, with the stereo
    /// sidechain either inactive or active.
    #[must_use]
    pub fn stereo_effect() -> Self {
        let main = DEFAULT_EFFECT_CONFIGURATION_PORTS.to_vec();
        let mut with_sidechain = main.clone();
        with_sidechain.push(ConfiguredAudioPort::new(
            SIDECHAIN_INPUT,
            ChannelLayout::Stereo,
        ));
        Self::Enumerated(vec![
            AudioIoConfigurationSpec::new(main),
            AudioIoConfigurationSpec::new(with_sidechain),
        ])
    }

    /// Return enumerated accepted configurations when this is a finite policy.
    #[must_use]
    pub fn configurations(&self) -> Option<&[AudioIoConfigurationSpec]> {
        match self {
            Self::AnyStructurallyValid => None,
            Self::Enumerated(configurations) => Some(configurations),
        }
    }

    /// Return whether one already structurally valid proposal is accepted.
    #[must_use]
    pub fn accepts(&self, configuration: AudioIoConfiguration<'_>) -> bool {
        match self {
            Self::AnyStructurallyValid => true,
            Self::Enumerated(configurations) => configurations
                .iter()
                .any(|candidate| candidate.matches(configuration)),
        }
    }

    pub(crate) fn validate_for_ports(
        &self,
        descriptors: &[AudioPortDescriptor],
    ) -> Result<(), AudioIoPolicyError> {
        let Self::Enumerated(configurations) = self else {
            return Ok(());
        };
        if configurations.is_empty() {
            return Err(AudioIoPolicyError::NoAllowedConfigurations);
        }
        for (index, configuration) in configurations.iter().enumerate() {
            AudioIoConfiguration::new(configuration.ports())
                .validate(descriptors)
                .map_err(|error| AudioIoPolicyError::InvalidAllowedConfiguration {
                    index,
                    error,
                })?;
            for (previous, other) in configurations[..index].iter().enumerate() {
                if same_audio_configuration(configuration.ports(), other.ports()) {
                    return Err(AudioIoPolicyError::DuplicateAllowedConfiguration {
                        first: previous,
                        second: index,
                    });
                }
            }
        }
        Ok(())
    }
}

fn same_audio_configuration(left: &[ConfiguredAudioPort], right: &[ConfiguredAudioPort]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .all(|expected| right.iter().any(|actual| actual == expected))
}

/// Invalid whole-component audio configuration policy definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioIoPolicyError {
    /// An enumerated policy contained no accepted configurations.
    NoAllowedConfigurations,
    /// One allowed configuration is structurally invalid for the port schema.
    InvalidAllowedConfiguration {
        /// Zero-based configuration index in the policy.
        index: usize,
        /// Structural validation failure.
        error: AudioIoConfigurationError,
    },
    /// Two enumerated configurations are semantically identical.
    DuplicateAllowedConfiguration {
        /// Earlier duplicate index.
        first: usize,
        /// Later duplicate index.
        second: usize,
    },
}

impl fmt::Display for AudioIoPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoAllowedConfigurations => {
                formatter.write_str("enumerated audio I/O policy has no allowed configurations")
            }
            Self::InvalidAllowedConfiguration { index, error } => write!(
                formatter,
                "audio I/O policy configuration {index} is invalid: {error}"
            ),
            Self::DuplicateAllowedConfiguration { first, second } => write!(
                formatter,
                "audio I/O policy configurations {first} and {second} are duplicates"
            ),
        }
    }
}

impl std::error::Error for AudioIoPolicyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidAllowedConfiguration { error, .. } => Some(error),
            Self::NoAllowedConfigurations | Self::DuplicateAllowedConfiguration { .. } => None,
        }
    }
}

/// Coherent requested audio I/O configuration for a component activation.
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

    /// Return all requested active ports.
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
        validate_audio_port_schema(descriptors)?;

        for (index, configured) in self.ports.iter().enumerate() {
            if !descriptors
                .iter()
                .any(|descriptor| descriptor.key == configured.key)
            {
                return Err(AudioIoConfigurationError::UnknownConfiguredPort(
                    configured.key.clone(),
                ));
            }

            if self.ports[..index]
                .iter()
                .any(|previous| previous.key == configured.key)
            {
                return Err(AudioIoConfigurationError::DuplicateConfiguredPort(
                    configured.key.clone(),
                ));
            }
        }

        for descriptor in descriptors.iter().filter(|descriptor| !descriptor.optional) {
            if !self.ports.iter().any(|port| port.key == descriptor.key) {
                return Err(AudioIoConfigurationError::MissingRequiredPort(
                    descriptor.key.clone(),
                ));
            }
        }

        Ok(())
    }

    /// Resolve this setup-facing configuration into a dense activation-owned table.
    ///
    /// The returned configuration has one slot for every descriptor in schema
    /// order. Inactive optional ports are represented by `None`, so an
    /// [`AudioPortIndex`] can be checked with one indexed lookup rather than a
    /// stable-key scan in the callback.
    ///
    /// # Errors
    ///
    /// Returns [`AudioIoConfigurationError`] if structural validation fails or a
    /// schema index cannot be represented by `AudioPortIndex`.
    pub fn resolve(
        self,
        descriptors: &[AudioPortDescriptor],
    ) -> Result<ResolvedAudioIoConfiguration, AudioIoConfigurationError> {
        self.validate(descriptors)?;

        let mut resolved = Vec::with_capacity(descriptors.len());
        for (raw_index, descriptor) in descriptors.iter().enumerate() {
            let index = u32::try_from(raw_index)
                .map(AudioPortIndex::new)
                .map_err(|_| AudioIoConfigurationError::PortIndexNotRepresentable(raw_index))?;
            let active = self
                .ports
                .iter()
                .find(|configured| configured.key == descriptor.key)
                .map(|configured| ResolvedAudioPort {
                    index,
                    direction: descriptor.direction,
                    role: descriptor.role,
                    layout: configured.layout,
                });
            resolved.push(active);
        }

        Ok(ResolvedAudioIoConfiguration { ports: resolved })
    }
}

/// One active audio port after stable identity has been resolved to schema-local identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedAudioPort {
    index: AudioPortIndex,
    direction: PortDirection,
    role: PortRole,
    layout: ChannelLayout,
}

impl ResolvedAudioPort {
    /// Return the dense schema-local identity.
    #[must_use]
    pub const fn index(self) -> AudioPortIndex {
        self.index
    }

    /// Return whether this is an input or output port.
    #[must_use]
    pub const fn direction(self) -> PortDirection {
        self.direction
    }

    /// Return the conventional semantic role.
    #[must_use]
    pub const fn role(self) -> PortRole {
        self.role
    }

    /// Return the active semantic channel layout.
    #[must_use]
    pub const fn layout(self) -> ChannelLayout {
        self.layout
    }
}

/// Activation-owned dense audio configuration.
///
/// Slots correspond one-for-one with the immutable audio schema. An inactive
/// optional port occupies a `None` slot. This representation is independent of
/// plugin-format IDs, device handles, stable strings, or application graph IDs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAudioIoConfiguration {
    ports: Vec<Option<ResolvedAudioPort>>,
}

impl ResolvedAudioIoConfiguration {
    /// Return the active port at one schema-local index, or `None` when the index
    /// is unknown or the optional port is inactive.
    #[must_use]
    pub fn port(&self, index: AudioPortIndex) -> Option<ResolvedAudioPort> {
        usize::try_from(index.get())
            .ok()
            .and_then(|index| self.ports.get(index))
            .copied()
            .flatten()
    }

    /// Return the number of schema slots represented by this configuration.
    #[must_use]
    pub const fn schema_port_count(&self) -> usize {
        self.ports.len()
    }

    /// Iterate active ports in schema order.
    pub fn active_ports(&self) -> impl Iterator<Item = ResolvedAudioPort> + '_ {
        self.ports.iter().copied().flatten()
    }

    /// Validate one process-time audio endpoint against the active configuration.
    ///
    /// # Errors
    ///
    /// Returns [`AudioEndpointError`] when the dense index is unknown/inactive,
    /// the direction is wrong, or the channel lies outside the active layout.
    pub fn validate_endpoint(
        &self,
        port: AudioPortIndex,
        channel: u32,
        expected_direction: PortDirection,
    ) -> Result<(), AudioEndpointError> {
        let raw = usize::try_from(port.get())
            .map_err(|_| AudioEndpointError::PortIndexNotRepresentable(port))?;
        let Some(slot) = self.ports.get(raw) else {
            return Err(AudioEndpointError::UnknownPort(port));
        };
        let Some(active) = *slot else {
            return Err(AudioEndpointError::InactivePort(port));
        };
        if active.direction != expected_direction {
            return Err(AudioEndpointError::WrongDirection {
                port,
                expected: expected_direction,
                actual: active.direction,
            });
        }
        let channel_count = active.layout.channel_count();
        if channel >= channel_count {
            return Err(AudioEndpointError::ChannelOutOfRange {
                port,
                channel,
                channel_count,
            });
        }
        Ok(())
    }
}

/// Invalid process-time endpoint relative to one active audio configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioEndpointError {
    /// The dense port index cannot be represented by platform `usize`.
    PortIndexNotRepresentable(AudioPortIndex),
    /// The dense index is outside the component schema.
    UnknownPort(AudioPortIndex),
    /// The schema port exists but is inactive for this activation.
    InactivePort(AudioPortIndex),
    /// Input/output endpoint direction disagrees with the schema.
    WrongDirection {
        /// Port whose direction is invalid for this endpoint.
        port: AudioPortIndex,
        /// Direction required by the endpoint kind.
        expected: PortDirection,
        /// Direction declared by the component schema.
        actual: PortDirection,
    },
    /// The endpoint channel is outside the active layout.
    ChannelOutOfRange {
        /// Active port being addressed.
        port: AudioPortIndex,
        /// Invalid zero-based channel.
        channel: u32,
        /// Active channel count.
        channel_count: u32,
    },
}

impl fmt::Display for AudioEndpointError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PortIndexNotRepresentable(port) => write!(
                formatter,
                "audio port index {} is not representable on this platform",
                port.get()
            ),
            Self::UnknownPort(port) => {
                write!(
                    formatter,
                    "audio endpoint targets unknown port {}",
                    port.get()
                )
            }
            Self::InactivePort(port) => {
                write!(
                    formatter,
                    "audio endpoint targets inactive port {}",
                    port.get()
                )
            }
            Self::WrongDirection {
                port,
                expected,
                actual,
            } => write!(
                formatter,
                "audio endpoint port {} has direction {actual:?}, expected {expected:?}",
                port.get()
            ),
            Self::ChannelOutOfRange {
                port,
                channel,
                channel_count,
            } => write!(
                formatter,
                "audio endpoint port {} channel {channel} exceeds active channel count {channel_count}",
                port.get()
            ),
        }
    }
}

impl std::error::Error for AudioEndpointError {}

/// Structural audio-I/O validation error.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// A schema position cannot be represented as `AudioPortIndex`.
    PortIndexNotRepresentable(usize),
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
            Self::PortIndexNotRepresentable(index) => write!(
                formatter,
                "audio port schema position {index} cannot be represented by AudioPortIndex"
            ),
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

/// Conventional port schema for a stereo effect helper.
pub static DEFAULT_EFFECT_PORTS: [AudioPortDescriptor; 3] = [
    AudioPortDescriptor::new(
        MAIN_INPUT,
        "Main Input",
        PortDirection::Input,
        PortRole::Main,
        false,
    ),
    AudioPortDescriptor::new(
        MAIN_OUTPUT,
        "Main Output",
        PortDirection::Output,
        PortRole::Main,
        false,
    ),
    AudioPortDescriptor::new(
        SIDECHAIN_INPUT,
        "Sidechain",
        PortDirection::Input,
        PortRole::Sidechain,
        true,
    ),
];

/// Default active effect configuration: stereo main input and output.
pub static DEFAULT_EFFECT_CONFIGURATION_PORTS: [ConfiguredAudioPort; 2] = [
    ConfiguredAudioPort::new(MAIN_INPUT, ChannelLayout::Stereo),
    ConfiguredAudioPort::new(MAIN_OUTPUT, ChannelLayout::Stereo),
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
        assert_eq!(validate_audio_port_schema(&DEFAULT_EFFECT_PORTS), Ok(()));
        assert_eq!(
            audio_port_index(&DEFAULT_EFFECT_PORTS, &MAIN_INPUT),
            Some(AudioPortIndex::new(0))
        );

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
    fn default_effect_policy_accepts_only_supported_whole_layouts() {
        let policy = AudioIoPolicy::stereo_effect();
        assert!(policy.accepts(DEFAULT_EFFECT_CONFIGURATION));

        let with_sidechain = [
            ConfiguredAudioPort::new(MAIN_INPUT, ChannelLayout::Stereo),
            ConfiguredAudioPort::new(MAIN_OUTPUT, ChannelLayout::Stereo),
            ConfiguredAudioPort::new(SIDECHAIN_INPUT, ChannelLayout::Stereo),
        ];
        assert!(policy.accepts(AudioIoConfiguration::new(&with_sidechain)));

        let mono_main = [
            ConfiguredAudioPort::new(MAIN_INPUT, ChannelLayout::Mono),
            ConfiguredAudioPort::new(MAIN_OUTPUT, ChannelLayout::Mono),
        ];
        assert!(!policy.accepts(AudioIoConfiguration::new(&mono_main)));

        let mono_sidechain = [
            ConfiguredAudioPort::new(MAIN_INPUT, ChannelLayout::Stereo),
            ConfiguredAudioPort::new(MAIN_OUTPUT, ChannelLayout::Stereo),
            ConfiguredAudioPort::new(SIDECHAIN_INPUT, ChannelLayout::Mono),
        ];
        assert!(!policy.accepts(AudioIoConfiguration::new(&mono_sidechain)));
    }

    #[test]
    fn enumerated_policy_matching_is_order_independent() {
        let policy = AudioIoPolicy::enumerated(vec![AudioIoConfigurationSpec::new(vec![
            ConfiguredAudioPort::new(MAIN_INPUT, ChannelLayout::Stereo),
            ConfiguredAudioPort::new(MAIN_OUTPUT, ChannelLayout::Stereo),
        ])]);
        let reversed = [
            ConfiguredAudioPort::new(MAIN_OUTPUT, ChannelLayout::Stereo),
            ConfiguredAudioPort::new(MAIN_INPUT, ChannelLayout::Stereo),
        ];
        assert!(policy.accepts(AudioIoConfiguration::new(&reversed)));
        assert_eq!(policy.validate_for_ports(&DEFAULT_EFFECT_PORTS), Ok(()));
    }

    #[test]
    fn policy_definition_rejects_invalid_and_duplicate_configurations() {
        let unknown = AudioIoPolicy::enumerated(vec![AudioIoConfigurationSpec::new(vec![
            ConfiguredAudioPort::new(PortKey::new("audio.unknown"), ChannelLayout::Stereo),
        ])]);
        assert!(matches!(
            unknown.validate_for_ports(&DEFAULT_EFFECT_PORTS),
            Err(AudioIoPolicyError::InvalidAllowedConfiguration { .. })
        ));

        let duplicate = AudioIoPolicy::enumerated(vec![
            AudioIoConfigurationSpec::new(DEFAULT_EFFECT_CONFIGURATION_PORTS.to_vec()),
            AudioIoConfigurationSpec::new(DEFAULT_EFFECT_CONFIGURATION_PORTS.to_vec()),
        ]);
        assert_eq!(
            duplicate.validate_for_ports(&DEFAULT_EFFECT_PORTS),
            Err(AudioIoPolicyError::DuplicateAllowedConfiguration {
                first: 0,
                second: 1,
            })
        );
    }

    #[test]
    fn resolved_configuration_is_dense_and_preserves_inactive_optional_slots() {
        let resolved = DEFAULT_EFFECT_CONFIGURATION
            .resolve(&DEFAULT_EFFECT_PORTS)
            .expect("default effect configuration resolves");
        assert_eq!(resolved.schema_port_count(), 3);
        assert_eq!(
            resolved
                .port(AudioPortIndex::new(0))
                .map(ResolvedAudioPort::layout),
            Some(ChannelLayout::Stereo)
        );
        assert_eq!(
            resolved
                .port(AudioPortIndex::new(1))
                .map(ResolvedAudioPort::direction),
            Some(PortDirection::Output)
        );
        assert_eq!(resolved.port(AudioPortIndex::new(2)), None);
        assert_eq!(resolved.active_ports().count(), 2);
    }

    #[test]
    fn resolved_configuration_validates_endpoint_direction_activity_and_channels() {
        let resolved = DEFAULT_EFFECT_CONFIGURATION
            .resolve(&DEFAULT_EFFECT_PORTS)
            .expect("default effect configuration resolves");
        assert_eq!(
            resolved.validate_endpoint(AudioPortIndex::new(0), 1, PortDirection::Input),
            Ok(())
        );
        assert!(matches!(
            resolved.validate_endpoint(AudioPortIndex::new(0), 0, PortDirection::Output),
            Err(AudioEndpointError::WrongDirection { .. })
        ));
        assert_eq!(
            resolved.validate_endpoint(AudioPortIndex::new(2), 0, PortDirection::Input),
            Err(AudioEndpointError::InactivePort(AudioPortIndex::new(2)))
        );
        assert_eq!(
            resolved.validate_endpoint(AudioPortIndex::new(0), 2, PortDirection::Input),
            Err(AudioEndpointError::ChannelOutOfRange {
                port: AudioPortIndex::new(0),
                channel: 2,
                channel_count: 2,
            })
        );
        assert_eq!(
            resolved.validate_endpoint(AudioPortIndex::new(99), 0, PortDirection::Input),
            Err(AudioEndpointError::UnknownPort(AudioPortIndex::new(99)))
        );
    }

    #[test]
    fn audio_port_metadata_can_be_owned_at_runtime() {
        let key = String::from("audio.dynamic.in");
        let name = String::from("Dynamic Input");
        let descriptor =
            AudioPortDescriptor::owned(key, name, PortDirection::Input, PortRole::Auxiliary, true);
        assert_eq!(descriptor.key.as_str(), "audio.dynamic.in");
        assert_eq!(descriptor.name.as_ref(), "Dynamic Input");
        assert_eq!(validate_audio_port_schema(&[descriptor]), Ok(()));
    }

    #[test]
    fn discrete_layouts_use_the_full_u32_domain_type() {
        let channels = ChannelCount::new(100_000).expect("non-zero channel count");
        assert_eq!(ChannelLayout::Discrete(channels).channel_count(), 100_000);
    }

    #[test]
    fn configuration_rejects_unknown_ports() {
        const UNKNOWN: PortKey = PortKey::new("audio.unknown");
        let ports = [ConfiguredAudioPort::new(UNKNOWN, ChannelLayout::Stereo)];

        assert_eq!(
            AudioIoConfiguration::new(&ports).validate(&DEFAULT_EFFECT_PORTS),
            Err(AudioIoConfigurationError::UnknownConfiguredPort(UNKNOWN))
        );
    }

    #[test]
    fn configuration_requires_non_optional_ports() {
        let ports = [ConfiguredAudioPort::new(MAIN_INPUT, ChannelLayout::Stereo)];

        assert_eq!(
            AudioIoConfiguration::new(&ports).validate(&DEFAULT_EFFECT_PORTS),
            Err(AudioIoConfigurationError::MissingRequiredPort(MAIN_OUTPUT))
        );
    }

    #[test]
    fn configuration_rejects_duplicate_active_ports() {
        let ports = [
            ConfiguredAudioPort::new(MAIN_INPUT, ChannelLayout::Stereo),
            ConfiguredAudioPort::new(MAIN_INPUT, ChannelLayout::Stereo),
        ];

        assert_eq!(
            AudioIoConfiguration::new(&ports).validate(&DEFAULT_EFFECT_PORTS),
            Err(AudioIoConfigurationError::DuplicateConfiguredPort(
                MAIN_INPUT
            ))
        );
    }
}
