from pathlib import Path

root = Path('.')

audio_path = root / 'crates/chassis-core/src/audio.rs'
text = audio_path.read_text()
old = 'pub const DEFAULT_EFFECT_PORTS: [AudioPortDescriptor; 3] = ['
new = 'pub static DEFAULT_EFFECT_PORTS: [AudioPortDescriptor; 3] = ['
if old not in text:
    raise SystemExit('default effect descriptor array shape changed unexpectedly')
audio_path.write_text(text.replace(old, new, 1))

clap_path = root / 'crates/chassis-clap/src/audio.rs'
text = clap_path.read_text()
old = 'pub const DEFAULT_CLAP_AUDIO_PORTS: [ClapAudioPort; 3] = ['
new = 'pub static DEFAULT_CLAP_AUDIO_PORTS: [ClapAudioPort; 3] = ['
if old not in text:
    raise SystemExit('default CLAP audio mapping shape changed unexpectedly')
clap_path.write_text(text.replace(old, new, 1))
