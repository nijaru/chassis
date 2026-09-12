from pathlib import Path
import re

root = Path('.')

# --- chassis-core audio metadata -------------------------------------------------
audio_path = root / 'crates/chassis-core/src/audio.rs'
text = audio_path.read_text()

text = text.replace(
    'use std::vec::Vec;',
    'use std::{borrow::Cow, string::String, vec::Vec};',
)

old = '''#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
'''
new = '''#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
'''
if old not in text:
    raise SystemExit('PortKey block changed unexpectedly')
text = text.replace(old, new)

old = '''#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
'''
new = '''#[derive(Debug, Clone, PartialEq, Eq)]
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
'''
if old not in text:
    raise SystemExit('AudioPortDescriptor block changed unexpectedly')
text = text.replace(old, new)

text = text.replace(
    '''        if descriptors[..index]
            .iter()
            .any(|previous| previous.key == descriptor.key)
        {
            return Err(AudioIoConfigurationError::DuplicateDescriptorPort(
                descriptor.key,
            ));
        }
''',
    '''        if descriptors[..index]
            .iter()
            .any(|previous| previous.key == descriptor.key)
        {
            return Err(AudioIoConfigurationError::DuplicateDescriptorPort(
                descriptor.key.clone(),
            ));
        }
''',
)

old = '''pub fn audio_port_index(
    descriptors: &[AudioPortDescriptor],
    key: PortKey,
) -> Option<AudioPortIndex> {
    descriptors
        .iter()
        .position(|descriptor| descriptor.key == key)
        .and_then(|index| u32::try_from(index).ok())
        .map(AudioPortIndex::new)
}
'''
new = '''pub fn audio_port_index(
    descriptors: &[AudioPortDescriptor],
    key: &PortKey,
) -> Option<AudioPortIndex> {
    descriptors
        .iter()
        .position(|descriptor| &descriptor.key == key)
        .and_then(|index| u32::try_from(index).ok())
        .map(AudioPortIndex::new)
}
'''
if old not in text:
    raise SystemExit('audio_port_index shape changed unexpectedly')
text = text.replace(old, new)

old = '''#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfiguredAudioPort {
    /// Stable key of the active port.
    pub key: PortKey,
    /// Accepted semantic layout.
    pub layout: ChannelLayout,
}
'''
new = '''#[derive(Debug, Clone, PartialEq, Eq)]
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
'''
if old not in text:
    raise SystemExit('ConfiguredAudioPort block changed unexpectedly')
text = text.replace(old, new)

# Error values must own a clone now that keys can be runtime-owned.
text = text.replace('configured.key,\n                ));', 'configured.key.clone(),\n                ));')
text = text.replace('descriptor.key,\n                ));', 'descriptor.key.clone(),\n                ));')
text = text.replace(
    '#[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub enum AudioIoConfigurationError',
    '#[derive(Debug, Clone, PartialEq, Eq)]\npub enum AudioIoConfigurationError',
)

# Use constructor helpers for static descriptors/configuration so const authoring
# remains concise despite owned-capable metadata.
def descriptor_repl(match):
    key, name, direction, role, optional = match.groups()
    return f'AudioPortDescriptor::new({key}, "{name}", PortDirection::{direction}, PortRole::{role}, {optional})'

text = re.sub(
    r'AudioPortDescriptor \{\s*key: ([A-Z][A-Z0-9_]*),\s*name: "([^"]+)",\s*direction: PortDirection::(Input|Output),\s*role: PortRole::(Main|Sidechain|Auxiliary),\s*optional: (true|false),\s*\}',
    descriptor_repl,
    text,
)
text = re.sub(
    r'ConfiguredAudioPort \{\s*key: ([A-Z][A-Z0-9_]*),\s*layout: (ChannelLayout::[A-Za-z]+),\s*\}',
    r'ConfiguredAudioPort::new(\1, \2)',
    text,
)
text = text.replace('audio_port_index(&DEFAULT_EFFECT_PORTS, MAIN_INPUT)', 'audio_port_index(&DEFAULT_EFFECT_PORTS, &MAIN_INPUT)')

# Prove runtime-owned audio metadata explicitly.
test_anchor = '''    #[test]
    fn discrete_layouts_use_the_full_u32_domain_type() {
'''
owned_test = '''    #[test]
    fn audio_port_metadata_can_be_owned_at_runtime() {
        let key = String::from("audio.dynamic.in");
        let name = String::from("Dynamic Input");
        let descriptor = AudioPortDescriptor::owned(
            key,
            name,
            PortDirection::Input,
            PortRole::Auxiliary,
            true,
        );
        assert_eq!(descriptor.key.as_str(), "audio.dynamic.in");
        assert_eq!(descriptor.name.as_ref(), "Dynamic Input");
        assert_eq!(validate_audio_port_schema(&[descriptor]), Ok(()));
    }

'''
if test_anchor not in text:
    raise SystemExit('audio owned metadata test anchor missing')
text = text.replace(test_anchor, owned_test + test_anchor, 1)
audio_path.write_text(text)

# --- schema/runtime lookup APIs borrow stable keys -------------------------------
for rel in ['crates/chassis-core/src/schema.rs', 'crates/chassis-core/src/runtime.rs']:
    path = root / rel
    text = path.read_text()
    text = text.replace(
        'pub fn audio_port_index(&self, key: PortKey) -> Option<AudioPortIndex> {\n        audio_port_index(&self.audio_ports, key)\n    }',
        'pub fn audio_port_index(&self, key: &PortKey) -> Option<AudioPortIndex> {\n        audio_port_index(&self.audio_ports, key)\n    }',
    )
    text = text.replace(
        'pub fn audio_port_index(&self, key: PortKey) -> Option<AudioPortIndex> {\n        self.schema.audio_port_index(key)\n    }',
        'pub fn audio_port_index(&self, key: &PortKey) -> Option<AudioPortIndex> {\n        self.schema.audio_port_index(key)\n    }',
    )
    path.write_text(text)

# Migrate static-key lookup call sites to the borrowed API.
for path in root.rglob('*.rs'):
    text = path.read_text()
    original = text
    text = re.sub(
        r'\.audio_port_index\((MAIN_INPUT|MAIN_OUTPUT|SIDECHAIN_INPUT|OTHER_INPUT|OTHER_OUTPUT)\)',
        r'.audio_port_index(&\1)',
        text,
    )
    if text != original:
        path.write_text(text)

# Replace direct static descriptor/configuration literals in test/adaptor files.
for path in root.rglob('*.rs'):
    if path == audio_path:
        continue
    text = path.read_text()
    original = text
    text = re.sub(
        r'AudioPortDescriptor \{\s*key: ([A-Z][A-Z0-9_]*),\s*name: "([^"]+)",\s*direction: PortDirection::(Input|Output),\s*role: PortRole::(Main|Sidechain|Auxiliary),\s*optional: (true|false),\s*\}',
        descriptor_repl,
        text,
    )
    text = re.sub(
        r'ConfiguredAudioPort \{\s*key: ([A-Z][A-Z0-9_]*),\s*layout: (ChannelLayout::[A-Za-z]+),\s*\}',
        r'ConfiguredAudioPort::new(\1, \2)',
        text,
    )
    if text != original:
        path.write_text(text)

# Const-pattern matching on an owned-capable key is no longer appropriate.
conformance_path = root / 'crates/chassis-core/tests/conformance.rs'
text = conformance_path.read_text()
text = text.replace(
    '''        Err(ActivateError::InvalidAudioIo(
            AudioIoConfigurationError::MissingRequiredPort(MAIN_OUTPUT)
        ))
''',
    '''        Err(ActivateError::InvalidAudioIo(
            AudioIoConfigurationError::MissingRequiredPort(key)
        )) if key == MAIN_OUTPUT
''',
)
conformance_path.write_text(text)

# --- CLAP setup projection --------------------------------------------------------
clap_path = root / 'crates/chassis-clap/src/audio.rs'
text = clap_path.read_text()
text = text.replace(
    '#[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub struct ClapAudioPort',
    '#[derive(Debug, Clone, PartialEq, Eq)]\npub struct ClapAudioPort',
)
text = text.replace(
    '#[derive(Debug, Clone, Copy)]\nstruct PortBinding',
    '#[derive(Debug, Clone)]\nstruct PortBinding',
)
text = text.replace(
    "    fn info(self, supports_f64: bool) -> AudioPortInfo<'static> {",
    "    fn info(&self, supports_f64: bool) -> AudioPortInfo<'_> {",
)
text = text.replace(
    '''pub(crate) struct ClapProcessPort {
    /// Stable setup identity retained only while process endpoint consumers migrate.
    pub(crate) key: PortKey,
    /// Dense Chassis schema-local audio-port identity for callback use.
    pub(crate) index: AudioPortIndex,
    pub(crate) layout: ChannelLayout,
}
''',
    '''pub(crate) struct ClapProcessPort {
    /// Dense Chassis schema-local audio-port identity for callback use.
    pub(crate) index: AudioPortIndex,
    pub(crate) layout: ChannelLayout,
}
''',
)
text = text.replace(
    ") -> Option<AudioPortInfo<'static>> {",
    ") -> Option<AudioPortInfo<'_>> {",
)
text = text.replace(
    '''        ports
            .get(index)
            .copied()
            .map(|binding| binding.info(self.supports_f64))
''',
    '''        ports
            .get(index)
            .map(|binding| binding.info(self.supports_f64))
''',
)

# Setup mapping comparisons borrow keys; errors/configuration clone only off-RT.
text = text.replace('AudioMappingError::InvalidClapId(mapped.key)', 'AudioMappingError::InvalidClapId(mapped.key.clone())')
text = text.replace('AudioMappingError::DuplicateMappedPort(mapped.key)', 'AudioMappingError::DuplicateMappedPort(mapped.key.clone())')
text = text.replace('AudioMappingError::UnknownMappedPort(mapped.key)', 'AudioMappingError::UnknownMappedPort(mapped.key.clone())')
text = text.replace('key: mapped.key,\n                layout: mapped.layout,', 'key: mapped.key.clone(),\n                layout: mapped.layout,')
text = text.replace('audio_port_index(descriptors, mapped.key)', 'audio_port_index(descriptors, &mapped.key)')
text = text.replace('.copied()\n        .ok_or(AudioMappingError::UnknownMappedPort(mapped.key.clone()))?', '.cloned()\n        .ok_or(AudioMappingError::UnknownMappedPort(mapped.key.clone()))?')
text = text.replace('in_place_pair_id(descriptors, mapping, descriptor, *mapped)?', 'in_place_pair_id(descriptors, mapping, &descriptor, mapped)?')
text = text.replace('mapping: *mapped,', 'mapping: mapped.clone(),')

old = '''fn in_place_pair_id(
    descriptors: &[AudioPortDescriptor],
    mapping: &[ClapAudioPort],
    descriptor: AudioPortDescriptor,
    mapped: ClapAudioPort,
) -> Result<Option<u32>, AudioMappingError> {
    let Some(pair_key) = mapped.in_place_pair else {
        return Ok(None);
    };
    let pair_descriptor = descriptors
        .iter()
        .copied()
        .find(|candidate| candidate.key == pair_key)
        .ok_or(AudioMappingError::UnknownInPlacePair {
            port: mapped.key,
            pair: pair_key,
        })?;
    if pair_descriptor.direction == descriptor.direction {
        return Err(AudioMappingError::InPlacePairSameDirection {
            port: mapped.key,
            pair: pair_key,
        });
    }
    let pair = mapping
        .iter()
        .copied()
        .find(|candidate| candidate.key == pair_key)
        .ok_or(AudioMappingError::InactiveInPlacePair {
            port: mapped.key,
            pair: pair_key,
        })?;
    if pair.layout.channel_count() != mapped.layout.channel_count() {
        return Err(AudioMappingError::InPlacePairChannelMismatch {
            port: mapped.key,
            pair: pair_key,
        });
    }
    if pair.in_place_pair != Some(mapped.key) {
        return Err(AudioMappingError::NonReciprocalInPlacePair {
            port: mapped.key,
            pair: pair_key,
        });
    }
    Ok(Some(pair.id))
}
'''
new = '''fn in_place_pair_id(
    descriptors: &[AudioPortDescriptor],
    mapping: &[ClapAudioPort],
    descriptor: &AudioPortDescriptor,
    mapped: &ClapAudioPort,
) -> Result<Option<u32>, AudioMappingError> {
    let Some(pair_key) = mapped.in_place_pair.as_ref() else {
        return Ok(None);
    };
    let pair_descriptor = descriptors
        .iter()
        .find(|candidate| &candidate.key == pair_key)
        .ok_or_else(|| AudioMappingError::UnknownInPlacePair {
            port: mapped.key.clone(),
            pair: pair_key.clone(),
        })?;
    if pair_descriptor.direction == descriptor.direction {
        return Err(AudioMappingError::InPlacePairSameDirection {
            port: mapped.key.clone(),
            pair: pair_key.clone(),
        });
    }
    let pair = mapping
        .iter()
        .find(|candidate| &candidate.key == pair_key)
        .ok_or_else(|| AudioMappingError::InactiveInPlacePair {
            port: mapped.key.clone(),
            pair: pair_key.clone(),
        })?;
    if pair.layout.channel_count() != mapped.layout.channel_count() {
        return Err(AudioMappingError::InPlacePairChannelMismatch {
            port: mapped.key.clone(),
            pair: pair_key.clone(),
        });
    }
    if pair.in_place_pair.as_ref() != Some(&mapped.key) {
        return Err(AudioMappingError::NonReciprocalInPlacePair {
            port: mapped.key.clone(),
            pair: pair_key.clone(),
        });
    }
    Ok(Some(pair.id))
}
'''
if old not in text:
    raise SystemExit('CLAP in_place_pair_id shape changed unexpectedly')
text = text.replace(old, new)

old = '''fn process_pair_key(binding: &PortBinding) -> &str {
    match (binding.descriptor.direction, binding.mapping.in_place_pair) {
        (PortDirection::Output, Some(input)) => input.as_str(),
        (PortDirection::Input, Some(_)) | (_, None) => binding.mapping.key.as_str(),
    }
}
'''
new = '''fn process_pair_key(binding: &PortBinding) -> &str {
    match (binding.descriptor.direction, binding.mapping.in_place_pair.as_ref()) {
        (PortDirection::Output, Some(input)) => input.as_str(),
        (PortDirection::Input, Some(_)) | (_, None) => binding.mapping.key.as_str(),
    }
}
'''
if old not in text:
    raise SystemExit('CLAP process_pair_key shape changed unexpectedly')
text = text.replace(old, new)

old = '''fn validate_pair_alignment(
    inputs: &[PortBinding],
    outputs: &[PortBinding],
) -> Result<(), AudioMappingError> {
    for (index, input) in inputs.iter().enumerate() {
        let Some(pair) = input.mapping.in_place_pair else {
            continue;
        };
        if outputs.get(index).map(|output| output.mapping.key) != Some(pair) {
            return Err(AudioMappingError::InPlacePairIndexMismatch {
                port: input.mapping.key,
                pair,
            });
        }
    }
    for (index, output) in outputs.iter().enumerate() {
        let Some(pair) = output.mapping.in_place_pair else {
            continue;
        };
        if inputs.get(index).map(|input| input.mapping.key) != Some(pair) {
            return Err(AudioMappingError::InPlacePairIndexMismatch {
                port: output.mapping.key,
                pair,
            });
        }
    }
    Ok(())
}
'''
new = '''fn validate_pair_alignment(
    inputs: &[PortBinding],
    outputs: &[PortBinding],
) -> Result<(), AudioMappingError> {
    for (index, input) in inputs.iter().enumerate() {
        let Some(pair) = input.mapping.in_place_pair.as_ref() else {
            continue;
        };
        if outputs.get(index).map(|output| &output.mapping.key) != Some(pair) {
            return Err(AudioMappingError::InPlacePairIndexMismatch {
                port: input.mapping.key.clone(),
                pair: pair.clone(),
            });
        }
    }
    for (index, output) in outputs.iter().enumerate() {
        let Some(pair) = output.mapping.in_place_pair.as_ref() else {
            continue;
        };
        if inputs.get(index).map(|input| &input.mapping.key) != Some(pair) {
            return Err(AudioMappingError::InPlacePairIndexMismatch {
                port: output.mapping.key.clone(),
                pair: pair.clone(),
            });
        }
    }
    Ok(())
}
'''
if old not in text:
    raise SystemExit('CLAP pair alignment shape changed unexpectedly')
text = text.replace(old, new)

text = text.replace(
    '''            (Some(input), Some(output)) => {
                input.mapping.in_place_pair == Some(output.mapping.key)
                    && output.mapping.in_place_pair == Some(input.mapping.key)
            }
''',
    '''            (Some(input), Some(output)) => {
                input.mapping.in_place_pair.as_ref() == Some(&output.mapping.key)
                    && output.mapping.in_place_pair.as_ref() == Some(&input.mapping.key)
            }
''',
)
text = text.replace(
    '''    ClapProcessPort {
        key: binding.mapping.key,
        index: binding.index,
        layout: binding.mapping.layout,
    }
''',
    '''    ClapProcessPort {
        index: binding.index,
        layout: binding.mapping.layout,
    }
''',
)
text = text.replace(
    '#[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub(crate) enum AudioMappingError',
    '#[derive(Debug, Clone, PartialEq, Eq)]\npub(crate) enum AudioMappingError',
)

# Remove tests that asserted stable keys remained in the callback plan; dense
# indices are now the only process identity.
text = re.sub(
    r'\n\s*assert_eq!\(\s*configuration\.process_slots\(\)\[[^\]]+\]\s*\.input\s*\.expect\([^\n]+\)\s*\.key,\s*[A-Z][A-Z0-9_]*\s*\);',
    '',
    text,
    flags=re.S,
)
text = re.sub(
    r'\n\s*assert_eq!\(\s*configuration\.process_slots\(\)\[[^\]]+\]\s*\.output\s*\.expect\([^\n]+\)\s*\.key,\s*[A-Z][A-Z0-9_]*\s*\);',
    '',
    text,
    flags=re.S,
)
text = re.sub(
    r'\n\s*assert_eq!\(\s*(?:main|slot|slots\[[^\]]+\])\.(?:input|output)\.expect\([^\n]+\)\.key,\s*[A-Z][A-Z0-9_]*\s*\);',
    '',
    text,
    flags=re.S,
)

# Non-Copy default mapping elements used in tests need explicit clones.
text = re.sub(r'DEFAULT_CLAP_AUDIO_PORTS\[(\d+)\]', r'DEFAULT_CLAP_AUDIO_PORTS[\1].clone()', text)
# Do not alter the const mapping declaration itself.
text = text.replace('ClapAudioPort::new(MAIN_INPUT.clone(),', 'ClapAudioPort::new(MAIN_INPUT,')
text = text.replace('ClapAudioPort::new(MAIN_OUTPUT.clone(),', 'ClapAudioPort::new(MAIN_OUTPUT,')
text = text.replace('ClapAudioPort::new(SIDECHAIN_INPUT.clone(),', 'ClapAudioPort::new(SIDECHAIN_INPUT,')

clap_path.write_text(text)

# Dynamic key support should not leave any stable key in the CLAP process struct.
text = clap_path.read_text()
process_struct = text[text.index('pub(crate) struct ClapProcessPort'):text.index('pub(crate) struct ClapProcessSlot')]
if 'PortKey' in process_struct or ' key:' in process_struct:
    raise SystemExit('stable audio key remains in CLAP process port')

# Sanity: key lookup APIs should now borrow and PortKey must no longer be Copy.
audio_text = audio_path.read_text()
if 'pub struct PortKey(Cow' not in audio_text or 'Clone, Copy, PartialEq' in audio_text.split('pub struct PortKey', 1)[0][-100:]:
    raise SystemExit('PortKey ownership migration did not apply')
if 'key: &PortKey' not in audio_text:
    raise SystemExit('audio_port_index did not migrate to borrowed key lookup')
