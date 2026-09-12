from pathlib import Path
import re

root = Path('.')

# Add an identified conventional-effect helper while keeping the identity-less
# helper for transient/embedded components that do not participate in state yet.
schema_path = root / 'crates/chassis-core/src/schema.rs'
text = schema_path.read_text()
anchor = '''    pub fn stereo_effect(
        parameters: Vec<ParameterDescriptor>,
    ) -> Result<Self, ComponentSchemaError> {
        Self::unidentified(DEFAULT_EFFECT_PORTS.to_vec(), Vec::new(), parameters)
    }
'''
helper = anchor + '''
    /// Construct a conventional stereo-effect schema with semantic state identity.
    ///
    /// # Errors
    ///
    /// Returns [`ComponentSchemaError`] when the supplied parameter schema is invalid.
    pub fn stereo_effect_with_state(
        id: ComponentId,
        state_schema: StateSchemaVersion,
        parameters: Vec<ParameterDescriptor>,
    ) -> Result<Self, ComponentSchemaError> {
        Self::new(
            id,
            state_schema,
            DEFAULT_EFFECT_PORTS.to_vec(),
            Vec::new(),
            parameters,
        )
    }
'''
if anchor not in text:
    raise SystemExit('stereo effect helper changed unexpectedly')
text = text.replace(anchor, helper)
schema_path.write_text(text)

# CLAP parameter-state projection consumes one semantic StateIdentity rather than
# caller-supplied product ID/version pairs.
parameters_path = root / 'crates/chassis-clap/src/parameters.rs'
text = parameters_path.read_text()
text = text.replace(
    '    parameters::{\n        ChoiceId, ChoiceOption, ParameterDescriptor, ParameterIndex, ParameterKind, ParameterValue,\n        ParameterValuesMut,\n    },\n',
    '    parameters::{\n        ChoiceId, ChoiceOption, ParameterDescriptor, ParameterIndex, ParameterKind, ParameterValue,\n        ParameterValuesMut,\n    },\n    schema::StateIdentity,\n',
)
old = '''    pub(crate) fn encode_state(
        &self,
        product_id: &str,
        product_schema: u32,
        limits: StateLimits,
    ) -> Result<Vec<u8>, ParameterStateError> {
        let mut values = vec![0.0; self.publication.len()];
        self.publication
            .snapshot_control_into(&mut values)
            .map_err(publication_state_error)?;
        let mut document = StateDocument::new(product_id, product_schema)
            .map_err(ParameterStateError::Document)?;
'''
new = '''    pub(crate) fn encode_state(
        &self,
        identity: &StateIdentity,
        limits: StateLimits,
    ) -> Result<Vec<u8>, ParameterStateError> {
        let mut values = vec![0.0; self.publication.len()];
        self.publication
            .snapshot_control_into(&mut values)
            .map_err(publication_state_error)?;
        let mut document = StateDocument::new(
            identity.component().as_str(),
            identity.schema().get(),
        )
        .map_err(ParameterStateError::Document)?;
'''
if old not in text:
    raise SystemExit('CLAP encode_state shape changed unexpectedly')
text = text.replace(old, new)
old = '''    pub(crate) fn apply_state(
        &self,
        document: &StateDocument,
        product_id: &str,
        product_schema: u32,
    ) -> Result<(), ParameterStateError> {
        if document.product_id() != product_id {
            return Err(ParameterStateError::ProductIdentityMismatch);
        }
        if document.product_schema() != product_schema {
            return Err(ParameterStateError::ProductSchemaMismatch);
        }
'''
new = '''    pub(crate) fn apply_state(
        &self,
        document: &StateDocument,
        identity: &StateIdentity,
    ) -> Result<(), ParameterStateError> {
        if document.product_id() != identity.component().as_str() {
            return Err(ParameterStateError::ProductIdentityMismatch);
        }
        if document.product_schema() != identity.schema().get() {
            return Err(ParameterStateError::ProductSchemaMismatch);
        }
'''
if old not in text:
    raise SystemExit('CLAP apply_state shape changed unexpectedly')
text = text.replace(old, new)

# Parameter-state unit tests use the same semantic identity object.
test_import = '''        parameters::{ChoiceId, ChoiceOption, ParameterDescriptor, ParameterStore},
        state::StateEntry,
'''
test_import_new = '''        parameters::{ChoiceId, ChoiceOption, ParameterDescriptor, ParameterStore},
        schema::{ComponentId, StateSchemaVersion},
        state::StateEntry,
'''
if test_import not in text:
    raise SystemExit('parameter test import anchor missing')
text = text.replace(test_import, test_import_new)
helper_anchor = '''    fn descriptors() -> Vec<ParameterDescriptor> {
'''
identity_helper = '''    fn state_identity() -> StateIdentity {
        StateIdentity::new(
            ComponentId::new("com.example.test").expect("semantic identity is valid"),
            StateSchemaVersion::new(1),
        )
    }

'''
if helper_anchor not in text:
    raise SystemExit('parameter test helper anchor missing')
text = text.replace(helper_anchor, identity_helper + helper_anchor, 1)
text = text.replace(
    '.encode_state("com.example.test", 1, StateLimits::default())',
    '.encode_state(&state_identity(), StateLimits::default())',
)
text = text.replace(
    'state.apply_state(&document, "com.example.test", 1)',
    'state.apply_state(&document, &state_identity())',
)
text = text.replace(
    '.apply_state(&document, "com.example.test", 1)',
    '.apply_state(&document, &state_identity())',
)
parameters_path.write_text(text)

# CLAP shared/main-thread state retains the coherent schema snapshot. Deployment
# ID remains descriptor metadata only; semantic persistence identity comes from
# ComponentSchema.
lib_path = root / 'crates/chassis-clap/src/lib.rs'
text = lib_path.read_text()
text = text.replace(
    '    runtime::{Component, InstanceRuntime, Process as ChassisProcess, Processor},\n',
    '    runtime::{Component, InstanceRuntime, Process as ChassisProcess, Processor},\n    schema::ComponentSchema,\n',
)
text = text.replace(
    '''    /// Chassis parameter-state schema version projected through CLAP state.\n    const CLAP_STATE_SCHEMA: u32 = 1;\n''',
    '',
)
old = '''pub struct ChassisShared {
    parameters: Arc<ClapParameterState>,
    render: Arc<RenderState>,
}
'''
new = '''pub struct ChassisShared {
    schema: Arc<ComponentSchema>,
    parameters: Arc<ClapParameterState>,
    render: Arc<RenderState>,
}
'''
if old not in text:
    raise SystemExit('ChassisShared shape changed unexpectedly')
text = text.replace(old, new)
old = '''pub struct ChassisMainThread<'host, C> {
    component: C,
    audio: ClapAudioConfiguration,
    shared: Arc<ClapParameterState>,
'''
new = '''pub struct ChassisMainThread<'host, C> {
    component: C,
    schema: Arc<ComponentSchema>,
    audio: ClapAudioConfiguration,
    shared: Arc<ClapParameterState>,
'''
if old not in text:
    raise SystemExit('ChassisMainThread shape changed unexpectedly')
text = text.replace(old, new)

old = '''    if !schema.parameters().is_empty() && C::CLAP_MAX_PARAMETER_EVENTS == 0 {
'''
new = '''    if schema.state_identity().is_none() {
        return Err(PluginError::Message(
            "CLAP state projection requires semantic component state identity",
        ));
    }
    if !schema.parameters().is_empty() && C::CLAP_MAX_PARAMETER_EVENTS == 0 {
'''
if old not in text:
    raise SystemExit('new_shared schema anchor missing')
text = text.replace(old, new, 1)
old = '''    Ok(ChassisShared {
        parameters: Arc::new(parameters),
        render: Arc::new(RenderState::default()),
    })
'''
new = '''    Ok(ChassisShared {
        schema: Arc::new(schema),
        parameters: Arc::new(parameters),
        render: Arc::new(RenderState::default()),
    })
'''
if old not in text:
    raise SystemExit('new_shared return shape changed unexpectedly')
text = text.replace(old, new)

old = '''    if !shared.parameters.matches_descriptors(schema.parameters()) {
        return Err(PluginError::Message(
            "CLAP component schema changed between shared and main-thread construction",
        ));
    }
    let mut audio = ClapAudioConfiguration::new(schema.audio_ports(), C::CLAP_AUDIO_PORTS)
'''
new = '''    if schema != *shared.schema {
        return Err(PluginError::Message(
            "CLAP component schema changed between shared and main-thread construction",
        ));
    }
    let mut audio = ClapAudioConfiguration::new(shared.schema.audio_ports(), C::CLAP_AUDIO_PORTS)
'''
if old not in text:
    raise SystemExit('new_main_thread schema comparison changed unexpectedly')
text = text.replace(old, new)
old = '''    Ok(ChassisMainThread {
        component,
        audio,
        shared: Arc::clone(&shared.parameters),
'''
new = '''    Ok(ChassisMainThread {
        component,
        schema: Arc::clone(&shared.schema),
        audio,
        shared: Arc::clone(&shared.parameters),
'''
if old not in text:
    raise SystemExit('main-thread construction shape changed unexpectedly')
text = text.replace(old, new)

old = '''    let schema = main_thread
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
'''
new = '''    let process = map_process_config(audio_config, C::CLAP_MAX_PARAMETER_EVENTS)?;
    let runtime = InstanceRuntime::from_schema((*main_thread.schema).clone())
'''
if old not in text:
    raise SystemExit('activate_processor schema construction changed unexpectedly')
text = text.replace(old, new)

old = '''        let encoded = self
            .shared
            .encode_state(C::CLAP_ID, C::CLAP_STATE_SCHEMA, StateLimits::default())
            .map_err(|error| state_error(&error))?;
'''
new = '''        let identity = self
            .schema
            .state_identity()
            .ok_or(PluginError::Message("CLAP state identity is unavailable"))?;
        let encoded = self
            .shared
            .encode_state(identity, StateLimits::default())
            .map_err(|error| state_error(&error))?;
'''
if old not in text:
    raise SystemExit('CLAP state save shape changed unexpectedly')
text = text.replace(old, new)
old = '''        self.shared
            .apply_state(&document, C::CLAP_ID, C::CLAP_STATE_SCHEMA)
            .map_err(|error| state_error(&error))?;
'''
new = '''        let identity = self
            .schema
            .state_identity()
            .ok_or(PluginError::Message("CLAP state identity is unavailable"))?;
        self.shared
            .apply_state(&document, identity)
            .map_err(|error| state_error(&error))?;
'''
if old not in text:
    raise SystemExit('CLAP state load shape changed unexpectedly')
text = text.replace(old, new)
lib_path.write_text(text)

# Parse Rust impl blocks at column zero.
def impl_blocks(text: str, prefix: str):
    lines = text.splitlines(keepends=True)
    offsets = []
    pos = 0
    for line in lines:
        offsets.append(pos)
        pos += len(line)
    blocks = []
    for i, line in enumerate(lines):
        if line.startswith(prefix) and line.rstrip().endswith('{'):
            for j in range(i + 1, len(lines)):
                if lines[j].strip() == '}' and not lines[j].startswith((' ', '\t')):
                    blocks.append((offsets[i], offsets[j] + len(lines[j]), line))
                    break
            else:
                raise SystemExit(f'unterminated impl in line {i + 1}')
    return blocks

# Replace one conventional schema helper call while preserving its parameter expression.
def replace_stereo_call(block: str, semantic_id: str) -> str:
    needle = 'chassis_core::schema::ComponentSchema::stereo_effect('
    start = block.find(needle)
    if start < 0:
        raise SystemExit('CLAP Component schema is not a conventional stereo-effect helper')
    open_paren = start + len(needle) - 1
    depth = 0
    close = None
    for index in range(open_paren, len(block)):
        char = block[index]
        if char == '(':
            depth += 1
        elif char == ')':
            depth -= 1
            if depth == 0:
                close = index
                break
    if close is None:
        raise SystemExit('unterminated stereo_effect call')
    argument = block[open_paren + 1:close].strip()
    replacement = f'''chassis_core::schema::ComponentSchema::stereo_effect_with_state(
            chassis_core::schema::ComponentId::new("{semantic_id}")
                .expect("semantic component identity is valid"),
            chassis_core::schema::StateSchemaVersion::new(1),
            {argument},
        )'''
    return block[:start] + replacement + block[close + 1:]

# Every CLAP export gets an explicit semantic identity independent of CLAP_ID.
# Fixture IDs are derived from source path/type, not from the deployment ID.
for path in root.rglob('*.rs'):
    text = path.read_text()
    clap_types = []
    for start, end, line in impl_blocks(text, 'impl ClapStereoEffect for '):
        match = re.match(r'impl ClapStereoEffect for ([A-Za-z_][A-Za-z0-9_]*) \{', line.strip())
        if match:
            clap_types.append(match.group(1))
    if not clap_types:
        continue

    relative = path.relative_to(root).with_suffix('')
    path_slug = '.'.join(part.replace('_', '-').lower() for part in relative.parts)
    for type_name in clap_types:
        blocks = impl_blocks(text, f'impl Component for {type_name} ')
        if len(blocks) != 1:
            raise SystemExit(f'expected one Component impl for CLAP export {type_name} in {path}')
        start, end, _ = blocks[0]
        block = text[start:end]
        semantic_id = f'org.nijaru.chassis.semantic.{path_slug}.{type_name.lower()}'
        block = replace_stereo_call(block, semantic_id)
        text = text[:start] + block + text[end:]
    path.write_text(text)

# Host restart constructs state explicitly; derive identity from Component schema
# rather than deployment constants.
restart_path = root / 'crates/chassis-clap/tests/host_restart.rs'
text = restart_path.read_text()
old = '''    let mut document =
        StateDocument::new(DelayedProbe::CLAP_ID, DelayedProbe::CLAP_STATE_SCHEMA).unwrap();
'''
new = '''    let schema = DelayedProbe::default().schema().unwrap();
    let identity = schema.state_identity().unwrap();
    let mut document = StateDocument::new(
        identity.component().as_str(),
        identity.schema().get(),
    )
    .unwrap();
'''
if old not in text:
    raise SystemExit('host restart explicit CLAP state identity changed unexpectedly')
restart_path.write_text(text.replace(old, new))

# Update stale adapter trait docs after the direct Component schema migration.
lib_path = root / 'crates/chassis-clap/src/lib.rs'
text = lib_path.read_text()
text = text.replace(
    '    /// The mapping must contain exactly one entry for every descriptor returned\n    /// by [`Component::parameter_descriptors`]. Float, integer, boolean, and\n',
    '    /// The mapping must contain exactly one entry for every parameter in the\n    /// component [`ComponentSchema`]. Float, integer, boolean, and\n',
)
lib_path.write_text(text)

# Fail closed if deployment-state identity remains or a CLAP-exported component is
# still unidentified.
offenders = []
for path in root.rglob('*.rs'):
    text = path.read_text()
    if 'CLAP_STATE_SCHEMA' in text:
        offenders.append(f'{path}: CLAP_STATE_SCHEMA')
    if '.encode_state(C::CLAP_ID' in text or '.apply_state(&document, C::CLAP_ID' in text:
        offenders.append(f'{path}: deployment state identity')
    for start, end, line in impl_blocks(text, 'impl ClapStereoEffect for '):
        match = re.match(r'impl ClapStereoEffect for ([A-Za-z_][A-Za-z0-9_]*) \{', line.strip())
        if not match:
            continue
        type_name = match.group(1)
        component_blocks = impl_blocks(text, f'impl Component for {type_name} ')
        if len(component_blocks) != 1:
            offenders.append(f'{path}: missing Component impl for {type_name}')
            continue
        cstart, cend, _ = component_blocks[0]
        if 'stereo_effect_with_state(' not in text[cstart:cend]:
            offenders.append(f'{path}: unidentified CLAP component {type_name}')
if offenders:
    raise SystemExit('state identity migration incomplete: ' + '; '.join(offenders))
