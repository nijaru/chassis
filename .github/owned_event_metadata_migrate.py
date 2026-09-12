from pathlib import Path
import re

root = Path('.')

events_path = root / 'crates/chassis-core/src/events.rs'
text = events_path.read_text()
text = text.replace('use core::fmt;\n', 'use core::fmt;\nuse std::{borrow::Cow, string::String, vec::Vec};\n')

old = '''#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EventPortKey(&'static str);

impl EventPortKey {
    /// Construct a stable event-port key from static product metadata.
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

impl fmt::Display for EventPortKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}
'''
new = '''#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EventPortKey(Cow<'static, str>);

impl EventPortKey {
    /// Construct a zero-allocation event-port key from static product metadata.
    #[must_use]
    pub const fn new(value: &'static str) -> Self {
        Self(Cow::Borrowed(value))
    }

    /// Construct an event-port key owned by setup/control-domain metadata.
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

impl AsRef<str> for EventPortKey {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for EventPortKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}
'''
if old not in text:
    raise SystemExit('EventPortKey block changed unexpectedly')
text = text.replace(old, new)

old = '''#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventPortDescriptor {
    /// Persistent product key.
    pub key: EventPortKey,
    /// Human-readable display name. This is not persistent identity.
    pub name: &'static str,
    /// Input/output direction.
    pub direction: EventPortDirection,
    /// Dialects this port can represent.
    pub dialects: &'static [EventDialect],
}
'''
new = '''#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventPortDescriptor {
    /// Persistent product key.
    pub key: EventPortKey,
    /// Human-readable display name. This is not persistent identity.
    pub name: Cow<'static, str>,
    /// Input/output direction.
    pub direction: EventPortDirection,
    /// Dialects this port can represent.
    pub dialects: Cow<'static, [EventDialect]>,
}

impl EventPortDescriptor {
    /// Construct static product metadata without allocating.
    #[must_use]
    pub const fn new(
        key: EventPortKey,
        name: &'static str,
        direction: EventPortDirection,
        dialects: &'static [EventDialect],
    ) -> Self {
        Self {
            key,
            name: Cow::Borrowed(name),
            direction,
            dialects: Cow::Borrowed(dialects),
        }
    }

    /// Construct runtime-owned event-port metadata for hosted/dynamic components.
    #[must_use]
    pub fn owned(
        key: impl Into<String>,
        name: impl Into<String>,
        direction: EventPortDirection,
        dialects: Vec<EventDialect>,
    ) -> Self {
        Self {
            key: EventPortKey::owned(key),
            name: Cow::Owned(name.into()),
            direction,
            dialects: Cow::Owned(dialects),
        }
    }
}
'''
if old not in text:
    raise SystemExit('EventPortDescriptor block changed unexpectedly')
text = text.replace(old, new)

text = text.replace(
    '#[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub enum EventPortSchemaError',
    '#[derive(Debug, Clone, PartialEq, Eq)]\npub enum EventPortSchemaError',
)
text = text.replace('EventPortSchemaError::DuplicateKey(descriptor.key)', 'EventPortSchemaError::DuplicateKey(descriptor.key.clone())')
text = text.replace('EventPortSchemaError::NoDialects(descriptor.key)', 'EventPortSchemaError::NoDialects(descriptor.key.clone())')
text = text.replace('port: descriptor.key,\n                    dialect:', 'port: descriptor.key.clone(),\n                    dialect:')

old = '''pub fn event_port_index(
    descriptors: &[EventPortDescriptor],
    key: EventPortKey,
) -> Option<EventPortIndex> {
    descriptors
        .iter()
        .position(|descriptor| descriptor.key == key)
        .and_then(|index| u32::try_from(index).ok())
        .map(EventPortIndex::new)
}
'''
new = '''pub fn event_port_index(
    descriptors: &[EventPortDescriptor],
    key: &EventPortKey,
) -> Option<EventPortIndex> {
    descriptors
        .iter()
        .position(|descriptor| &descriptor.key == key)
        .and_then(|index| u32::try_from(index).ok())
        .map(EventPortIndex::new)
}
'''
if old not in text:
    raise SystemExit('event_port_index shape changed unexpectedly')
text = text.replace(old, new)

# Static descriptor convenience for tests plus explicit owned metadata coverage.
def event_descriptor_repl(match):
    key, name, direction, dialects = match.groups()
    return f'EventPortDescriptor::new({key}, "{name}", EventPortDirection::{direction}, {dialects})'

text = re.sub(
    r'EventPortDescriptor \{\s*key: ([A-Z][A-Z0-9_]*|EventPortKey::new\("[^"]+"\)),\s*name: "([^"]+)",\s*direction: EventPortDirection::(Input|Output),\s*dialects: ([A-Z][A-Z0-9_]*),\s*\}',
    event_descriptor_repl,
    text,
)
text = re.sub(r'const ([A-Z][A-Z0-9_]*): &\[EventPortDescriptor\] = &\[', r'static \1: &[EventPortDescriptor] = &[', text)
text = text.replace('event_port_index(&ports, NOTE_INPUT)', 'event_port_index(&ports, &NOTE_INPUT)')

owned_anchor = '''    #[test]
    fn note_channel_and_key_reject_out_of_domain_values() {
'''
owned_test = '''    #[test]
    fn event_port_metadata_can_be_owned_at_runtime() {
        let descriptor = EventPortDescriptor::owned(
            String::from("notes.dynamic"),
            String::from("Dynamic Notes"),
            EventPortDirection::Input,
            vec![EventDialect::Notes, EventDialect::Midi1],
        );
        assert_eq!(descriptor.key.as_str(), "notes.dynamic");
        assert_eq!(descriptor.name.as_ref(), "Dynamic Notes");
        assert_eq!(descriptor.dialects.as_ref(), &[EventDialect::Notes, EventDialect::Midi1]);
        assert_eq!(validate_event_port_schema(&[descriptor]), Ok(()));
    }

'''
if owned_anchor not in text:
    raise SystemExit('event owned metadata test anchor missing')
text = text.replace(owned_anchor, owned_test + owned_anchor, 1)
events_path.write_text(text)

# Component schema/runtime stable lookup APIs borrow event keys.
for rel in ['crates/chassis-core/src/schema.rs', 'crates/chassis-core/src/runtime.rs']:
    path = root / rel
    text = path.read_text()
    text = text.replace(
        'pub fn event_port_index(&self, key: EventPortKey) -> Option<EventPortIndex> {\n        event_port_index(&self.event_ports, key)\n    }',
        'pub fn event_port_index(&self, key: &EventPortKey) -> Option<EventPortIndex> {\n        event_port_index(&self.event_ports, key)\n    }',
    )
    text = text.replace(
        'pub fn event_port_index(&self, key: EventPortKey) -> Option<EventPortIndex> {\n        self.schema.event_port_index(key)\n    }',
        'pub fn event_port_index(&self, key: &EventPortKey) -> Option<EventPortIndex> {\n        self.schema.event_port_index(key)\n    }',
    )
    path.write_text(text)

# Replace static descriptor literals and static descriptor slice constants in all
# Rust fixtures. Lowercase local keys are handled separately below.
for path in root.rglob('*.rs'):
    if path == events_path:
        continue
    text = path.read_text()
    original = text
    text = re.sub(
        r'EventPortDescriptor \{\s*key: ([A-Z][A-Z0-9_]*|EventPortKey::new\("[^"]+"\)),\s*name: "([^"]+)",\s*direction: EventPortDirection::(Input|Output),\s*dialects: ([A-Z][A-Z0-9_]*),\s*\}',
        event_descriptor_repl,
        text,
    )
    text = re.sub(r'const ([A-Z][A-Z0-9_]*): &\[EventPortDescriptor\] = &\[', r'static \1: &[EventPortDescriptor] = &[', text)
    text = re.sub(
        r'\.event_port_index\((NOTE_INPUT|INPUT_KEY)\)',
        r'.event_port_index(&\1)',
        text,
    )
    if text != original:
        path.write_text(text)

# The coherent-schema unit test uses a local event key, so clone it into the
# owned descriptor and borrow it for lookup.
schema_path = root / 'crates/chassis-core/src/schema.rs'
text = schema_path.read_text()
old = '''            vec![EventPortDescriptor {
                key: note_key,
                name: "Notes",
                direction: EventPortDirection::Input,
                dialects: NOTE_DIALECTS,
            }],
'''
new = '''            vec![EventPortDescriptor::new(
                note_key.clone(),
                "Notes",
                EventPortDirection::Input,
                NOTE_DIALECTS,
            )],
'''
if old not in text:
    raise SystemExit('schema local event descriptor shape changed unexpectedly')
text = text.replace(old, new)
text = text.replace('schema.event_port_index(note_key)', 'schema.event_port_index(&note_key)')
schema_path.write_text(text)

# Sanity checks: realtime note addresses remain dense-only and static-only event
# metadata types are gone.
events_text = events_path.read_text()
if "pub struct EventPortKey(Cow<'static, str>)" not in events_text:
    raise SystemExit('EventPortKey ownership migration did not apply')
if "dialects: Cow<'static, [EventDialect]>" not in events_text:
    raise SystemExit('EventPortDescriptor dialect ownership migration did not apply')
if 'key: &EventPortKey' not in events_text:
    raise SystemExit('event_port_index did not migrate to borrowed lookup')
address_start = events_text.index('pub struct NoteAddress')
address_end = events_text.index('impl NoteAddress', address_start)
if 'EventPortKey' in events_text[address_start:address_end]:
    raise SystemExit('stable event key leaked into NoteAddress')
