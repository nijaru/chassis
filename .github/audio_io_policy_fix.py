from pathlib import Path
import sys

path = Path('crates/chassis-core/tests/conformance.rs')
text = path.read_text()
current = 'fn malformed_audio_configuration_never_reaches_product_activation() {'
legacy = 'fn activation_rejects_invalid_audio_configuration_without_calling_product() {'

if len(sys.argv) != 2 or sys.argv[1] not in {'before', 'after'}:
    raise SystemExit('usage: audio_io_policy_fix.py before|after')

if sys.argv[1] == 'before':
    if current not in text or legacy in text:
        raise SystemExit('conformance migration anchor changed unexpectedly')
    text = text.replace(current, legacy, 1)
else:
    if legacy not in text:
        raise SystemExit('temporary conformance migration anchor missing after migration')
    text = text.replace(legacy, current, 1)

path.write_text(text)
