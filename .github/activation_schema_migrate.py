from pathlib import Path
import re

root = Path('.')

# ActivationConfig carries the exact validated runtime schema generation.
process_path = root / 'crates/chassis-core/src/process.rs'
text = process_path.read_text()
text = text.replace(
    '    parameters::{ParameterAutomationError, ParameterStore},\n};',
    '    parameters::{ParameterAutomationError, ParameterStore},\n    schema::ComponentSchema,\n};',
)
old_struct = '''pub struct ActivationConfig<'a> {
    process: ProcessConfig,
    audio_io: AudioIoConfiguration<'a>,
}

impl<'a> ActivationConfig<'a> {
    pub(crate) const fn new(process: ProcessConfig, audio_io: AudioIoConfiguration<'a>) -> Self {
        Self { process, audio_io }
    }
'''
new_struct = '''pub struct ActivationConfig<'a> {
    process: ProcessConfig,
    audio_io: AudioIoConfiguration<'a>,
    schema: &'a ComponentSchema,
}

impl<'a> ActivationConfig<'a> {
    pub(crate) const fn new(
        process: ProcessConfig,
        audio_io: AudioIoConfiguration<'a>,
        schema: &'a ComponentSchema,
    ) -> Self {
        Self {
            process,
            audio_io,
            schema,
        }
    }
'''
if old_struct not in text:
    raise SystemExit('ActivationConfig shape changed unexpectedly')
text = text.replace(old_struct, new_struct)
old_accessor = '''    pub const fn audio_io(self) -> AudioIoConfiguration<'a> {
        self.audio_io
    }
}
'''
new_accessor = '''    pub const fn audio_io(self) -> AudioIoConfiguration<'a> {
        self.audio_io
    }

    /// Return the immutable component schema generation frozen by the runtime.
    #[must_use]
    pub const fn schema(self) -> &'a ComponentSchema {
        self.schema
    }
}
'''
if old_accessor not in text:
    raise SystemExit('ActivationConfig accessor block changed unexpectedly')
text = text.replace(old_accessor, new_accessor)

# Process unit tests need a validated schema behind ActivationConfig.
text = text.replace(
    'audio::{AudioPortIndex, DEFAULT_EFFECT_CONFIGURATION},',
    'audio::{AudioPortIndex, DEFAULT_EFFECT_CONFIGURATION, DEFAULT_EFFECT_PORTS},',
)
helper_anchor = '''    fn empty_parameters() -> ParameterStore {
        ParameterStore::new(&[]).expect("empty parameter schema is valid")
    }
'''
helper = helper_anchor + '''
    fn default_effect_schema() -> ComponentSchema {
        ComponentSchema::unidentified(DEFAULT_EFFECT_PORTS.to_vec(), vec![], vec![])
            .expect("default effect schema is valid")
    }
'''
if helper_anchor not in text:
    raise SystemExit('process test helper anchor missing')
text = text.replace(helper_anchor, helper)
text, count = re.subn(
    r'let activation = ActivationConfig::new\(process, DEFAULT_EFFECT_CONFIGURATION\);',
    'let schema = default_effect_schema();\n        let activation = ActivationConfig::new(process, DEFAULT_EFFECT_CONFIGURATION, &schema);',
    text,
)
if count < 4:
    raise SystemExit(f'expected multiple process test activations, found {count}')
process_path.write_text(text)

# Runtime passes its frozen schema into all activation/process config views.
runtime_path = root / 'crates/chassis-core/src/runtime.rs'
text = runtime_path.read_text()
old_active = '''impl<P> ActiveRuntime<P> {
    fn config(&self) -> ActivationConfig<'_> {
        ActivationConfig::new(self.process, AudioIoConfiguration::new(&self.audio_ports))
    }
}
'''
new_active = '''impl<P> ActiveRuntime<P> {
    fn config<'a>(&'a self, schema: &'a ComponentSchema) -> ActivationConfig<'a> {
        ActivationConfig::new(
            self.process,
            AudioIoConfiguration::new(&self.audio_ports),
            schema,
        )
    }
}
'''
if old_active not in text:
    raise SystemExit('ActiveRuntime config shape changed unexpectedly')
text = text.replace(old_active, new_active)
text = text.replace(
    'self.active.as_ref().map(ActiveRuntime::config)',
    'self.active\n            .as_ref()\n            .map(|active| active.config(&self.schema))',
)
text = text.replace(
    'let config = ActivationConfig::new(process, AudioIoConfiguration::new(&audio_ports));',
    'let config = ActivationConfig::new(\n            process,\n            AudioIoConfiguration::new(&audio_ports),\n            &self.schema,\n        );',
)
text = text.replace(
    'let config = ActivationConfig::new(*process, AudioIoConfiguration::new(audio_ports));',
    'let config = ActivationConfig::new(\n            *process,\n            AudioIoConfiguration::new(audio_ports),\n            schema,\n        );',
)
runtime_path.write_text(text)

# Processors resolve semantic audio keys against the frozen runtime schema, not
# by re-querying Component metadata.
for path in root.rglob('*.rs'):
    text = path.read_text()
    original = text
    text = re.sub(
        r'audio_port_index\(self\.audio_ports\(\),\s*([A-Z][A-Z0-9_]*)\)',
        r'config.schema().audio_port_index(\1)',
        text,
    )
    if 'config.schema()' in text:
        # Methods that did not previously use activation config often named it _config.
        text = text.replace('_config: &ActivationConfig', 'config: &ActivationConfig')
    if text != original:
        path.write_text(text)

# The old standalone lookup import is no longer needed where all consumers moved
# to ComponentSchema::audio_port_index(). A method call such as
# `schema.audio_port_index(...)` does not count as a free-function use.
for path in root.rglob('*.rs'):
    text = path.read_text()
    if not re.search(r'(?<![.\w])audio_port_index\(', text):
        text = text.replace('audio_port_index, ', '')
        text = text.replace(', audio_port_index', '')
        path.write_text(text)

# Sanity: every crate-private ActivationConfig constructor now has a schema arg.
offenders = []
for path in root.rglob('*.rs'):
    text = path.read_text()
    for match in re.finditer(r'ActivationConfig::new\((.*?)\)', text, flags=re.S):
        # Constructor expressions may contain nested calls; this is only a quick
        # guard against the exact old two-argument spelling.
        if 'DEFAULT_EFFECT_CONFIGURATION)' in match.group(0):
            offenders.append(str(path))
if offenders:
    raise SystemExit(f'old ActivationConfig constructor remains: {sorted(set(offenders))}')
