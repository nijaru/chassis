//! Borrowed, bounded process-time parameter automation.
//!
//! [`ParameterEvents`] validates a backend-normalized event slice without
//! copying it. Events are globally sample-sorted, retain equal-offset source
//! order, and use a point representation for linear ramps: a `Linear` event
//! reaches its endpoint from the preceding point for that parameter. The
//! implicit preceding point is the parameter's base value at block offset zero.

use core::fmt;

use crate::parameters::ParameterIndex;

/// A process-time parameter value borrowed from backend/event storage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParameterEventValue<'a> {
    /// Finite plain-unit floating-point value.
    Float(f64),
    /// Signed integer value.
    Integer(i64),
    /// Boolean value.
    Boolean(bool),
    /// Stable choice identity borrowed from event storage.
    Choice(&'a str),
}

/// The change represented by one parameter event.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParameterEventChange<'a> {
    /// Set the value at the event's sample offset.
    Set(ParameterEventValue<'a>),
    /// Linearly reach the floating-point endpoint at the event's offset.
    ///
    /// The segment starts at the preceding point for this parameter. If no
    /// earlier point exists in the block, it starts at the parameter's base
    /// value at offset zero.
    Linear {
        /// Endpoint value at [`ParameterEvent::offset`].
        value: f64,
    },
}

/// One borrowed, sample-positioned parameter change.
///
/// Realtime events use a schema-local [`ParameterIndex`] rather than a stable
/// string key. Adapters/runtime setup resolve persistent [`ParameterKey`](crate::parameters::ParameterKey)
/// identities to dense indices before entering the process path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParameterEvent<'a> {
    offset: u32,
    parameter: ParameterIndex,
    change: ParameterEventChange<'a>,
}

impl<'a> ParameterEvent<'a> {
    /// Construct an instantaneous parameter set.
    #[must_use]
    pub const fn set(
        offset: u32,
        parameter: ParameterIndex,
        value: ParameterEventValue<'a>,
    ) -> Self {
        Self {
            offset,
            parameter,
            change: ParameterEventChange::Set(value),
        }
    }

    /// Construct a linear endpoint for one floating-point parameter.
    #[must_use]
    pub const fn linear(offset: u32, parameter: ParameterIndex, value: f64) -> Self {
        Self {
            offset,
            parameter,
            change: ParameterEventChange::Linear { value },
        }
    }

    /// Return the block-relative sample offset.
    #[must_use]
    pub const fn offset(self) -> u32 {
        self.offset
    }

    /// Return the schema-local dense parameter identity.
    #[must_use]
    pub const fn parameter(self) -> ParameterIndex {
        self.parameter
    }

    /// Return the event change.
    #[must_use]
    pub const fn change(self) -> ParameterEventChange<'a> {
        self.change
    }
}

/// Validated borrowed parameter events for one process block.
///
/// Construction checks the event count, value shape, finite floating-point
/// values, and nondecreasing sample offsets. It does not copy event storage or
/// validate an index against a component schema; the active
/// [`ParameterStore`](crate::parameters::ParameterStore) performs that second
/// boundary check before product DSP runs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParameterEvents<'a> {
    events: &'a [ParameterEvent<'a>],
    frame_count: u32,
}

impl<'a> ParameterEvents<'a> {
    /// Validate an event slice for one block.
    ///
    /// `max_events` is supplied by the adapter/runtime from its accepted
    /// process bound. No event storage is allocated or copied.
    ///
    /// # Errors
    ///
    /// Returns [`ParameterEventsError`] when the event slice exceeds its bound,
    /// contains malformed values, or is not sample-sorted.
    pub fn new(
        events: &'a [ParameterEvent<'a>],
        frame_count: u32,
        max_events: u32,
    ) -> Result<Self, ParameterEventsError> {
        let maximum =
            usize::try_from(max_events).map_err(|_| ParameterEventsError::LimitNotRepresentable)?;
        if events.len() > maximum {
            return Err(ParameterEventsError::TooManyEvents {
                actual: events.len(),
                maximum: max_events,
            });
        }

        let mut previous_offset = None;
        for (index, event) in events.iter().enumerate() {
            if event.offset >= frame_count {
                return Err(ParameterEventsError::OffsetOutOfRange {
                    index,
                    offset: event.offset,
                    frame_count,
                });
            }
            if let Some(previous) = previous_offset
                && event.offset < previous
            {
                return Err(ParameterEventsError::NonMonotonicOffsets {
                    index,
                    previous,
                    actual: event.offset,
                });
            }
            previous_offset = Some(event.offset);

            match event.change {
                ParameterEventChange::Set(ParameterEventValue::Float(value))
                | ParameterEventChange::Linear { value } => {
                    if !value.is_finite() {
                        return Err(ParameterEventsError::NonFiniteValue { index });
                    }
                }
                ParameterEventChange::Set(ParameterEventValue::Choice(value)) => {
                    if !valid_choice_identifier(value) {
                        return Err(ParameterEventsError::InvalidChoice { index });
                    }
                }
                ParameterEventChange::Set(
                    ParameterEventValue::Integer(_) | ParameterEventValue::Boolean(_),
                ) => {}
            }
        }

        Ok(Self {
            events,
            frame_count,
        })
    }

    /// Return an empty event list for a specific block size.
    #[must_use]
    pub const fn empty_for_block(frame_count: u32) -> ParameterEvents<'static> {
        ParameterEvents {
            events: &[],
            frame_count,
        }
    }

    /// Return the actual block frame count used during validation.
    #[must_use]
    pub const fn frame_count(self) -> u32 {
        self.frame_count
    }

    /// Return the number of validated events.
    #[must_use]
    pub const fn len(self) -> usize {
        self.events.len()
    }

    /// Return whether no events were supplied.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.events.is_empty()
    }

    /// Iterate events in the source-provided order.
    pub fn iter(self) -> impl Iterator<Item = &'a ParameterEvent<'a>> {
        self.events.iter()
    }

    /// Iterate only events targeting one dense parameter index.
    pub fn for_parameter(
        self,
        parameter: ParameterIndex,
    ) -> impl Iterator<Item = &'a ParameterEvent<'a>> {
        self.events
            .iter()
            .filter(move |event| event.parameter == parameter)
    }

    /// Create a lazy floating-point trajectory cursor for one parameter.
    ///
    /// The caller supplies the validated base value at block offset zero. A
    /// cursor may be queried at nondecreasing offsets only.
    ///
    /// # Errors
    ///
    /// Returns [`ParameterCursorError::NonFiniteInitial`] for a non-finite base
    /// value.
    pub fn float_cursor(
        self,
        parameter: ParameterIndex,
        initial: f64,
    ) -> Result<FloatParameterCursor<'a>, ParameterCursorError> {
        if !initial.is_finite() {
            return Err(ParameterCursorError::NonFiniteInitial);
        }
        Ok(FloatParameterCursor {
            events: self.events,
            frame_count: self.frame_count,
            parameter,
            next_event: 0,
            point_offset: 0,
            point_value: initial,
            last_query_offset: None,
        })
    }
}

/// Lazy cursor over one floating-point parameter's sample trajectory.
///
/// The cursor performs no allocation. It consumes only events for its target
/// while preserving the source order of equal-offset events.
pub struct FloatParameterCursor<'events> {
    events: &'events [ParameterEvent<'events>],
    frame_count: u32,
    parameter: ParameterIndex,
    next_event: usize,
    point_offset: u32,
    point_value: f64,
    last_query_offset: Option<u32>,
}

impl FloatParameterCursor<'_> {
    /// Evaluate the trajectory at one sample offset.
    ///
    /// Values are constant between points unless the next point is linear,
    /// in which case the returned value is interpolated lazily.
    ///
    /// # Errors
    ///
    /// Returns an error for an out-of-range/non-monotonic query or when a
    /// directly-used set event is not floating-point. The latter indicates
    /// that the cursor was used without the schema validation performed by
    /// [`ParameterStore`](crate::parameters::ParameterStore).
    pub fn value_at(&mut self, offset: u32) -> Result<f64, ParameterCursorError> {
        if offset >= self.frame_count {
            return Err(ParameterCursorError::OffsetOutOfRange {
                offset,
                frame_count: self.frame_count,
            });
        }
        if let Some(previous) = self.last_query_offset
            && offset < previous
        {
            return Err(ParameterCursorError::NonMonotonicQuery {
                previous,
                actual: offset,
            });
        }

        let mut index = self.next_event;
        while index < self.events.len() {
            let event = &self.events[index];
            if event.parameter != self.parameter {
                index += 1;
                continue;
            }
            if event.offset > offset {
                break;
            }
            self.apply_point(index, event)?;
            index += 1;
        }
        self.next_event = index;
        self.last_query_offset = Some(offset);

        let mut next = index;
        while next < self.events.len() && self.events[next].parameter != self.parameter {
            next += 1;
        }
        let Some(event) = self.events.get(next) else {
            return Ok(self.point_value);
        };
        let ParameterEventChange::Linear { value } = event.change else {
            return Ok(self.point_value);
        };
        if event.offset <= self.point_offset {
            return Ok(self.point_value);
        }

        let span = f64::from(event.offset - self.point_offset);
        let progress = f64::from(offset - self.point_offset) / span;
        if self.point_value.is_sign_negative() != value.is_sign_negative()
            && self.point_value != 0.0
            && value != 0.0
        {
            Ok(self.point_value.mul_add(1.0 - progress, value * progress))
        } else {
            Ok(self.point_value + (value - self.point_value) * progress)
        }
    }

    fn apply_point(
        &mut self,
        index: usize,
        event: &ParameterEvent<'_>,
    ) -> Result<(), ParameterCursorError> {
        let value = match event.change {
            ParameterEventChange::Set(ParameterEventValue::Float(value))
            | ParameterEventChange::Linear { value } => value,
            ParameterEventChange::Set(_) => {
                return Err(ParameterCursorError::NonFloatValue { index });
            }
        };
        self.point_offset = event.offset;
        self.point_value = value;
        Ok(())
    }
}

fn valid_choice_identifier(value: &str) -> bool {
    !value.is_empty()
        && !value.chars().any(char::is_whitespace)
        && !value.chars().any(char::is_control)
}

/// Failure while validating a borrowed process event slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterEventsError {
    /// The caller-supplied event bound cannot be represented by platform `usize`.
    LimitNotRepresentable,
    /// The event slice exceeded the caller-supplied bound.
    TooManyEvents {
        /// Actual event count.
        actual: usize,
        /// Accepted maximum.
        maximum: u32,
    },
    /// An event used an offset outside the current block.
    OffsetOutOfRange {
        /// Zero-based event index.
        index: usize,
        /// Invalid offset.
        offset: u32,
        /// Block frame count.
        frame_count: u32,
    },
    /// Source events were not globally nondecreasing by offset.
    NonMonotonicOffsets {
        /// Zero-based event index.
        index: usize,
        /// Previous event offset.
        previous: u32,
        /// Current event offset.
        actual: u32,
    },
    /// A float or linear endpoint was not finite.
    NonFiniteValue {
        /// Zero-based event index.
        index: usize,
    },
    /// A choice identity was empty or contained whitespace/control characters.
    InvalidChoice {
        /// Zero-based event index.
        index: usize,
    },
}

impl fmt::Display for ParameterEventsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LimitNotRepresentable => {
                formatter.write_str("parameter event limit is not representable on this platform")
            }
            Self::TooManyEvents { actual, maximum } => {
                write!(
                    formatter,
                    "parameter event slice has {actual} entries but {maximum} are allowed"
                )
            }
            Self::OffsetOutOfRange {
                index,
                offset,
                frame_count,
            } => write!(
                formatter,
                "parameter event {index} has offset {offset} outside {frame_count}-frame block"
            ),
            Self::NonMonotonicOffsets {
                index,
                previous,
                actual,
            } => write!(
                formatter,
                "parameter event {index} offset {actual} follows previous offset {previous}"
            ),
            Self::NonFiniteValue { index } => {
                write!(formatter, "parameter event {index} has a non-finite value")
            }
            Self::InvalidChoice { index } => {
                write!(
                    formatter,
                    "parameter event {index} has an invalid choice identity"
                )
            }
        }
    }
}

impl std::error::Error for ParameterEventsError {}

/// Failure while evaluating a floating-point trajectory cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterCursorError {
    /// Cursor base value was not finite.
    NonFiniteInitial,
    /// Query offset was outside the validated block.
    OffsetOutOfRange {
        /// Invalid query offset.
        offset: u32,
        /// Block frame count.
        frame_count: u32,
    },
    /// Query offsets must be nondecreasing.
    NonMonotonicQuery {
        /// Earlier query offset.
        previous: u32,
        /// New query offset.
        actual: u32,
    },
    /// A set event for the cursor target was not floating-point.
    NonFloatValue {
        /// Zero-based event index.
        index: usize,
    },
}

impl fmt::Display for ParameterCursorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteInitial => formatter.write_str("parameter cursor base is not finite"),
            Self::OffsetOutOfRange {
                offset,
                frame_count,
            } => write!(
                formatter,
                "parameter cursor offset {offset} outside {frame_count}-frame block"
            ),
            Self::NonMonotonicQuery { previous, actual } => write!(
                formatter,
                "parameter cursor offset {actual} follows previous offset {previous}"
            ),
            Self::NonFloatValue { index } => {
                write!(formatter, "parameter event {index} is not floating-point")
            }
        }
    }
}

impl std::error::Error for ParameterCursorError {}

#[cfg(test)]
mod tests {
    use super::*;

    const GAIN: ParameterIndex = ParameterIndex::new(0);
    const OTHER: ParameterIndex = ParameterIndex::new(1);

    #[test]
    fn validates_bounded_sorted_events() {
        let events = [
            ParameterEvent::set(0, GAIN, ParameterEventValue::Float(0.5)),
            ParameterEvent::linear(2, GAIN, 1.0),
        ];
        let validated = ParameterEvents::new(&events, 4, 2).expect("events are valid");
        assert_eq!(validated.len(), 2);
        assert_eq!(validated.frame_count(), 4);

        assert_eq!(
            ParameterEvents::new(&events, 4, 1),
            Err(ParameterEventsError::TooManyEvents {
                actual: 2,
                maximum: 1,
            })
        );

        let equal_offset = [
            ParameterEvent::set(1, GAIN, ParameterEventValue::Float(0.25)),
            ParameterEvent::set(1, GAIN, ParameterEventValue::Float(0.75)),
        ];
        let equal_offset =
            ParameterEvents::new(&equal_offset, 4, 2).expect("equal offsets retain source order");
        let mut events = equal_offset.iter();
        assert_eq!(
            events.next().expect("first event exists").change(),
            ParameterEventChange::Set(ParameterEventValue::Float(0.25))
        );
        assert_eq!(
            events.next().expect("second event exists").change(),
            ParameterEventChange::Set(ParameterEventValue::Float(0.75))
        );
    }

    #[test]
    fn rejects_malformed_event_order_and_values() {
        let reversed = [
            ParameterEvent::set(2, GAIN, ParameterEventValue::Float(0.5)),
            ParameterEvent::set(1, GAIN, ParameterEventValue::Float(0.25)),
        ];
        assert!(matches!(
            ParameterEvents::new(&reversed, 4, 2),
            Err(ParameterEventsError::NonMonotonicOffsets { .. })
        ));

        let invalid_choice = [ParameterEvent::set(
            0,
            GAIN,
            ParameterEventValue::Choice("bad choice"),
        )];
        assert_eq!(
            ParameterEvents::new(&invalid_choice, 4, 1),
            Err(ParameterEventsError::InvalidChoice { index: 0 })
        );

        let non_finite = [ParameterEvent::linear(0, GAIN, f64::NAN)];
        assert_eq!(
            ParameterEvents::new(&non_finite, 4, 1),
            Err(ParameterEventsError::NonFiniteValue { index: 0 })
        );
    }

    #[test]
    fn cursor_evaluates_sets_and_linear_segments_without_copying() {
        let events = [
            ParameterEvent::set(1, GAIN, ParameterEventValue::Float(0.5)),
            ParameterEvent::set(2, OTHER, ParameterEventValue::Boolean(true)),
            ParameterEvent::linear(3, GAIN, 0.0),
        ];
        let events = ParameterEvents::new(&events, 4, 3).expect("events are valid");
        let mut cursor = events
            .float_cursor(GAIN, 1.0)
            .expect("cursor base is valid");

        assert_eq!(cursor.value_at(0), Ok(1.0));
        assert_eq!(cursor.value_at(1), Ok(0.5));
        assert_eq!(cursor.value_at(2), Ok(0.25));
        assert_eq!(cursor.value_at(3), Ok(0.0));
        assert_eq!(
            cursor.value_at(2),
            Err(ParameterCursorError::NonMonotonicQuery {
                previous: 3,
                actual: 2,
            })
        );
    }

    #[test]
    fn cursor_interpolates_extreme_finite_endpoints_without_nan() {
        let events = [
            ParameterEvent::set(0, GAIN, ParameterEventValue::Float(-f64::MAX)),
            ParameterEvent::linear(2, GAIN, f64::MAX),
        ];
        let events = ParameterEvents::new(&events, 3, 2).expect("events are finite");
        let mut cursor = events
            .float_cursor(GAIN, 0.0)
            .expect("cursor base is valid");

        assert_eq!(cursor.value_at(0), Ok(-f64::MAX));
        assert!(
            cursor
                .value_at(1)
                .expect("middle point is valid")
                .is_finite()
        );
        assert_eq!(cursor.value_at(2), Ok(f64::MAX));
    }
}
