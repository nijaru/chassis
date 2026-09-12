from pathlib import Path
import re

root = Path('.')

# Add an explicit conventional-effect helper above the neutral Component trait.
schema_path = root / 'crates/chassis-core/src/schema.rs'
text = schema_path.read_text()
text = text.replace(
    'AudioIoConfigurationError, AudioPortDescriptor, AudioPortIndex, PortKey, audio_port_index,\n        validate_audio_port_schema,',
    'AudioIoConfigurationError, AudioPortDescriptor, AudioPortIndex, DEFAULT_EFFECT_PORTS, PortKey,\n        audio_port_index, validate_audio_port_schema,',
)
anchor = '''    pub fn unidentified(
        audio_ports: Vec<AudioPortDescriptor>,
        event_ports: Vec<EventPortDescriptor>,
        parameters: Vec<ParameterDescriptor>,
    ) -> Result<Self, ComponentSchemaError> {
        Self::build(None, audio_ports, event_ports, parameters)
    }
'''
helper = anchor + '''
    /// Construct the conventional stereo-effect schema with optional sidechain.
    ///
    /// This is an authoring convenience above neutral core semantics; components
    /// with different audio/event I/O construct their schema explicitly.
    ///
    /// # Errors
    ///
    /// Returns [`ComponentSchemaError`] when the supplied parameter schema is invalid.
    pub fn stereo_effect(
        parameters: Vec<ParameterDescriptor>,
    ) -> Result<Self, ComponentSchemaError> {
        Self::unidentified(DEFAULT_EFFECT_PORTS.to_vec(), Vec::new(), parameters)
    }
'''
if anchor not in text:
    raise SystemExit('ComponentSchema unidentified constructor changed unexpectedly')
text = text.replace(anchor, helper)
schema_path.write_text(text)

# Component::schema() becomes the only component metadata authority.
runtime_path = root / 'crates/chassis-core/src/runtime.rs'
text = runtime_path.read_text()
start = text.index('    /// Return the stable audio-port schema for this component.')
end = text.index('    /// Create the exclusively-owned realtime processor for one activation.', start)
replacement = '''    /// Build one coherent immutable component schema for this generation.
    ///
    /// Schema construction is a setup/control-domain operation. The runtime
    /// validates and owns the returned snapshot; activation receives that exact
    /// frozen generation through [`ActivationConfig::schema`].
    ///
    /// # Errors
    ///
    /// Returns [`ComponentSchemaError`] when any immutable schema is invalid.
    fn schema(&self) -> Result<ComponentSchema, ComponentSchemaError>;

'''
text = text[:start] + replacement + text[end:]

# Delete proof-order runtime constructors now that callers use coherent schemas.
start = text.index('    /// Historical pre-alpha constructor for a conventional-effect schema.')
end = text.index("    /// Construct one inactive runtime from a component's coherent immutable schema.", start)
text = text[:start] + text[end:]
text = text.replace('        AudioPortIndex, ConfiguredAudioPort, DEFAULT_EFFECT_PORTS, PortDirection, PortKey,\n',
                    '        AudioPortIndex, ConfiguredAudioPort, PortDirection, PortKey,\n')
text = text.replace('        ParameterDescriptor, ParameterStateError, ParameterStore, ParameterStoreError,\n',
                    '        ParameterStateError, ParameterStore, ParameterStoreError,\n')
runtime_path.write_text(text)

# Nonstandard component schemas: zero-audio event processors and the explicit
# schema-drift fixture.
note_path = root / 'crates/chassis-core/tests/note_runtime.rs'
text = note_path.read_text()
old = '''    fn audio_ports(&self) -> &[AudioPortDescriptor] {
        &[]
    }

    fn event_ports(&self) -> &[EventPortDescriptor] {
        NOTE_PORTS
    }
'''
new = '''    fn schema(
        &self,
    ) -> Result<chassis_core::schema::ComponentSchema, chassis_core::schema::ComponentSchemaError> {
        chassis_core::schema::ComponentSchema::unidentified(
            vec![],
            NOTE_PORTS.to_vec(),
            vec![],
        )
    }
'''
if old not in text:
    raise SystemExit('note runtime legacy schema methods changed unexpectedly')
text = text.replace(old, new)
text = text.replace('audio::{AudioIoConfiguration, AudioPortDescriptor},', 'audio::AudioIoConfiguration,')
note_path.write_text(text)

event_path = root / 'crates/chassis-core/tests/event_schema_runtime.rs'
text = event_path.read_text()
old = '''    fn audio_ports(&self) -> &[AudioPortDescriptor] {
        &[]
    }

    fn event_ports(&self) -> &[EventPortDescriptor] {
        self.ports
    }
'''
new = '''    fn schema(
        &self,
    ) -> Result<chassis_core::schema::ComponentSchema, chassis_core::schema::ComponentSchemaError> {
        chassis_core::schema::ComponentSchema::unidentified(
            vec![],
            self.ports.to_vec(),
            vec![],
        )
    }
'''
if old not in text:
    raise SystemExit('event schema runtime legacy methods changed unexpectedly')
text = text.replace(old, new)
text = text.replace('audio::{AudioIoConfiguration, AudioPortDescriptor},', 'audio::AudioIoConfiguration,')
event_path.write_text(text)

component_schema_path = root / 'crates/chassis-core/tests/component_schema_runtime.rs'
text = component_schema_path.read_text()
old = '''    fn audio_ports(&self) -> &[AudioPortDescriptor] {
        self.audio_ports
    }

'''
if old not in text:
    raise SystemExit('component schema runtime legacy audio method changed unexpectedly')
text = text.replace(old, '')
component_schema_path.write_text(text)

# Migrate all remaining Component implementations. Rustfmt keeps impl-closing
# braces at column zero, which gives us a stable block boundary here.
def component_blocks(text: str):
    lines = text.splitlines(keepends=True)
    offsets = []
    pos = 0
    for line in lines:
        offsets.append(pos)
        pos += len(line)
    blocks = []
    for i, line in enumerate(lines):
        if line.startswith('impl Component for ') and line.rstrip().endswith('{'):
            for j in range(i + 1, len(lines)):
                if lines[j].strip() == '}' and not lines[j].startswith((' ', '\t')):
                    blocks.append((offsets[i], offsets[j] + len(lines[j])))
                    break
            else:
                raise SystemExit(f'unterminated Component impl at line {i + 1}')
    return blocks

param_method = re.compile(
    r'\n    fn parameter_descriptors\(&self\) -> &\[ParameterDescriptor\] \{\n'
    r'        &self\.([A-Za-z_][A-Za-z0-9_]*)\n'
    r'    \}\n'
)

for path in root.rglob('*.rs'):
    text = path.read_text()
    blocks = component_blocks(text)
    if not blocks:
        continue
    for start, end in reversed(blocks):
        block = text[start:end]
        match = param_method.search(block)
        parameter_field = None
        if match:
            parameter_field = match.group(1)
            block = block[:match.start()] + '\n' + block[match.end():]
        elif 'fn parameter_descriptors(&self)' in block:
            raise SystemExit(f'unhandled parameter descriptor method in {path}')

        if 'fn audio_ports(&self)' in block or 'fn event_ports(&self)' in block:
            raise SystemExit(f'legacy audio/event schema method remains in {path}')

        if 'fn schema(' not in block:
            activation = re.search(r'(    type ActivationError = .*?;\n)', block)
            if not activation:
                raise SystemExit(f'no ActivationError anchor in {path}')
            parameters = f'self.{parameter_field}.clone()' if parameter_field else 'vec![]'
            method = f'''\n    fn schema(\n        &self,\n    ) -> Result<chassis_core::schema::ComponentSchema, chassis_core::schema::ComponentSchemaError> {{\n        chassis_core::schema::ComponentSchema::stereo_effect({parameters})\n    }}\n'''
            insert_at = activation.end()
            block = block[:insert_at] + method + block[insert_at:]

        text = text[:start] + block + text[end:]
    path.write_text(text)

# CLAP consumes one coherent schema projection instead of separate Component methods.
clap_path = root / 'crates/chassis-clap/src/lib.rs'
text = clap_path.read_text()
old = '''    let component = C::default();
    ParameterStore::new(component.parameter_descriptors())
        .map_err(|_| PluginError::Message("Invalid Chassis parameter schema"))?;
    if !component.parameter_descriptors().is_empty() && C::CLAP_MAX_PARAMETER_EVENTS == 0 {
        return Err(PluginError::Message(
            "CLAP parameter projection requires a positive event bound",
        ));
    }
    if C::CLAP_MAX_PARAMETER_EVENTS > C::CLAP_MAX_INPUT_EVENTS {
        return Err(PluginError::Message(
            "CLAP parameter event bound exceeds total input event bound",
        ));
    }
    let parameters =
        ClapParameterState::new(component.parameter_descriptors(), C::CLAP_PARAMETER_IDS)
            .map_err(|error| parameter_mapping_error(&error))?
            .with_input_event_bound(C::CLAP_MAX_INPUT_EVENTS);
'''
new = '''    let component = C::default();
    let schema = component
        .schema()
        .map_err(|_| PluginError::Message("Invalid Chassis component schema"))?;
    if !schema.parameters().is_empty() && C::CLAP_MAX_PARAMETER_EVENTS == 0 {
        return Err(PluginError::Message(
            "CLAP parameter projection requires a positive event bound",
        ));
    }
    if C::CLAP_MAX_PARAMETER_EVENTS > C::CLAP_MAX_INPUT_EVENTS {
        return Err(PluginError::Message(
            "CLAP parameter event bound exceeds total input event bound",
        ));
    }
    let parameters = ClapParameterState::new(schema.parameters(), C::CLAP_PARAMETER_IDS)
        .map_err(|error| parameter_mapping_error(&error))?
        .with_input_event_bound(C::CLAP_MAX_INPUT_EVENTS);
'''
if old not in text:
    raise SystemExit('CLAP shared schema construction changed unexpectedly')
text = text.replace(old, new)

old = '''    let component = C::default();
    if !shared
        .parameters
        .matches_descriptors(component.parameter_descriptors())
    {
        return Err(PluginError::Message(
            "CLAP component schema changed between shared and main-thread construction",
        ));
    }
    let mut audio = ClapAudioConfiguration::new(component.audio_ports(), C::CLAP_AUDIO_PORTS)
        .map_err(|error| audio_mapping_error(&error))?;
'''
new = '''    let component = C::default();
    let schema = component
        .schema()
        .map_err(|_| PluginError::Message("Invalid Chassis component schema"))?;
    if !shared.parameters.matches_descriptors(schema.parameters()) {
        return Err(PluginError::Message(
            "CLAP component schema changed between shared and main-thread construction",
        ));
    }
    let mut audio = ClapAudioConfiguration::new(schema.audio_ports(), C::CLAP_AUDIO_PORTS)
        .map_err(|error| audio_mapping_error(&error))?;
'''
if old not in text:
    raise SystemExit('CLAP main-thread schema construction changed unexpectedly')
text = text.replace(old, new)

old = '''    if !shared
        .parameters
        .matches_descriptors(main_thread.component.parameter_descriptors())
    {
        return Err(PluginError::Message(
            "CLAP component schema does not match its parameter projection",
        ));
    }
    let process = map_process_config(audio_config, C::CLAP_MAX_PARAMETER_EVENTS)?;
    let runtime = InstanceRuntime::for_component(&main_thread.component)
        .map_err(|_| PluginError::Message("Invalid Chassis instance runtime"))?;
'''
new = '''    let schema = main_thread
        .component
        .schema()
        .map_err(|_| PluginError::Message("Invalid Chassis component schema"))?;
    if !shared.parameters.matches_descriptors(schema.parameters()) {
        return Err(PluginError::Message(
            "CLAP component schema does not match its parameter projection",
        ));
    }
    let process = map_process_config(audio_config, C::CLAP_MAX_PARAMETER_EVENTS)?;
    let runtime = InstanceRuntime::from_schema(schema)
        .map_err(|_| PluginError::Message("Invalid Chassis instance runtime"))?;
'''
if old not in text:
    raise SystemExit('CLAP activation schema check changed unexpectedly')
text = text.replace(old, new)
clap_path.write_text(text)

# Sanity checks: no Component impl may retain proof-era metadata methods, and no
# Rust caller may use the removed runtime constructors/component methods.
for path in root.rglob('*.rs'):
    text = path.read_text()
    for start, end in component_blocks(text):
        block = text[start:end]
        if 'fn schema(' not in block:
            raise SystemExit(f'Component impl without schema in {path}')
        for legacy in ('fn audio_ports(&self)', 'fn event_ports(&self)', 'fn parameter_descriptors(&self)'):
            if legacy in block:
                raise SystemExit(f'{legacy} remains in Component impl {path}')
    if '.parameter_descriptors()' in text:
        raise SystemExit(f'legacy component parameter query remains in {path}')
    if 'InstanceRuntime::new(' in text or 'new_with_event_ports(' in text:
        raise SystemExit(f'proof-era runtime constructor remains in {path}')
