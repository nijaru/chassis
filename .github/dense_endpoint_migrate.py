from pathlib import Path
import re

root = Path('.')
buffer_path = root / 'crates/chassis-core/src/buffer.rs'
text = buffer_path.read_text()
start = text.index('use crate::audio::{AudioPortIndex, PortKey};')
end = text.index('/// Proven relationship between the host buffers represented by one channel view.')
replacement = '''use crate::audio::AudioPortIndex;

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

'''
buffer_path.write_text(text[:start] + replacement + text[end:])

# Remove the now-impossible unresolved-endpoint integration test.
endpoint_test = root / 'crates/chassis-core/tests/audio_endpoint_runtime.rs'
text = endpoint_test.read_text()
text, count = re.subn(
    r'\n#\[test\]\nfn unresolved_endpoint_is_rejected_before_product_dsp\(\) \{.*?\n\}\n(?=\n#\[test\])',
    '',
    text,
    flags=re.S,
)
if count != 1:
    raise SystemExit(f'expected one unresolved endpoint test, found {count}')
endpoint_test.write_text(text)

# All resolved endpoint construction becomes dense-index construction.
resolved_counts = {'input': 0, 'output': 0}
for path in root.rglob('*.rs'):
    text = path.read_text()
    text, n = re.subn(
        r'InputEndpoint::resolved\(\s*[^,]+,\s*([^,]+),\s*([^)]+)\)',
        r'InputEndpoint::new(\1, \2)',
        text,
    )
    resolved_counts['input'] += n
    text, n = re.subn(
        r'OutputEndpoint::resolved\(\s*[^,]+,\s*([^,]+),\s*([^)]+)\)',
        r'OutputEndpoint::new(\1, \2)',
        text,
    )
    resolved_counts['output'] += n

    # Proof-era key-only unit fixtures use the conventional schema order.
    text = text.replace(
        'InputEndpoint::new(MAIN_INPUT, ',
        'InputEndpoint::new(AudioPortIndex::new(0), ',
    )
    text = text.replace(
        'OutputEndpoint::new(MAIN_OUTPUT, ',
        'OutputEndpoint::new(AudioPortIndex::new(1), ',
    )

    # Dense identity is no longer optional.
    text = re.sub(
        r'endpoint\.port_index\(\) == Some\((self\.[A-Za-z0-9_]+)\)',
        r'endpoint.port_index() == \1',
        text,
    )
    path.write_text(text)

if resolved_counts['input'] == 0 or resolved_counts['output'] == 0:
    raise SystemExit(f'expected resolved endpoint call sites: {resolved_counts}')

# Runtime validation consumes the dense value directly.
runtime_path = root / 'crates/chassis-core/src/runtime.rs'
text = runtime_path.read_text()
text, runtime_count = re.subn(
    r'let port = endpoint\s*\.port_index\(\)\s*\.ok_or\(AudioEndpointError::UnresolvedPortIdentity\)\?;',
    'let port = endpoint.port_index();',
    text,
)
if runtime_count != 2:
    raise SystemExit(f'expected two unresolved runtime checks, found {runtime_count}')
runtime_path.write_text(text)

# The unresolved state is no longer representable by the endpoint type.
audio_path = root / 'crates/chassis-core/src/audio.rs'
text = audio_path.read_text()
old_variant = (
    '    /// The endpoint still uses the proof-era stable-key-only representation.\n'
    '    UnresolvedPortIdentity,\n'
)
if old_variant not in text:
    raise SystemExit('missing unresolved endpoint error variant')
text = text.replace(old_variant, '')
old_display = (
    '            Self::UnresolvedPortIdentity => {\n'
    '                formatter.write_str("audio endpoint has no resolved AudioPortIndex")\n'
    '            }\n'
)
if old_display not in text:
    raise SystemExit('missing unresolved endpoint display arm')
text = text.replace(old_display, '')
audio_path.write_text(text)

# Update imports and the endpoint unit assertion after the API cut.
buffer_path = root / 'crates/chassis-core/src/buffer.rs'
text = buffer_path.read_text()
text = text.replace('    use crate::audio::{MAIN_INPUT, MAIN_OUTPUT};\n', '')
text = text.replace(
    'assert_eq!(input.port_index(), Some(AudioPortIndex::new(0)));',
    'assert_eq!(input.port_index(), AudioPortIndex::new(0));',
)
text = text.replace(
    'assert_eq!(output.port_index(), Some(AudioPortIndex::new(1)));',
    'assert_eq!(output.port_index(), AudioPortIndex::new(1));',
)
buffer_path.write_text(text)

process_path = root / 'crates/chassis-core/src/process.rs'
text = process_path.read_text()
text = text.replace(
    'audio::{DEFAULT_EFFECT_CONFIGURATION, MAIN_INPUT, MAIN_OUTPUT},',
    'audio::{AudioPortIndex, DEFAULT_EFFECT_CONFIGURATION},',
)
process_path.write_text(text)

for relative in [
    'crates/chassis-core/tests/telemetry_runtime.rs',
    'crates/chassis-core/tests/realtime_alloc.rs',
]:
    path = root / relative
    text = path.read_text()
    text = text.replace(
        'audio::{AudioPortIndex, DEFAULT_EFFECT_CONFIGURATION, MAIN_INPUT, MAIN_OUTPUT},',
        'audio::{AudioPortIndex, DEFAULT_EFFECT_CONFIGURATION},',
    )
    path.write_text(text)

endpoint_test = root / 'crates/chassis-core/tests/audio_endpoint_runtime.rs'
text = endpoint_test.read_text()
text = text.replace(
    'AudioEndpointError, AudioPortIndex, DEFAULT_EFFECT_CONFIGURATION, MAIN_INPUT, MAIN_OUTPUT,\n        SIDECHAIN_INPUT,',
    'AudioEndpointError, AudioPortIndex, DEFAULT_EFFECT_CONFIGURATION,',
)
endpoint_test.write_text(text)

# No stable-key or optional-index endpoint API should remain.
offenders = []
for path in root.rglob('*.rs'):
    text = path.read_text()
    if 'InputEndpoint::resolved(' in text or 'OutputEndpoint::resolved(' in text:
        offenders.append(str(path))
    if 'port_index() == Some(' in text:
        offenders.append(str(path))
if offenders:
    raise SystemExit(f'unmigrated dense endpoint call sites: {sorted(set(offenders))}')
