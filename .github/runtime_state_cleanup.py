from pathlib import Path

root = Path('.')

# --- runtime public surface ------------------------------------------------------
runtime_path = root / 'crates/chassis-core/src/runtime.rs'
text = runtime_path.read_text()

# Remove the parameter-only load error; complete semantic-state loading is the
# runtime-level contract now.
start = text.index('/// Failure while decoding, migrating, or applying a complete parameter state.')
end = text.index('/// Failure while validating a complete semantic instance state.', start)
text = text[:start] + text[end:]

# Remove parameter-only export compatibility methods.
start = text.index('    /// Build a semantic document containing all durable parameter base values.')
end = text.index('    /// Build the complete durable semantic state using schema-owned identity.', start)
text = text[:start] + text[end:]

# Make complete export use schema identity directly and remove explicit-ID export.
start = text.index('    /// Build the complete durable semantic state using schema-owned identity.')
end = text.index('    /// Encode complete durable semantic state under schema-owned identity.', start)
new_state_document = '''    /// Build the complete durable semantic state using schema-owned identity.
    ///
    /// Framework-managed parameters are emitted from the canonical parameter
    /// store and validated custom entries are appended. Encoding remains
    /// deterministic because [`StateDocument`] canonicalizes entry order.
    ///
    /// # Errors
    ///
    /// Returns [`InstanceStateExportError::MissingStateIdentity`] when this
    /// component is intentionally identity-less, or wraps a parameter/state
    /// document error.
    pub fn state_document(&self) -> Result<StateDocument, InstanceStateExportError> {
        let (product_id, product_schema) = self
            .semantic_state_identity()
            .ok_or(InstanceStateExportError::MissingStateIdentity)?;
        let mut document = self
            .parameters
            .state_document(product_id, product_schema)
            .map_err(InstanceStateExportError::State)?;
        for entry in &self.custom_state {
            document
                .insert(entry.clone())
                .map_err(ParameterStateError::Document)
                .map_err(InstanceStateExportError::State)?;
        }
        Ok(document)
    }

'''
text = text[:start] + new_state_document + text[end:]

# Remove explicit-ID complete encoding compatibility method.
start = text.index('    /// Historical explicit-identity complete-state encoding.')
end = text.index('    fn parameter_state_candidate(', start)
text = text[:start] + text[end:]

# Remove parameter-only publication compatibility method.
start = text.index('    /// Replace durable parameter state transactionally from a complete document.')
end = text.index('    /// Replace complete semantic instance state using schema-owned identity.', start)
text = text[:start] + text[end:]

# Complete state publication performs the temporary-candidate transaction itself;
# remove the explicit-ID public wrapper.
start = text.index('    /// Replace complete semantic instance state using schema-owned identity.')
end = text.index('    /// Decode, migrate, and apply a complete parameter state transactionally.', start)
new_apply_state = '''    /// Replace complete semantic instance state using schema-owned identity.
    ///
    /// Framework parameters and custom entries are validated into temporary
    /// candidates. Product validation sees both candidates, and neither is
    /// published until every check succeeds.
    ///
    /// # Errors
    ///
    /// Returns [`InstanceSemanticStateError`] for missing identity, framework
    /// state validation failures, or product validation failure. Publication is
    /// atomic across parameter and custom state.
    pub fn apply_state<E, F>(
        &mut self,
        document: &StateDocument,
        validate_product_state: F,
    ) -> Result<(), InstanceSemanticStateError<E>>
    where
        F: FnOnce(&ParameterStore, &[StateEntry]) -> Result<(), E>,
    {
        let (product_id, product_schema) =
            self.semantic_state_identity()
                .ok_or(InstanceSemanticStateError::Parameters(
                    InstanceStateError::MissingStateIdentity,
                ))?;
        let candidate_parameters = self
            .parameter_state_candidate(document, &product_id, product_schema)
            .map_err(InstanceSemanticStateError::Parameters)?;
        let mut candidate_custom: Vec<_> = document
            .entries()
            .iter()
            .filter(|entry| !entry.key().starts_with("parameter/"))
            .cloned()
            .collect();
        candidate_custom
            .sort_unstable_by(|left, right| left.key().as_bytes().cmp(right.key().as_bytes()));
        validate_product_state(&candidate_parameters, &candidate_custom)
            .map_err(InstanceSemanticStateError::Product)?;

        self.parameters = candidate_parameters;
        self.custom_state = candidate_custom;
        Ok(())
    }

'''
text = text[:start] + new_apply_state + text[end:]

# Replace all parameter-only / explicit-ID byte-loading paths with the one
# schema-owned complete semantic-state loader.
start = text.index('    /// Decode, migrate, and apply a complete parameter state transactionally.')
end = text.index('\n}\n\nfn validate_audio_endpoints', start)
new_apply_bytes = '''    /// Decode, migrate, validate, and publish complete semantic state using
    /// schema-owned identity and schema version.
    ///
    /// # Errors
    ///
    /// Returns [`InstanceSemanticStateLoadError`] for missing state identity,
    /// bounded decode failures, migration failures, framework validation, or
    /// product validation. The runtime is unchanged on failure.
    pub fn apply_state_bytes<M, E, F>(
        &mut self,
        bytes: &[u8],
        limits: StateLimits,
        migrations: &[&M],
        validate_product_state: F,
    ) -> Result<(), InstanceSemanticStateLoadError<M::Error, E>>
    where
        M: StateMigration + ?Sized,
        M::Error: std::error::Error + 'static,
        E: std::error::Error + 'static,
        F: FnOnce(&ParameterStore, &[StateEntry]) -> Result<(), E>,
    {
        let target_schema = self
            .schema
            .state_identity()
            .map(|identity| identity.schema().get())
            .ok_or(InstanceSemanticStateLoadError::Apply(
                InstanceSemanticStateError::Parameters(InstanceStateError::MissingStateIdentity),
            ))?;
        let document = StateDocument::decode_with_limits(bytes, limits)
            .map_err(InstanceSemanticStateLoadError::Decode)?;
        let document = document
            .migrate_to(target_schema, migrations)
            .map_err(InstanceSemanticStateLoadError::Migration)?;
        self.apply_state(&document, validate_product_state)
            .map_err(InstanceSemanticStateLoadError::Apply)
    }
'''
text = text[:start] + new_apply_bytes + text[end:]

# Runtime-level compatibility names must be gone.
for stale in [
    'state_document_for_product',
    'encode_state_for_product',
    'apply_parameter_state_for_product',
    'apply_parameter_state_bytes',
    'apply_state_for_product',
    'apply_state_bytes_for_product',
    'InstanceStateLoadError',
    'parameter_state_document(',
    'encode_parameter_state(',
]:
    if stale in text:
        raise SystemExit(f'stale runtime state compatibility API remains: {stale}')

runtime_path.write_text(text)

# ActivationConfig now receives an already policy-accepted configuration.
process_path = root / 'crates/chassis-core/src/process.rs'
text = process_path.read_text()
text = text.replace(
    '''/// Product code receives this only after framework structural I/O validation.
/// Product-specific I/O policy remains an explicit future extension; this type
/// does not silently infer it.
''',
    '''/// Product code receives this only after framework structural validation and
/// the component schema's whole-audio-I/O policy have accepted the configuration.
''',
)
process_path.write_text(text)

# --- remove duplicated proof-era runtime state tests -----------------------------
instance_path = root / 'crates/chassis-core/tests/instance_runtime.rs'
text = instance_path.read_text()
text = text.replace(
    '''    runtime::{
        Component, InstanceRuntime, InstanceStateError, InstanceStateLoadError, Process, Processor,
    },
    state::{StateDocument, StateEntry, StateMigration, StateMigrationError, StateValue},
''',
    '''    runtime::{Component, InstanceRuntime, Process, Processor},
''',
)
start = text.index('const LEGACY_STATE_V1: &[u8] = &[')
end = text.index('fn process_config() -> ProcessConfig {', start)
text = text[:start] + text[end:]
start = text.index('#[test]\nfn complete_state_replacement_is_transactional()')
end = text.index('#[test]\nfn process_uses_durable_base_state()', start)
text = text[:start] + text[end:]
instance_path.write_text(text)

# --- adversarial bounded-load test moves to schema-owned semantic state ----------
adversarial_path = root / 'crates/chassis-core/tests/state_adversarial.rs'
text = adversarial_path.read_text()
text = text.replace(
    '''    parameters::{ParameterDescriptor, ParameterValue},
    runtime::{InstanceRuntime, Processor},
''',
    '''    parameters::{ParameterDescriptor, ParameterValue},
    runtime::{InstanceRuntime, Processor},
    schema::{ComponentId, ComponentSchema, StateSchemaVersion},
''',
)
text = text.replace(
    '''    let schema = chassis_core::schema::ComponentSchema::stereo_effect(vec![descriptor])
        .expect("runtime schema is valid");
''',
    '''    let schema = ComponentSchema::stereo_effect_with_state(
        ComponentId::new("com.example.effect").expect("semantic identity is valid"),
        StateSchemaVersion::new(1),
        vec![descriptor],
    )
    .expect("runtime schema is valid");
''',
)
old = '''            runtime
                .apply_parameter_state_bytes(
                    &encoded[..length],
                    "com.example.effect",
                    1,
                    StateLimits::default(),
                    &migrations,
                )
                .is_err(),
'''
new = '''            runtime
                .apply_state_bytes(
                    &encoded[..length],
                    StateLimits::default(),
                    &migrations,
                    |_, _| Ok::<(), Infallible>(()),
                )
                .is_err(),
'''
if old not in text:
    raise SystemExit('adversarial truncated load shape changed unexpectedly')
text = text.replace(old, new, 1)
old = '''    runtime
        .apply_parameter_state_bytes(
            &encoded,
            "com.example.effect",
            1,
            StateLimits::default(),
            &migrations,
        )
        .expect("complete replacement applies");
'''
new = '''    runtime
        .apply_state_bytes(
            &encoded,
            StateLimits::default(),
            &migrations,
            |_, _| Ok::<(), Infallible>(()),
        )
        .expect("complete replacement applies");
'''
if old not in text:
    raise SystemExit('adversarial complete load shape changed unexpectedly')
text = text.replace(old, new, 1)
adversarial_path.write_text(text)

# Fail closed across Rust callers: runtime explicit-ID compatibility APIs should
# no longer be referenced outside the lower ParameterStore implementation.
for path in root.rglob('*.rs'):
    source = path.read_text()
    if path == root / 'crates/chassis-core/src/parameters.rs':
        continue
    for stale in [
        'state_document_for_product(',
        'encode_state_for_product(',
        'apply_parameter_state_for_product(',
        'apply_parameter_state_bytes(',
        'apply_state_for_product(',
        'apply_state_bytes_for_product(',
        'parameter_state_document(',
        'encode_parameter_state(',
        'InstanceStateLoadError',
    ]:
        if stale in source:
            raise SystemExit(f'{stale} remains in {path}')
