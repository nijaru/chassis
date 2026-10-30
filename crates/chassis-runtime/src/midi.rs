//! Opaque, borrowed MIDI 1 input and bounded payload admission.
//!
//! No status/data interpretation, padding normalization, `SysEx` framing, or
//! conversion to semantic notes occurs here. Payload ownership stays with the
//! caller; a consumer retaining `SysEx` must copy it into independently owned,
//! bounded storage. A callback copy must use preallocated storage.

use core::fmt;

use crate::events::{
    EventDialect, EventFlags, EventPortDescriptor, EventPortDirection, EventPortIndex,
};

/// Opaque MIDI 1 bytes. Even empty or unframed `SysEx` is preserved verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidiMessage<'a> {
    /// Exactly three opaque bytes, including unused padding supplied by a source.
    Short([u8; 3]),
    /// Borrowed payload, valid only for the lifetime of its source storage.
    SysEx(&'a [u8]),
}

/// A sample-positioned MIDI message with a concrete activation-local port.
///
/// Payloads cannot escape their owner without a copy:
/// ```compile_fail
/// use chassis_runtime::{events::EventPortIndex, midi::{MidiEvent, MidiMessage}};
/// let event = {
///     let bytes = vec![1, 2, 3];
///     MidiEvent::new(0, EventPortIndex::new(0), MidiMessage::SysEx(&bytes))
/// };
/// println!("{:?}", event.message());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MidiEvent<'a> {
    offset: u32,
    port: EventPortIndex,
    message: MidiMessage<'a>,
    flags: EventFlags,
}

impl<'a> MidiEvent<'a> {
    /// Construct an event with no annotations.
    #[must_use]
    pub const fn new(offset: u32, port: EventPortIndex, message: MidiMessage<'a>) -> Self {
        Self {
            offset,
            port,
            message,
            flags: EventFlags::new(false, false),
        }
    }

    /// Return the block-relative sample offset.
    #[must_use]
    pub const fn offset(self) -> u32 {
        self.offset
    }

    /// Return the concrete event port.
    #[must_use]
    pub const fn port(self) -> EventPortIndex {
        self.port
    }

    /// Return the opaque borrowed message.
    #[must_use]
    pub const fn message(self) -> MidiMessage<'a> {
        self.message
    }

    /// Return portable source annotations.
    #[must_use]
    pub const fn flags(self) -> EventFlags {
        self.flags
    }

    /// Replace annotations without changing payload storage.
    #[must_use]
    pub const fn with_flags(mut self, flags: EventFlags) -> Self {
        self.flags = flags;
        self
    }

    /// Retarget an event without copying its payload.
    #[must_use]
    pub const fn with_port(mut self, port: EventPortIndex) -> Self {
        self.port = port;
        self
    }
}

/// Activation-owned MIDI event/aggregate `SysEx` byte bounds. Both default to zero.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct MidiEventLimits {
    max_events: u32,
    max_sysex_bytes: u32,
}

impl MidiEventLimits {
    /// Construct count and aggregate payload bounds (not per-message bounds).
    #[must_use]
    pub const fn new(max_events: u32, max_sysex_bytes: u32) -> Self {
        Self {
            max_events,
            max_sysex_bytes,
        }
    }

    /// Return the input event or output attempt bound.
    #[must_use]
    pub const fn max_events(self) -> u32 {
        self.max_events
    }

    /// Return the aggregate `SysEx` byte bound.
    #[must_use]
    pub const fn max_sysex_bytes(self) -> u32 {
        self.max_sysex_bytes
    }
}

/// Immutable, indexed, allocation-free source for one callback.
///
/// The finite length and every event (including bytes, port, flags and offset)
/// must remain identical across all calls/traversals in that callback. `get`
/// returns `Some` exactly for `0..len()`, and must not allocate, deallocate,
/// block, or perform unbounded work. Returned payloads borrow live source storage;
/// adapters must not retain host-owned pointers/payloads beyond the callback.
/// Admission caches counts/byte totals under these obligations, just as process
/// buffer sources repeat their topology without requalification on each access.
pub trait MidiEventSource {
    /// Return the stable finite event count.
    fn len(&self) -> usize;
    /// Return one event borrowing this source, or `None` outside its range.
    fn get(&self, index: usize) -> Option<MidiEvent<'_>>;
    /// Return whether the source contains no events.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<const N: usize> MidiEventSource for [MidiEvent<'_>; N] {
    fn len(&self) -> usize {
        N
    }
    fn get(&self, index: usize) -> Option<MidiEvent<'_>> {
        self.as_slice().get(index).copied()
    }
}

// A sized reference permits `MidiEvents::new(&slice, ...)` without a wrapper or
// materializing messages from an adapter's lazy source.
impl MidiEventSource for &[MidiEvent<'_>] {
    fn len(&self) -> usize {
        <[MidiEvent<'_>]>::len(self)
    }
    fn get(&self, index: usize) -> Option<MidiEvent<'_>> {
        <[MidiEvent<'_>]>::get(self, index).copied()
    }
}

/// Validated borrowed MIDI view. Admission scans at most the accepted event count
/// and uses lengths rather than traversing payload bytes. No payload is copied.
#[derive(Clone, Copy)]
pub struct MidiEvents<'a> {
    source: &'a dyn MidiEventSource,
    count: usize,
    frame_count: u32,
    sysex_bytes: u32,
}

impl fmt::Debug for MidiEvents<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MidiEvents")
            .field("count", &self.count)
            .field("frame_count", &self.frame_count)
            .field("sysex_bytes", &self.sysex_bytes)
            .finish_non_exhaustive()
    }
}

impl<'a> MidiEvents<'a> {
    /// Admit a stable source with explicit count and aggregate byte bounds.
    ///
    /// # Errors
    /// Rejects count/source inconsistencies, nonmonotonic or out-of-block times,
    /// and overflowing or over-budget payload totals before product processing.
    pub fn new(
        source: &'a dyn MidiEventSource,
        frame_count: u32,
        maximum_events: u32,
        maximum_sysex_bytes: u32,
    ) -> Result<Self, MidiEventsError> {
        let count = source.len();
        let maximum =
            usize::try_from(maximum_events).map_err(|_| MidiEventsError::LimitNotRepresentable)?;
        if count > maximum {
            return Err(MidiEventsError::TooManyEvents {
                actual: count,
                maximum: maximum_events,
            });
        }
        let mut previous = None;
        let mut sysex_bytes = 0_u32;
        for index in 0..count {
            let event = source
                .get(index)
                .ok_or(MidiEventsError::SourceCountMismatch)?;
            if event.offset() >= frame_count {
                return Err(MidiEventsError::OffsetOutOfRange {
                    index,
                    offset: event.offset(),
                    frame_count,
                });
            }
            if let Some(previous) = previous
                && event.offset() < previous
            {
                return Err(MidiEventsError::NonMonotonicOffsets {
                    index,
                    previous,
                    actual: event.offset(),
                });
            }
            previous = Some(event.offset());
            if let MidiMessage::SysEx(bytes) = event.message() {
                let length =
                    u32::try_from(bytes.len()).map_err(|_| MidiEventsError::SysExBytesOverflow)?;
                sysex_bytes = sysex_bytes
                    .checked_add(length)
                    .ok_or(MidiEventsError::SysExBytesOverflow)?;
                if sysex_bytes > maximum_sysex_bytes {
                    return Err(MidiEventsError::TooManySysExBytes {
                        actual: sysex_bytes,
                        maximum: maximum_sysex_bytes,
                    });
                }
            }
        }
        if source.len() != count || source.get(count).is_some() {
            return Err(MidiEventsError::SourceCountMismatch);
        }
        Ok(Self {
            source,
            count,
            frame_count,
            sysex_bytes,
        })
    }

    /// Return an empty MIDI view for one block.
    #[must_use]
    pub const fn empty_for_block(frame_count: u32) -> MidiEvents<'static> {
        const EMPTY: [MidiEvent<'static>; 0] = [];
        MidiEvents {
            source: &EMPTY,
            count: 0,
            frame_count,
            sysex_bytes: 0,
        }
    }

    /// Return the admitted event count without consulting the source again.
    #[must_use]
    pub const fn len(self) -> usize {
        self.count
    }
    /// Return whether this view is empty.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.count == 0
    }
    /// Return the admitted block size.
    #[must_use]
    pub const fn frame_count(self) -> u32 {
        self.frame_count
    }
    /// Return the cached aggregate `SysEx` bytes.
    #[must_use]
    pub const fn sysex_bytes(self) -> u32 {
        self.sysex_bytes
    }

    /// Traverse source order without copying payloads.
    #[must_use = "MIDI event traversal is lazy"]
    #[expect(
        clippy::missing_panics_doc,
        reason = "stable source is an explicit trait obligation"
    )]
    pub fn iter(self) -> impl ExactSizeIterator<Item = MidiEvent<'a>> + use<'a> {
        (0..self.count).map(move |index| {
            self.source
                .get(index)
                .expect("admitted MIDI source remains stable")
        })
    }

    /// Validate concrete input ports and MIDI 1 capability.
    ///
    /// # Errors
    /// Rejects unknown, output-only, or non-MIDI-1 targets.
    pub fn validate_ports(
        self,
        descriptors: &[EventPortDescriptor],
    ) -> Result<(), MidiEventPortError> {
        for event in self.iter() {
            let port = event.port();
            let index = usize::try_from(port.get())
                .map_err(|_| MidiEventPortError::PortIndexNotRepresentable(port))?;
            let descriptor = descriptors
                .get(index)
                .ok_or(MidiEventPortError::UnknownPort(port))?;
            if descriptor.direction != EventPortDirection::Input {
                return Err(MidiEventPortError::PortIsNotInput(port));
            }
            if !descriptor.dialects.contains(&EventDialect::Midi1) {
                return Err(MidiEventPortError::PortDoesNotSupportMidi1(port));
            }
        }
        Ok(())
    }
}

/// Invalid source or admission bounds. Errors never own heap storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidiEventsError {
    /// Count limit cannot be represented on this platform.
    LimitNotRepresentable,
    /// Source length/get disagree.
    SourceCountMismatch,
    /// Too many input events.
    TooManyEvents {
        /// Source count.
        actual: usize,
        /// Admission bound.
        maximum: u32,
    },
    /// A payload length or aggregate cannot fit in `u32`.
    SysExBytesOverflow,
    /// Aggregate payload budget exceeded.
    TooManySysExBytes {
        /// Aggregate bytes through the rejected event.
        actual: u32,
        /// Admission bound.
        maximum: u32,
    },
    /// Event is outside the block.
    OffsetOutOfRange {
        /// Source index.
        index: usize,
        /// Invalid offset.
        offset: u32,
        /// Admitted block size.
        frame_count: u32,
    },
    /// Source time decreased.
    NonMonotonicOffsets {
        /// Source index.
        index: usize,
        /// Previous offset.
        previous: u32,
        /// Current offset.
        actual: u32,
    },
}

impl fmt::Display for MidiEventsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid MIDI events: {self:?}")
    }
}
impl std::error::Error for MidiEventsError {}

/// Invalid MIDI input port capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidiEventPortError {
    /// Index cannot be represented on this platform.
    PortIndexNotRepresentable(EventPortIndex),
    /// Index is absent from the schema.
    UnknownPort(EventPortIndex),
    /// MIDI input targeted an output port.
    PortIsNotInput(EventPortIndex),
    /// Port does not support MIDI 1.
    PortDoesNotSupportMidi1(EventPortIndex),
}
impl fmt::Display for MidiEventPortError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid MIDI event port: {self:?}")
    }
}
impl std::error::Error for MidiEventPortError {}
