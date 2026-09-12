from pathlib import Path

root = Path('.')

# --- audio policy model ----------------------------------------------------------
audio_path = root / 'crates/chassis-core/src/audio.rs'
text = audio_path.read_text()
anchor = '''impl ConfiguredAudioPort {
    /// Construct a setup-facing active port selection.
    #[must_use]
    pub const fn new(key: PortKey, layout: ChannelLayout) -> Self {
        Self { key, layout }
    }
}

'''
policy = anchor + '''/// One owned allowed whole-component audio configuration.
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

'''
if anchor not in text:
    raise SystemExit('ConfiguredAudioPort anchor changed unexpectedly')
text = text.replace(anchor, policy, 1)

# Add focused policy tests before resolved-configuration tests.
test_anchor = '''    #[test]
    fn resolved_configuration_is_dense_and_preserves_inactive_optional_slots() {
'''
policy_tests = '''    #[test]
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

'''
if test_anchor not in text:
    raise SystemExit('audio policy test anchor missing')
text = text.replace(test_anchor, policy_tests + test_anchor, 1)
audio_path.write_text(text)

# --- ComponentSchema owns policy -------------------------------------------------
schema_path = root / 'crates/chassis-core/src/schema.rs'
text = schema_path.read_text()
text = text.replace(
    'AudioIoConfigurationError, AudioPortDescriptor, AudioPortIndex, DEFAULT_EFFECT_PORTS,\n        PortKey, audio_port_index, validate_audio_port_schema,',
    'AudioIoConfigurationError, AudioIoPolicy, AudioIoPolicyError, AudioPortDescriptor,\n        AudioPortIndex, DEFAULT_EFFECT_PORTS, PortKey, audio_port_index, validate_audio_port_schema,',
)
text = text.replace(
    '''    audio_ports: Vec<AudioPortDescriptor>,
    event_ports: Vec<EventPortDescriptor>,
''',
    '''    audio_ports: Vec<AudioPortDescriptor>,
    audio_io_policy: AudioIoPolicy,
    event_ports: Vec<EventPortDescriptor>,
''',
    1,
)
text = text.replace(
    '''        audio_ports: Vec<AudioPortDescriptor>,
        event_ports: Vec<EventPortDescriptor>,
        parameters: Vec<ParameterDescriptor>,
    ) -> Result<Self, ComponentSchemaError> {
        Self::build(
            Some(StateIdentity::new(id, state_schema)),
            audio_ports,
            event_ports,
            parameters,
        )
''',
    '''        audio_ports: Vec<AudioPortDescriptor>,
        audio_io_policy: AudioIoPolicy,
        event_ports: Vec<EventPortDescriptor>,
        parameters: Vec<ParameterDescriptor>,
    ) -> Result<Self, ComponentSchemaError> {
        Self::build(
            Some(StateIdentity::new(id, state_schema)),
            audio_ports,
            audio_io_policy,
            event_ports,
            parameters,
        )
''',
    1,
)
text = text.replace(
    '''    pub fn unidentified(
        audio_ports: Vec<AudioPortDescriptor>,
        event_ports: Vec<EventPortDescriptor>,
        parameters: Vec<ParameterDescriptor>,
    ) -> Result<Self, ComponentSchemaError> {
        Self::build(None, audio_ports, event_ports, parameters)
    }
''',
    '''    pub fn unidentified(
        audio_ports: Vec<AudioPortDescriptor>,
        audio_io_policy: AudioIoPolicy,
        event_ports: Vec<EventPortDescriptor>,
        parameters: Vec<ParameterDescriptor>,
    ) -> Result<Self, ComponentSchemaError> {
        Self::build(None, audio_ports, audio_io_policy, event_ports, parameters)
    }
''',
    1,
)
text = text.replace(
    'Self::unidentified(DEFAULT_EFFECT_PORTS.to_vec(), Vec::new(), parameters)',
    'Self::unidentified(\n            DEFAULT_EFFECT_PORTS.to_vec(),\n            AudioIoPolicy::stereo_effect(),\n            Vec::new(),\n            parameters,\n        )',
    1,
)
text = text.replace(
    '''            DEFAULT_EFFECT_PORTS.to_vec(),
            Vec::new(),
            parameters,
''',
    '''            DEFAULT_EFFECT_PORTS.to_vec(),
            AudioIoPolicy::stereo_effect(),
            Vec::new(),
            parameters,
''',
    1,
)
text = text.replace(
    '''        audio_ports: Vec<AudioPortDescriptor>,
        event_ports: Vec<EventPortDescriptor>,
        parameters: Vec<ParameterDescriptor>,
    ) -> Result<Self, ComponentSchemaError> {
        validate_audio_port_schema(&audio_ports).map_err(ComponentSchemaError::AudioPorts)?;
        validate_event_port_schema(&event_ports).map_err(ComponentSchemaError::EventPorts)?;
        ParameterStore::new(&parameters).map_err(ComponentSchemaError::Parameters)?;

        Ok(Self {
            state_identity,
            audio_ports,
            event_ports,
            parameters,
        })
''',
    '''        audio_ports: Vec<AudioPortDescriptor>,
        audio_io_policy: AudioIoPolicy,
        event_ports: Vec<EventPortDescriptor>,
        parameters: Vec<ParameterDescriptor>,
    ) -> Result<Self, ComponentSchemaError> {
        validate_audio_port_schema(&audio_ports).map_err(ComponentSchemaError::AudioPorts)?;
        audio_io_policy
            .validate_for_ports(&audio_ports)
            .map_err(ComponentSchemaError::AudioIoPolicy)?;
        validate_event_port_schema(&event_ports).map_err(ComponentSchemaError::EventPorts)?;
        ParameterStore::new(&parameters).map_err(ComponentSchemaError::Parameters)?;

        Ok(Self {
            state_identity,
            audio_ports,
            audio_io_policy,
            event_ports,
            parameters,
        })
''',
    1,
)
accessor_anchor = '''    /// Return the immutable event-port schema.
    #[must_use]
    pub fn event_ports(&self) -> &[EventPortDescriptor] {
        &self.event_ports
    }
'''
accessor = '''    /// Return the immutable whole-component audio I/O policy.
    #[must_use]
    pub const fn audio_io_policy(&self) -> &AudioIoPolicy {
        &self.audio_io_policy
    }

''' + accessor_anchor
if accessor_anchor not in text:
    raise SystemExit('ComponentSchema event accessor anchor missing')
text = text.replace(accessor_anchor, accessor, 1)
text = text.replace(
    '''pub enum ComponentSchemaError {
    /// Audio-port schema is invalid.
    AudioPorts(AudioIoConfigurationError),
''',
    '''pub enum ComponentSchemaError {
    /// Audio-port schema is invalid.
    AudioPorts(AudioIoConfigurationError),
    /// Whole-component audio I/O policy is invalid for the declared ports.
    AudioIoPolicy(AudioIoPolicyError),
''',
    1,
)
text = text.replace(
    '''            Self::AudioPorts(error) => write!(formatter, "invalid audio ports: {error}"),
            Self::EventPorts(error) => write!(formatter, "invalid event ports: {error}"),
''',
    '''            Self::AudioPorts(error) => write!(formatter, "invalid audio ports: {error}"),
            Self::AudioIoPolicy(error) => write!(formatter, "invalid audio I/O policy: {error}"),
            Self::EventPorts(error) => write!(formatter, "invalid event ports: {error}"),
''',
    1,
)
text = text.replace(
    '''            Self::AudioPorts(error) => Some(error),
            Self::EventPorts(error) => Some(error),
''',
    '''            Self::AudioPorts(error) => Some(error),
            Self::AudioIoPolicy(error) => Some(error),
            Self::EventPorts(error) => Some(error),
''',
    1,
)

# Schema unit-test constructor gets the conventional effect policy.
text = text.replace(
    '''            DEFAULT_EFFECT_PORTS.to_vec(),
            vec![EventPortDescriptor::new(''',
    '''            DEFAULT_EFFECT_PORTS.to_vec(),
            AudioIoPolicy::stereo_effect(),
            vec![EventPortDescriptor::new(''',
    1,
)
text = text.replace(
    'ComponentSchema::unidentified(DEFAULT_EFFECT_PORTS.to_vec(), vec![], vec![])',
    'ComponentSchema::unidentified(\n            DEFAULT_EFFECT_PORTS.to_vec(),\n            AudioIoPolicy::stereo_effect(),\n            vec![],\n            vec![],\n        )',
    1,
)
schema_path.write_text(text)

# --- migrate direct schema construction -----------------------------------------
process_path = root / 'crates/chassis-core/src/process.rs'
text = process_path.read_text()
text = text.replace(
    '''        ComponentSchema::unidentified(DEFAULT_EFFECT_PORTS.to_vec(), vec![], vec![])
            .expect("default effect schema is valid")''',
    '''        ComponentSchema::stereo_effect(vec![]).expect("default effect schema is valid")''',
)
process_path.write_text(text)

note_path = root / 'crates/chassis-core/tests/note_runtime.rs'
text = note_path.read_text()
text = text.replace(
    'chassis_core::schema::ComponentSchema::unidentified(vec![], NOTE_PORTS.to_vec(), vec![])',
    'chassis_core::schema::ComponentSchema::unidentified(\n            vec![],\n            chassis_core::audio::AudioIoPolicy::any_structurally_valid(),\n            NOTE_PORTS.to_vec(),\n            vec![],\n        )',
)
note_path.write_text(text)

event_path = root / 'crates/chassis-core/tests/event_schema_runtime.rs'
text = event_path.read_text()
text = text.replace(
    'chassis_core::schema::ComponentSchema::unidentified(vec![], self.ports.to_vec(), vec![])',
    'chassis_core::schema::ComponentSchema::unidentified(\n            vec![],\n            chassis_core::audio::AudioIoPolicy::any_structurally_valid(),\n            self.ports.to_vec(),\n            vec![],\n        )',
)
event_path.write_text(text)

component_path = root / 'crates/chassis-core/tests/component_schema_runtime.rs'
text = component_path.read_text()
text = text.replace(
    '''            self.audio_ports.to_vec(),
            vec![],
            vec![],
''',
    '''            self.audio_ports.to_vec(),
            chassis_core::audio::AudioIoPolicy::any_structurally_valid(),
            vec![],
            vec![],
''',
    1,
)
component_path.write_text(text)

semantic_path = root / 'crates/chassis-core/tests/semantic_state.rs'
text = semantic_path.read_text()
text = text.replace(
    '''            DEFAULT_EFFECT_PORTS.to_vec(),
            vec![],
            self.parameters.clone(),
''',
    '''            DEFAULT_EFFECT_PORTS.to_vec(),
            chassis_core::audio::AudioIoPolicy::stereo_effect(),
            vec![],
            self.parameters.clone(),
''',
    1,
)
semantic_path.write_text(text)

# --- runtime validates policy and drift ------------------------------------------
runtime_path = root / 'crates/chassis-core/src/runtime.rs'
text = runtime_path.read_text()
text = text.replace(
    '''        AudioEndpointError, AudioIoConfiguration, AudioIoConfigurationError, AudioPortDescriptor,
        AudioPortIndex, ConfiguredAudioPort, PortDirection, PortKey, ResolvedAudioIoConfiguration,
''',
    '''        AudioEndpointError, AudioIoConfiguration, AudioIoConfigurationError, AudioIoPolicyError,
        AudioPortDescriptor, AudioPortIndex, ConfiguredAudioPort, PortDirection, PortKey,
        ResolvedAudioIoConfiguration,
''',
    1,
)
text = text.replace(
    '''    /// The component's immutable parameter schema failed validation.
    InvalidParameters(ParameterStoreError),
''',
    '''    /// The component's whole-component audio I/O policy failed validation.
    InvalidAudioIoPolicy(AudioIoPolicyError),
    /// The component's immutable parameter schema failed validation.
    InvalidParameters(ParameterStoreError),
''',
    1,
)
text = text.replace(
    '''        ComponentSchemaError::AudioPorts(error) => InstanceRuntimeError::InvalidAudioPorts(error),
        ComponentSchemaError::EventPorts(error) => InstanceRuntimeError::InvalidEventPorts(error),
''',
    '''        ComponentSchemaError::AudioPorts(error) => InstanceRuntimeError::InvalidAudioPorts(error),
        ComponentSchemaError::AudioIoPolicy(error) => {
            InstanceRuntimeError::InvalidAudioIoPolicy(error)
        }
        ComponentSchemaError::EventPorts(error) => InstanceRuntimeError::InvalidEventPorts(error),
''',
    1,
)
text = text.replace(
    '''            Self::InvalidAudioPorts(error) => write!(formatter, "invalid audio ports: {error}"),
            Self::InvalidParameters(error) => write!(formatter, "invalid parameters: {error}"),
''',
    '''            Self::InvalidAudioPorts(error) => write!(formatter, "invalid audio ports: {error}"),
            Self::InvalidAudioIoPolicy(error) => {
                write!(formatter, "invalid audio I/O policy: {error}")
            }
            Self::InvalidParameters(error) => write!(formatter, "invalid parameters: {error}"),
''',
    1,
)
text = text.replace(
    '''            Self::InvalidAudioPorts(error) => Some(error),
            Self::InvalidParameters(error) => Some(error),
''',
    '''            Self::InvalidAudioPorts(error) => Some(error),
            Self::InvalidAudioIoPolicy(error) => Some(error),
            Self::InvalidParameters(error) => Some(error),
''',
    1,
)
text = text.replace(
    '''    /// The component parameter schema no longer matches the instance schema.
    ParameterSchemaMismatch,
''',
    '''    /// The component whole-audio-I/O policy no longer matches the instance schema.
    AudioIoPolicyMismatch,
    /// The component parameter schema no longer matches the instance schema.
    ParameterSchemaMismatch,
''',
    1,
)
text = text.replace(
    '''    /// The proposed whole-component I/O configuration failed structural validation.
    InvalidAudioIo(AudioIoConfigurationError),
''',
    '''    /// The proposed whole-component I/O configuration failed structural validation.
    InvalidAudioIo(AudioIoConfigurationError),
    /// The structurally valid I/O configuration is not accepted by component policy.
    UnsupportedAudioIo,
''',
    1,
)
text = text.replace(
    '''            Self::AudioPortSchemaMismatch => formatter
                .write_str("component audio-port schema does not match its instance runtime"),
            Self::ParameterSchemaMismatch => formatter
''',
    '''            Self::AudioPortSchemaMismatch => formatter
                .write_str("component audio-port schema does not match its instance runtime"),
            Self::AudioIoPolicyMismatch => formatter
                .write_str("component audio I/O policy does not match its instance runtime"),
            Self::ParameterSchemaMismatch => formatter
''',
    1,
)
text = text.replace(
    '''            Self::InvalidAudioIo(error) => write!(formatter, "invalid audio I/O: {error}"),
            Self::Product(error) => write!(formatter, "product activation failed: {error}"),
''',
    '''            Self::InvalidAudioIo(error) => write!(formatter, "invalid audio I/O: {error}"),
            Self::UnsupportedAudioIo => {
                formatter.write_str("audio I/O configuration is unsupported by component policy")
            }
            Self::Product(error) => write!(formatter, "product activation failed: {error}"),
''',
    1,
)
text = text.replace(
    '''            | Self::AudioPortSchemaMismatch
            | Self::ParameterSchemaMismatch
''',
    '''            | Self::AudioPortSchemaMismatch
            | Self::AudioIoPolicyMismatch
            | Self::ParameterSchemaMismatch
''',
    1,
)
text = text.replace(
    '''            | Self::EventPortSchemaMismatch
            | Self::StateIdentityMismatch => None,
''',
    '''            | Self::EventPortSchemaMismatch
            | Self::StateIdentityMismatch
            | Self::UnsupportedAudioIo => None,
''',
    1,
)
text = text.replace(
    '''        if self.schema.parameters() != component_schema.parameters() {
            return Err(ActivateError::ParameterSchemaMismatch);
        }
''',
    '''        if self.schema.audio_io_policy() != component_schema.audio_io_policy() {
            return Err(ActivateError::AudioIoPolicyMismatch);
        }
        if self.schema.parameters() != component_schema.parameters() {
            return Err(ActivateError::ParameterSchemaMismatch);
        }
''',
    1,
)
text = text.replace(
    '''        let resolved_audio = audio_io
            .resolve(self.schema.audio_ports())
            .map_err(ActivateError::InvalidAudioIo)?;
        let audio_ports = audio_io.ports().to_vec();
''',
    '''        let resolved_audio = audio_io
            .resolve(self.schema.audio_ports())
            .map_err(ActivateError::InvalidAudioIo)?;
        if !self.schema.audio_io_policy().accepts(audio_io) {
            return Err(ActivateError::UnsupportedAudioIo);
        }
        let audio_ports = audio_io.ports().to_vec();
''',
    1,
)
runtime_path.write_text(text)

# --- CLAP setup rejects a mapping outside schema policy --------------------------
clap_path = root / 'crates/chassis-clap/src/lib.rs'
text = clap_path.read_text()
old = '''    let mut audio = ClapAudioConfiguration::new(shared.schema.audio_ports(), C::CLAP_AUDIO_PORTS)
        .map_err(|error| audio_mapping_error(&error))?;
    if supports_f64 {
'''
new = '''    let mut audio = ClapAudioConfiguration::new(shared.schema.audio_ports(), C::CLAP_AUDIO_PORTS)
        .map_err(|error| audio_mapping_error(&error))?;
    if !shared.schema.audio_io_policy().accepts(audio.audio_io()) {
        return Err(PluginError::Message(
            "CLAP audio mapping is unsupported by the component audio I/O policy",
        ));
    }
    if supports_f64 {
'''
if old not in text:
    raise SystemExit('CLAP audio setup anchor changed unexpectedly')
text = text.replace(old, new, 1)
clap_path.write_text(text)

# --- activation policy regression coverage --------------------------------------
conformance_path = root / 'crates/chassis-core/tests/conformance.rs'
text = conformance_path.read_text()
anchor = '''#[test]
fn activation_rejects_invalid_audio_configuration_without_calling_product() {
'''
policy_test = '''#[test]
fn activation_rejects_structurally_valid_but_unsupported_audio_configuration() {
    let metrics = Arc::new(Metrics::default());
    let component = ConformanceEffect::new(Arc::clone(&metrics), 1.0);
    let mut runtime = runtime(&component);
    let mono = [
        ConfiguredAudioPort::new(MAIN_INPUT, ChannelLayout::Mono),
        ConfiguredAudioPort::new(MAIN_OUTPUT, ChannelLayout::Mono),
    ];

    assert_eq!(
        runtime.activate(
            &component,
            process_config(Some(1), 8),
            AudioIoConfiguration::new(&mono),
        ),
        Err(ActivateError::UnsupportedAudioIo)
    );
    assert!(!runtime.is_active());
    assert_eq!(metric(&metrics.activations), 0);
}

'''
if anchor not in text:
    raise SystemExit('conformance activation test anchor missing')
text = text.replace(anchor, policy_test + anchor, 1)
conformance_path.write_text(text)

# Fail closed on direct constructors that missed the mandatory policy argument.
# These patterns identify the known old call shapes after the targeted migration.
offenders = []
for path in root.rglob('*.rs'):
    source = path.read_text()
    if 'ComponentSchema::unidentified(DEFAULT_EFFECT_PORTS.to_vec(), vec![], vec![])' in source:
        offenders.append(str(path))
    if 'ComponentSchema::unidentified(vec![], NOTE_PORTS.to_vec(), vec![])' in source:
        offenders.append(str(path))
    if 'ComponentSchema::unidentified(vec![], self.ports.to_vec(), vec![])' in source:
        offenders.append(str(path))
if offenders:
    raise SystemExit(f'old ComponentSchema constructor shape remains: {sorted(set(offenders))}')
