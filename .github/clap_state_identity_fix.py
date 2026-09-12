from pathlib import Path

root = Path('.')
parameters_path = root / 'crates/chassis-clap/src/parameters.rs'
text = parameters_path.read_text()

old = '''    pub(crate) fn matches_descriptors(&self, descriptors: &[ParameterDescriptor]) -> bool {
        descriptors.len() == self.bindings.len()
            && descriptors
                .iter()
                .zip(&self.bindings)
                .all(|(descriptor, binding)| descriptor == binding.descriptor())
    }

'''
if old not in text:
    raise SystemExit('matches_descriptors helper changed unexpectedly')
text = text.replace(old, '')
text = text.replace(
    'restored.apply_state(&corrupt, "com.example.test", 1)',
    'restored.apply_state(&corrupt, &state_identity())',
)

# No parameter-state test/caller should retain the old explicit identity pair.
for stale in [
    'apply_state(&document, "com.example.test", 1)',
    'apply_state(&corrupt, "com.example.test", 1)',
    'encode_state("com.example.test", 1, StateLimits::default())',
]:
    if stale in text:
        raise SystemExit(f'stale explicit CLAP state identity remains: {stale}')

parameters_path.write_text(text)
