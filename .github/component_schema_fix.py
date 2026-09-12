from pathlib import Path

root = Path('.')

state_path = root / 'crates/chassis-core/tests/state_adversarial.rs'
text = state_path.read_text()
old = '''    let mut runtime =
        InstanceRuntime::<NoopProcessor>::new(&[descriptor]).expect("runtime schema is valid");
'''
new = '''    let schema = chassis_core::schema::ComponentSchema::stereo_effect(vec![descriptor])
        .expect("runtime schema is valid");
    let mut runtime =
        InstanceRuntime::<NoopProcessor>::from_schema(schema).expect("runtime schema is valid");
'''
if old not in text:
    raise SystemExit('state adversarial runtime constructor changed unexpectedly')
state_path.write_text(text.replace(old, new))

clap_path = root / 'crates/chassis-clap/src/lib.rs'
text = clap_path.read_text()
text = text.replace(
    'parameters::{ChoiceOption, ParameterKind, ParameterStore},',
    'parameters::{ChoiceOption, ParameterKind},',
)
clap_path.write_text(text)

# The removed constructor must not survive in any Rust spelling.
offenders = []
for path in root.rglob('*.rs'):
    text = path.read_text()
    if '::new_with_event_ports(' in text:
        offenders.append(str(path))
    if 'InstanceRuntime::new(' in text or 'InstanceRuntime::<' in text and '>::new(' in text:
        offenders.append(str(path))
if offenders:
    raise SystemExit(f'proof-era runtime constructor remains: {sorted(set(offenders))}')
