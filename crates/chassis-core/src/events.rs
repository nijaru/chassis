//! Format-independent event-port and semantic note contracts.
//!
//! Event ports use stable product identities while process-time events use dense
//! activation-local indices. This mirrors Chassis audio/parameter identity: a
//! backend numeric ID or declaration position must not become persistent product
//! identity by accident.
//!
//! The first executable event family is semantic note input. Raw MIDI, note
//! expression, parameter modulation, and output sinks build on the same port and
//! timing model without requiring one fabricated cross-family event order.

use core::fmt;

/// Stable author-facing identity for an event port.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
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

/// Dense activation-local identity for one event port.
///
/// Adapters/runtime setup resolve [`EventPortKey`] to this compact process-time
/// identity before entering the realtime path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EventPortIndex(u32);

impl EventPortIndex {
    /// Construct an event-port index from validated setup mapping.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Return the dense numeric index.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Direction of an event port from the component's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventPortDirection {
    /// Events enter the component through this port.
    Input,
    /// Events leave the component through this port.
    Output,
}

/// Semantic event dialect supported by one port.
///
/// These describe representational capability, not a requirement that adapters
/// convert every dialect into every other dialect. Lossy conversion remains an
/// explicit deployment/product policy.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventDialect {
    /// Chassis semantic note/note-expression events.
    Notes,
    /// Raw MIDI 1 messages/SysEx where supported.
    Midi1,
    /// MIDI 2 / UMP data where supported.
    Midi2,
}

/// Stable metadata for one event port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// Failure while validating a component's immutable event-port schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventPortSchemaError {
    /// A descriptor used an empty stable key.
    EmptyKey,
    /// Two descriptors used the same stable key.
    DuplicateKey(EventPortKey),
    /// A port declared no event dialects.
    NoDialects(EventPortKey),
    /// A port repeated the same dialect in its capability list.
    DuplicateDialect {
        /// Port whose dialect list is invalid.
        port: EventPortKey,
        /// Repeated dialect.
        dialect: EventDialect,
    },
}

impl fmt::Display for EventPortSchemaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyKey => formatter.write_str("event-port key must not be empty"),
            Self::DuplicateKey(key) => write!(formatter, "duplicate event-port key `{key}`"),
            Self::NoDialects(key) => write!(formatter, "event port `{key}` declares no dialects"),
            Self::DuplicateDialect { port, dialect } => {
                write!(formatter, "event port `{port}` repeats dialect {dialect:?}")
            }
        }
    }
}

impl std::error::Error for EventPortSchemaError {}

/// Validate stable event-port descriptors without allocating.
///
/// # Errors
///
/// Returns [`EventPortSchemaError`] for empty/duplicate keys, empty dialect
/// capability lists, or repeated dialects on one port.
pub fn validate_event_port_schema(
    descriptors: &[EventPortDescriptor],
) -> Result<(), EventPortSchemaError> {
    for (index, descriptor) in descriptors.iter().enumerate() {
        if descriptor.key.as_str().is_empty() {
            return Err(EventPortSchemaError::EmptyKey);
        }
        if descriptors[..index]
            .iter()
            .any(|previous| previous.key == descriptor.key)
        {
            return Err(EventPortSchemaError::DuplicateKey(descriptor.key));
        }
        if descriptor.dialects.is_empty() {
            return Err(EventPortSchemaError::NoDialects(descriptor.key));
        }
        for (dialect_index, dialect) in descriptor.dialects.iter().enumerate() {
            if descriptor.dialects[..dialect_index].contains(dialect) {
                return Err(EventPortSchemaError::DuplicateDialect {
                    port: descriptor.key,
                    dialect: *dialect,
                });
            }
        }
    }
    Ok(())
}

/// Backend-independent note identity when a source supplies one.
///
/// `None` in [`NoteAddress`] represents an unspecified/wildcard note identity;
/// backend sentinel values such as `-1` must not leak into product code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NoteId(u32);

impl NoteId {
    /// Construct a semantic note identity from an adapter-validated value.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Return the numeric identity.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// MIDI-style note channel in the range 0..=15.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NoteChannel(u8);

impl NoteChannel {
    /// Construct a note channel.
    #[must_use]
    pub const fn new(value: u8) -> Option<Self> {
        if value <= 15 { Some(Self(value)) } else { None }
    }

    /// Return the zero-based channel number.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// MIDI-style note key in the range 0..=127.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NoteKey(u8);

impl NoteKey {
    /// Construct a note key.
    #[must_use]
    pub const fn new(value: u8) -> Option<Self> {
        if value <= 127 {
            Some(Self(value))
        } else {
            None
        }
    }

    /// Return the key number.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// Finite normalized value in the inclusive range 0.0..=1.0.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct NormalizedValue(f64);

impl NormalizedValue {
    /// Construct a normalized value.
    #[must_use]
    pub fn new(value: f64) -> Option<Self> {
        (value.is_finite() && (0.0..=1.0).contains(&value)).then_some(Self(value))
    }

    /// Return the normalized scalar.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

/// Semantic target/address for a note event.
///
/// Optional fields preserve source wildcard/unspecified semantics without raw
/// backend sentinel integers. The event port itself is always explicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NoteAddress {
    port: EventPortIndex,
    note_id: Option<NoteId>,
    channel: Option<NoteChannel>,
    key: Option<NoteKey>,
}

impl NoteAddress {
    /// Construct a note address.
    #[must_use]
    pub const fn new(
        port: EventPortIndex,
        note_id: Option<NoteId>,
        channel: Option<NoteChannel>,
        key: Option<NoteKey>,
    ) -> Self {
        Self {
            port,
            note_id,
            channel,
            key,
        }
    }

    /// Return the activation-local event port.
    #[must_use]
    pub const fn port(self) -> EventPortIndex {
        self.port
    }

    /// Return source note identity when available.
    #[must_use]
    pub const fn note_id(self) -> Option<NoteId> {
        self.note_id
    }

    /// Return source channel when available.
    #[must_use]
    pub const fn channel(self) -> Option<NoteChannel> {
        self.channel
    }

    /// Return source key when available.
    #[must_use]
    pub const fn key(self) -> Option<NoteKey> {
        self.key
    }
}

/// Semantic note operation at one sample offset.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NoteEventKind {
    /// Start a note with normalized velocity.
    On {
        /// Note target/identity.
        address: NoteAddress,
        /// Normalized note-on velocity.
        velocity: NormalizedValue,
    },
    /// Release a note with normalized release velocity when provided by source.
    Off {
        /// Note target/identity.
        address: NoteAddress,
        /// Normalized release velocity.
        velocity: NormalizedValue,
    },
    /// Force immediate note termination/choke where source semantics provide it.
    Choke {
        /// Note target/identity.
        address: NoteAddress,
    },
    /// Notify that a note is fully ended where source/target semantics use it.
    End {
        /// Note target/identity.
        address: NoteAddress,
    },
}

impl NoteEventKind {
    /// Return the semantic note address targeted by this operation.
    #[must_use]
    pub const fn address(self) -> NoteAddress {
        match self {
            Self::On { address, .. }
            | Self::Off { address, .. }
            | Self::Choke { address }
            | Self::End { address } => address,
        }
    }
}

/// One sample-positioned semantic note operation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoteEvent {
    offset: u32,
    kind: NoteEventKind,
}

impl NoteEvent {
    /// Construct a note event.
    #[must_use]
    pub const fn new(offset: u32, kind: NoteEventKind) -> Self {
        Self { offset, kind }
    }

    /// Return the block-relative sample offset.
    #[must_use]
    pub const fn offset(self) -> u32 {
        self.offset
    }

    /// Return the semantic note operation.
    #[must_use]
    pub const fn kind(self) -> NoteEventKind {
        self.kind
    }
}

/// Validated borrowed semantic note events for one process block.
///
/// Events preserve source order for equal offsets and require nondecreasing
/// offsets. Construction never copies event storage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoteEvents<'a> {
    events: &'a [NoteEvent],
    frame_count: u32,
}

impl<'a> NoteEvents<'a> {
    /// Validate borrowed events against one block and explicit activation bound.
    ///
    /// # Errors
    ///
    /// Returns [`NoteEventsError`] when the event count exceeds `max_events`, an
    /// event offset lies outside the block, or source offsets are not monotonic.
    pub fn new(
        events: &'a [NoteEvent],
        frame_count: u32,
        max_events: u32,
    ) -> Result<Self, NoteEventsError> {
        let maximum =
            usize::try_from(max_events).map_err(|_| NoteEventsError::LimitNotRepresentable)?;
        if events.len() > maximum {
            return Err(NoteEventsError::TooManyEvents {
                actual: events.len(),
                maximum: max_events,
            });
        }

        let mut previous_offset = None;
        for (index, event) in events.iter().enumerate() {
            if event.offset >= frame_count {
                return Err(NoteEventsError::OffsetOutOfRange {
                    index,
                    offset: event.offset,
                    frame_count,
                });
            }
            if let Some(previous) = previous_offset
                && event.offset < previous
            {
                return Err(NoteEventsError::NonMonotonicOffsets {
                    index,
                    previous,
                    actual: event.offset,
                });
            }
            previous_offset = Some(event.offset);
        }

        Ok(Self {
            events,
            frame_count,
        })
    }

    /// Return an empty note-event list for one block size.
    #[must_use]
    pub const fn empty_for_block(frame_count: u32) -> NoteEvents<'static> {
        NoteEvents {
            events: &[],
            frame_count,
        }
    }

    /// Return the block frame count used during validation.
    #[must_use]
    pub const fn frame_count(self) -> u32 {
        self.frame_count
    }

    /// Return the number of events.
    #[must_use]
    pub const fn len(self) -> usize {
        self.events.len()
    }

    /// Return whether no note events were supplied.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.events.is_empty()
    }

    /// Iterate events in source-provided order.
    pub fn iter(self) -> impl Iterator<Item = &'a NoteEvent> {
        self.events.iter()
    }

    /// Iterate only events targeting one activation-local event port.
    pub fn for_port(self, port: EventPortIndex) -> impl Iterator<Item = &'a NoteEvent> {
        self.events
            .iter()
            .filter(move |event| event.kind.address().port() == port)
    }
}

/// Invalid borrowed semantic note events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteEventsError {
    /// `max_events` cannot be represented by platform `usize`.
    LimitNotRepresentable,
    /// More events were supplied than the accepted activation bound.
    TooManyEvents {
        /// Actual event count.
        actual: usize,
        /// Maximum accepted count.
        maximum: u32,
    },
    /// One event offset was not inside the process block.
    OffsetOutOfRange {
        /// Event index in source order.
        index: usize,
        /// Invalid block-relative offset.
        offset: u32,
        /// Block frame count.
        frame_count: u32,
    },
    /// Source event offsets decreased.
    NonMonotonicOffsets {
        /// Event index in source order.
        index: usize,
        /// Previous source offset.
        previous: u32,
        /// Current decreasing offset.
        actual: u32,
    },
}

impl fmt::Display for NoteEventsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LimitNotRepresentable => {
                formatter.write_str("note event limit is not representable on this platform")
            }
            Self::TooManyEvents { actual, maximum } => write!(
                formatter,
                "note event list has {actual} events but accepts at most {maximum}"
            ),
            Self::OffsetOutOfRange {
                index,
                offset,
                frame_count,
            } => write!(
                formatter,
                "note event {index} has offset {offset} outside {frame_count}-frame block"
            ),
            Self::NonMonotonicOffsets {
                index,
                previous,
                actual,
            } => write!(
                formatter,
                "note event {index} has offset {actual} before previous offset {previous}"
            ),
        }
    }
}

impl std::error::Error for NoteEventsError {}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE_INPUT: EventPortKey = EventPortKey::new("notes.in");
    const NOTE_DIALECTS: &[EventDialect] = &[EventDialect::Notes];

    fn address() -> NoteAddress {
        NoteAddress::new(
            EventPortIndex::new(0),
            Some(NoteId::new(42)),
            NoteChannel::new(1),
            NoteKey::new(60),
        )
    }

    fn velocity(value: f64) -> NormalizedValue {
        NormalizedValue::new(value).expect("test velocity is normalized")
    }

    #[test]
    fn validates_port_schema() {
        let ports = [EventPortDescriptor {
            key: NOTE_INPUT,
            name: "Notes",
            direction: EventPortDirection::Input,
            dialects: NOTE_DIALECTS,
        }];
        assert_eq!(validate_event_port_schema(&ports), Ok(()));
    }

    #[test]
    fn rejects_duplicate_port_dialects() {
        const DUPLICATED: &[EventDialect] = &[EventDialect::Notes, EventDialect::Notes];
        let ports = [EventPortDescriptor {
            key: NOTE_INPUT,
            name: "Notes",
            direction: EventPortDirection::Input,
            dialects: DUPLICATED,
        }];
        assert_eq!(
            validate_event_port_schema(&ports),
            Err(EventPortSchemaError::DuplicateDialect {
                port: NOTE_INPUT,
                dialect: EventDialect::Notes,
            })
        );
    }

    #[test]
    fn note_channel_and_key_reject_out_of_domain_values() {
        assert_eq!(NoteChannel::new(15).map(NoteChannel::get), Some(15));
        assert_eq!(NoteChannel::new(16), None);
        assert_eq!(NoteKey::new(127).map(NoteKey::get), Some(127));
        assert_eq!(NoteKey::new(128), None);
    }

    #[test]
    fn normalized_values_reject_nonfinite_and_out_of_range_values() {
        assert_eq!(NormalizedValue::new(-0.01), None);
        assert_eq!(NormalizedValue::new(1.01), None);
        assert_eq!(NormalizedValue::new(f64::NAN), None);
        assert_eq!(
            NormalizedValue::new(0.5).map(NormalizedValue::get),
            Some(0.5)
        );
    }

    #[test]
    fn note_events_preserve_equal_offset_source_order() {
        let events = [
            NoteEvent::new(
                7,
                NoteEventKind::On {
                    address: address(),
                    velocity: velocity(0.8),
                },
            ),
            NoteEvent::new(
                7,
                NoteEventKind::Off {
                    address: address(),
                    velocity: velocity(0.2),
                },
            ),
        ];
        let validated = NoteEvents::new(&events, 64, 8).expect("events are valid");
        let kinds: Vec<_> = validated.iter().map(|event| event.kind()).collect();
        assert_eq!(kinds, events.map(NoteEvent::kind));
    }

    #[test]
    fn note_events_reject_decreasing_offsets_and_bounds() {
        let events = [
            NoteEvent::new(4, NoteEventKind::Choke { address: address() }),
            NoteEvent::new(3, NoteEventKind::End { address: address() }),
        ];
        assert_eq!(
            NoteEvents::new(&events, 8, 8),
            Err(NoteEventsError::NonMonotonicOffsets {
                index: 1,
                previous: 4,
                actual: 3,
            })
        );

        let outside = [NoteEvent::new(8, NoteEventKind::End { address: address() })];
        assert_eq!(
            NoteEvents::new(&outside, 8, 8),
            Err(NoteEventsError::OffsetOutOfRange {
                index: 0,
                offset: 8,
                frame_count: 8,
            })
        );
    }
}
