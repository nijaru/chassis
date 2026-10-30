//! Format-independent processor activation and per-call process contracts.

use core::{fmt, num::NonZeroU32};

use crate::{
    audio::AudioIoConfiguration,
    automation::{ParameterEvent, ParameterEvents},
    buffer::ChannelBuffer,
    events::{
        EventDialect, EventOutputError, EventPortDescriptor, EventPortDirection, EventSink,
        NoteEvent, NoteEventKind, NoteEvents, OutputEvent,
    },
    midi::{MidiEvent, MidiEventLimits, MidiEvents, MidiMessage},
    parameters::{ParameterAutomationError, ParameterStore},
    schema::ComponentSchema,
    transport::TransportSnapshot,
};

/// Scheduling/quality context for one processing call.
///
/// This is semantic rather than format-specific. Backends emit only modes they
/// can represent. VST3 prefetch maps to [`Self::BufferedRealtime`]; CLAP core
/// render mode maps realtime/offline and does not emit the buffered mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProcessMode {
    /// The caller has a realtime delivery deadline.
    Realtime,
    /// Processing may be scheduled ahead/irregularly but must retain
    /// realtime-quality/nonblocking behavior rather than wait for wall-clock
    /// realtime or choose an offline-only algorithm.
    BufferedRealtime,
    /// Non-realtime rendering where a product may deliberately choose a more
    /// expensive offline-quality path if it supports one.
    Offline,
}

/// Reference into one of the normalized borrowed input families.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEventIndex {
    /// Index in the parameter stream (including global modulation).
    Parameter(u32),
    /// Index in the semantic note stream.
    Note(u32),
    /// Index in the opaque MIDI 1/SysEx and MIDI 2/UMP stream.
    Midi(u32),
}

/// Borrowed event in an authoritative mixed source order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputEventRef<'a> {
    /// Parameter automation or modulation.
    Parameter(&'a ParameterEvent),
    /// Semantic note operation or expression.
    Note(&'a NoteEvent),
    /// Opaque MIDI event, borrowing source storage for `SysEx`.
    Midi(MidiEvent<'a>),
}

impl InputEventRef<'_> {
    /// Return the block-relative sample offset.
    #[must_use]
    pub const fn offset(self) -> u32 {
        match self {
            Self::Parameter(event) => event.offset(),
            Self::Note(event) => event.offset(),
            Self::Midi(event) => event.offset(),
        }
    }
}

/// Invalid claimed source ordering. Validation never materializes event payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEventOrderError {
    /// Families describe different block sizes.
    FrameCountMismatch,
    /// The order must contain exactly as many indices as all families combined.
    EventCountMismatch,
    /// An index is unknown, repeated, or not the next index of its family.
    InvalidIndex {
        /// Position in the supplied order.
        position: usize,
        /// Invalid family reference.
        event: InputEventIndex,
    },
    /// Mixed event offsets decrease.
    NonMonotonicOffsets,
}

impl fmt::Display for InputEventOrderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FrameCountMismatch => {
                formatter.write_str("input families describe different blocks")
            }
            Self::EventCountMismatch => {
                formatter.write_str("input order does not cover all families")
            }
            Self::InvalidIndex { position, event } => write!(
                formatter,
                "invalid input order index {event:?} at {position}"
            ),
            Self::NonMonotonicOffsets => formatter.write_str("mixed input offsets decreased"),
        }
    }
}

impl std::error::Error for InputEventOrderError {}

/// Borrowed semantic context for one process block.
#[derive(Debug, Clone, Copy)]
pub struct ProcessContext<'a> {
    mode: ProcessMode,
    transport: &'a TransportSnapshot,
    parameter_events: ParameterEvents<'a>,
    note_events: NoteEvents<'a>,
    midi_events: MidiEvents<'a>,
    input_order: Option<&'a [InputEventIndex]>,
    parameters_visible: bool,
}

impl<'a> ProcessContext<'a> {
    /// Construct process context with empty note and MIDI families.
    #[must_use]
    pub const fn new(
        mode: ProcessMode,
        transport: &'a TransportSnapshot,
        parameter_events: ParameterEvents<'a>,
    ) -> Self {
        let frame_count = parameter_events.frame_count();
        Self::with_events(
            mode,
            transport,
            parameter_events,
            NoteEvents::empty_for_block(frame_count),
            MidiEvents::empty_for_block(frame_count),
        )
    }

    /// Construct process context with validated semantic note events.
    ///
    /// Frame-count and activation-owned event-count consistency are checked when
    /// the context is turned into a [`ProcessBlock`].
    #[must_use]
    pub const fn with_note_events(
        mode: ProcessMode,
        transport: &'a TransportSnapshot,
        parameter_events: ParameterEvents<'a>,
        note_events: NoteEvents<'a>,
    ) -> Self {
        Self::with_events(
            mode,
            transport,
            parameter_events,
            note_events,
            MidiEvents::empty_for_block(parameter_events.frame_count()),
        )
    }

    /// Construct context with all input families. Frame/count consistency is
    /// checked against activation bounds before DSP. Independent streams have
    /// no total ordering unless one is explicitly supplied with `with_input_order`.
    #[must_use]
    pub const fn with_events(
        mode: ProcessMode,
        transport: &'a TransportSnapshot,
        parameter_events: ParameterEvents<'a>,
        note_events: NoteEvents<'a>,
        midi_events: MidiEvents<'a>,
    ) -> Self {
        Self {
            mode,
            transport,
            parameter_events,
            note_events,
            midi_events,
            input_order: None,
            parameters_visible: true,
        }
    }

    /// Attach source-provided total order, preserving every event exactly once.
    /// Work is linear in the already bounded family counts.
    ///
    /// # Errors
    /// Rejects missing/repeated/unknown indices, per-family reordering, decreasing
    /// mixed offsets, or families from different blocks. Even when parameters
    /// are withheld, validation covers the original complete streams.
    pub fn with_input_order(
        mut self,
        order: &'a [InputEventIndex],
    ) -> Result<Self, InputEventOrderError> {
        if self.parameter_events.frame_count() != self.note_events.frame_count()
            || self.parameter_events.frame_count() != self.midi_events.frame_count()
        {
            return Err(InputEventOrderError::FrameCountMismatch);
        }
        if self
            .parameter_events
            .len()
            .checked_add(self.note_events.len())
            .and_then(|count| count.checked_add(self.midi_events.len()))
            != Some(order.len())
        {
            return Err(InputEventOrderError::EventCountMismatch);
        }
        let mut parameters = self.parameter_events.iter().enumerate();
        let mut notes = self.note_events.iter().enumerate();
        let mut midi = self.midi_events.iter().enumerate();
        let mut previous = None;
        for (position, &event) in order.iter().enumerate() {
            let offset = match event {
                InputEventIndex::Parameter(index) => parameters.next().and_then(|(next, event)| {
                    (usize::try_from(index).ok() == Some(next)).then_some(event.offset())
                }),
                InputEventIndex::Note(index) => notes.next().and_then(|(next, event)| {
                    (usize::try_from(index).ok() == Some(next)).then_some(event.offset())
                }),
                InputEventIndex::Midi(index) => midi.next().and_then(|(next, event)| {
                    (usize::try_from(index).ok() == Some(next)).then_some(event.offset())
                }),
            }
            .ok_or(InputEventOrderError::InvalidIndex { position, event })?;
            if previous.is_some_and(|previous| offset < previous) {
                return Err(InputEventOrderError::NonMonotonicOffsets);
            }
            previous = Some(offset);
        }
        self.input_order = Some(order);
        Ok(self)
    }

    /// Withhold parameter trajectories without rebuilding the order or losing
    /// relative note/MIDI order. The original parameter stream stays private.
    #[must_use]
    pub const fn without_parameter_events(mut self) -> Self {
        self.parameters_visible = false;
        self
    }

    /// Iterate real source order, or return `None` for independent families.
    /// Withheld parameters are filtered without allocation or index rewriting.
    #[must_use]
    #[expect(
        clippy::missing_panics_doc,
        reason = "private order is validated at construction; family streams cannot change"
    )]
    pub fn ordered_events(self) -> Option<impl Iterator<Item = InputEventRef<'a>> + use<'a>> {
        let order = self.input_order?;
        let mut parameters = self.parameter_events.iter();
        let mut notes = self.note_events.iter();
        let mut midi = self.midi_events.iter();
        Some(order.iter().filter_map(move |index| match index {
            InputEventIndex::Parameter(_) => {
                let event = parameters
                    .next()
                    .expect("validated input order covers parameters");
                self.parameters_visible
                    .then_some(InputEventRef::Parameter(event))
            }
            InputEventIndex::Note(_) => Some(InputEventRef::Note(
                notes.next().expect("validated input order covers notes"),
            )),
            InputEventIndex::Midi(_) => Some(InputEventRef::Midi(
                midi.next().expect("validated input order covers MIDI"),
            )),
        }))
    }

    /// Return the scheduling/quality mode for this block.
    #[must_use]
    pub const fn mode(self) -> ProcessMode {
        self.mode
    }

    /// Return the block-start transport snapshot.
    #[must_use]
    pub const fn transport(self) -> &'a TransportSnapshot {
        self.transport
    }

    pub(crate) const fn original_parameter_events(self) -> ParameterEvents<'a> {
        self.parameter_events
    }

    /// Return the validated borrowed parameter events, unless withheld.
    #[must_use]
    pub const fn parameter_events(self) -> ParameterEvents<'a> {
        if self.parameters_visible {
            self.parameter_events
        } else {
            ParameterEvents::empty_for_block(self.parameter_events.frame_count())
        }
    }

    /// Return the validated borrowed semantic note events.
    #[must_use]
    pub const fn note_events(self) -> NoteEvents<'a> {
        self.note_events
    }

    /// Return the validated borrowed MIDI input view.
    #[must_use]
    pub const fn midi_events(self) -> MidiEvents<'a> {
        self.midi_events
    }
}

/// Resource bounds/configuration supplied when a processor is activated.
///
/// Per-call scheduling mode is intentionally separate: some formats can change
/// realtime/prefetch scheduling without reactivation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProcessConfig {
    sample_rate: f64,
    guaranteed_min_frames: Option<NonZeroU32>,
    max_frames: NonZeroU32,
    max_parameter_events: u32,
    max_note_events: u32,
    max_output_note_events: u32,
    midi_input_limits: MidiEventLimits,
    midi_output_limits: MidiEventLimits,
}

impl ProcessConfig {
    /// Construct validated activation/resource bounds.
    ///
    /// `guaranteed_min_frames` is `None` when a backend does not promise a
    /// positive minimum. That is distinct from whether a particular backend may
    /// legally issue a zero-frame process call. `max_parameter_events` is the
    /// activation-owned bound for normalized parameter events in one callback;
    /// zero means that the component/adapter accepts no parameter events.
    /// Note input/output and MIDI count/byte capacities default to zero; enable
    /// them with [`Self::with_max_note_events`],
    /// [`Self::with_max_output_note_events`], [`Self::with_midi_input_limits`],
    /// and [`Self::with_midi_output_limits`].
    ///
    /// # Errors
    ///
    /// Returns [`ProcessConfigError::InvalidSampleRate`] when `sample_rate` is
    /// non-finite or not positive. Returns
    /// [`ProcessConfigError::InvalidFrameRange`] when a guaranteed minimum is
    /// greater than `max_frames`.
    pub fn new(
        sample_rate: f64,
        guaranteed_min_frames: Option<NonZeroU32>,
        max_frames: NonZeroU32,
        max_parameter_events: u32,
    ) -> Result<Self, ProcessConfigError> {
        if !sample_rate.is_finite() || sample_rate <= 0.0 {
            return Err(ProcessConfigError::InvalidSampleRate);
        }
        if guaranteed_min_frames.is_some_and(|minimum| minimum > max_frames) {
            return Err(ProcessConfigError::InvalidFrameRange);
        }

        Ok(Self {
            sample_rate,
            guaranteed_min_frames,
            max_frames,
            max_parameter_events,
            max_note_events: 0,
            max_output_note_events: 0,
            midi_input_limits: MidiEventLimits::new(0, 0),
            midi_output_limits: MidiEventLimits::new(0, 0),
        })
    }

    /// Set the maximum semantic note events accepted per callback.
    #[must_use]
    pub const fn with_max_note_events(mut self, max_note_events: u32) -> Self {
        self.max_note_events = max_note_events;
        self
    }

    /// Set the maximum semantic note-output write attempts per callback.
    /// Every attempt consumes budget, including validation and sink failures.
    #[must_use]
    pub const fn with_max_output_note_events(mut self, maximum: u32) -> Self {
        self.max_output_note_events = maximum;
        self
    }

    /// Set MIDI input event and aggregate `SysEx` bounds.
    #[must_use]
    pub const fn with_midi_input_limits(mut self, limits: MidiEventLimits) -> Self {
        self.midi_input_limits = limits;
        self
    }

    /// Set MIDI output attempt and aggregate `SysEx` bounds. Rejected downstream
    /// writes do not refund either budget.
    #[must_use]
    pub const fn with_midi_output_limits(mut self, limits: MidiEventLimits) -> Self {
        self.midi_output_limits = limits;
        self
    }

    /// Return MIDI input limits (zero by default).
    #[must_use]
    pub const fn midi_input_limits(self) -> MidiEventLimits {
        self.midi_input_limits
    }

    /// Return MIDI output limits (zero by default).
    #[must_use]
    pub const fn midi_output_limits(self) -> MidiEventLimits {
        self.midi_output_limits
    }

    /// Return the note-output attempt budget (zero by default).
    #[must_use]
    pub const fn max_output_note_events(self) -> u32 {
        self.max_output_note_events
    }

    /// Return the activation sample rate in Hz.
    #[must_use]
    pub const fn sample_rate(self) -> f64 {
        self.sample_rate
    }

    /// Return a positive lower block-size guarantee when the backend has one.
    ///
    /// `None` means no positive minimum is promised; it does not by itself say
    /// whether a zero-frame process call is legal.
    #[must_use]
    pub const fn guaranteed_min_frames(self) -> Option<NonZeroU32> {
        self.guaranteed_min_frames
    }

    /// Return the non-zero maximum frame count provisioned for this activation.
    #[must_use]
    pub const fn max_frames(self) -> NonZeroU32 {
        self.max_frames
    }

    /// Return the maximum normalized parameter events accepted per callback.
    #[must_use]
    pub const fn max_parameter_events(self) -> u32 {
        self.max_parameter_events
    }

    /// Return the maximum semantic note events accepted per callback.
    #[must_use]
    pub const fn max_note_events(self) -> u32 {
        self.max_note_events
    }
}

/// Invalid processor activation configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessConfigError {
    /// Sample rate was zero, negative, NaN, or infinite.
    InvalidSampleRate,
    /// A known minimum block size exceeded the maximum.
    InvalidFrameRange,
}

impl fmt::Display for ProcessConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSampleRate => {
                formatter.write_str("sample rate must be finite and positive")
            }
            Self::InvalidFrameRange => {
                formatter.write_str("guaranteed minimum frame count must not exceed maximum")
            }
        }
    }
}

impl std::error::Error for ProcessConfigError {}

/// Immutable configuration accepted for one processor activation.
///
/// Product code receives this only after framework structural validation and
/// the component schema's whole-audio-I/O policy have accepted the configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActivationConfig<'a> {
    process: ProcessConfig,
    audio_io: AudioIoConfiguration<'a>,
    schema: &'a ComponentSchema,
}

impl<'a> ActivationConfig<'a> {
    pub(crate) const fn new(
        process: ProcessConfig,
        audio_io: AudioIoConfiguration<'a>,
        schema: &'a ComponentSchema,
    ) -> Self {
        Self {
            process,
            audio_io,
            schema,
        }
    }

    /// Return the process resource bounds.
    #[must_use]
    pub const fn process(self) -> ProcessConfig {
        self.process
    }

    /// Return the accepted whole-component audio configuration.
    #[must_use]
    pub const fn audio_io(self) -> AudioIoConfiguration<'a> {
        self.audio_io
    }

    /// Return the immutable component schema generation frozen by the runtime.
    #[must_use]
    pub const fn schema(self) -> &'a ComponentSchema {
        self.schema
    }
}

/// Reiterable, allocation-free source of safe process channel views.
///
/// Each call must invoke the visitor exactly once with a finite iterator over
/// the complete topology. Construct the iterator on the stack; do not collect
/// channel views. Its sample borrows may coexist inside the visitor, enabling
/// cross-channel DSP without scratch copies, but cannot escape that call.
///
/// Traversals must repeat the same endpoints, relationships, and dimensions.
/// Every active endpoint appears exactly once; silence requires an explicit
/// view. The runtime stops iteration on the first invalid endpoint, bounding
/// topology validation by the activation's endpoint count plus one view.
pub trait ProcessBufferSource<S> {
    /// Lend a stack-owned iterator without allocating or retaining sample views.
    fn with_channels(
        &mut self,
        visitor: &mut dyn FnMut(&mut dyn Iterator<Item = ChannelBuffer<'_, S>>),
    );
}

/// Allocation-free source over an already-materialized channel slice.
pub struct ChannelBufferSlice<'buffers, 'samples, S> {
    buffers: &'buffers mut [ChannelBuffer<'samples, S>],
}

impl<'buffers, 'samples, S> ChannelBufferSlice<'buffers, 'samples, S> {
    /// Borrow an existing slice as a process buffer source.
    #[must_use]
    pub fn new(buffers: &'buffers mut [ChannelBuffer<'samples, S>]) -> Self {
        Self { buffers }
    }

    /// Return the underlying materialized channel slice.
    #[must_use]
    pub fn as_mut_slice(&mut self) -> &mut [ChannelBuffer<'samples, S>] {
        self.buffers
    }
}

impl<S> ProcessBufferSource<S> for ChannelBufferSlice<'_, '_, S> {
    fn with_channels(
        &mut self,
        visitor: &mut dyn FnMut(&mut dyn Iterator<Item = ChannelBuffer<'_, S>>),
    ) {
        visitor(&mut self.buffers.iter_mut().map(ChannelBuffer::reborrow));
    }
}

/// Borrowed realtime process call passed to product DSP.
///
/// The buffer source already contains or can lazily construct safe channel views
/// whose aliasing has been proved by the adapter/runtime boundary. Construction
/// validates only facts that can vary per callback: frame-count bounds, event
/// bounds/context, and source dimensions. Stable endpoint/schema mapping should
/// be resolved outside the hot path rather than rescanned with allocation or
/// quadratic semantic lookup work every block.
pub struct ProcessBlock<'a, S> {
    frame_count: u32,
    context: ProcessContext<'a>,
    parameters: &'a ParameterStore,
    modulation: &'a [f64],
    source: &'a mut dyn ProcessBufferSource<S>,
    event_sink: Option<&'a mut dyn EventSink>,
    event_ports: &'a [EventPortDescriptor],
    remaining_note_attempts: u32,
    remaining_midi_attempts: u32,
    remaining_sysex_bytes: u32,
    previous_output_offset: Option<u32>,
}

impl<'a, S> ProcessBlock<'a, S> {
    // Only InstanceRuntime constructs production blocks, after complete callback
    // admission. Do not rescan the context or lazy source at this projection.
    pub(crate) fn from_validated(
        config: &ActivationConfig<'a>,
        parameters: &'a ParameterStore,
        modulation: &'a [f64],
        frame_count: u32,
        context: ProcessContext<'a>,
        source: &'a mut dyn ProcessBufferSource<S>,
    ) -> Self {
        Self {
            frame_count,
            context,
            parameters,
            modulation,
            source,
            event_sink: None,
            event_ports: config.schema().event_ports(),
            remaining_note_attempts: config.process().max_output_note_events(),
            remaining_midi_attempts: config.process().midi_output_limits().max_events(),
            remaining_sysex_bytes: config.process().midi_output_limits().max_sysex_bytes(),
            previous_output_offset: None,
        }
    }

    pub(crate) fn validate_context(
        process: ProcessConfig,
        frame_count: u32,
        context: ProcessContext<'_>,
    ) -> Result<usize, ProcessBlockError> {
        if context.original_parameter_events().frame_count() != frame_count {
            return Err(ProcessBlockError::ParameterEventFrameCountMismatch {
                expected: frame_count,
                actual: context.original_parameter_events().frame_count(),
            });
        }
        let maximum_parameter_events = usize::try_from(process.max_parameter_events())
            .map_err(|_| ProcessBlockError::ParameterEventLimitNotRepresentable)?;
        if context.original_parameter_events().len() > maximum_parameter_events {
            return Err(ProcessBlockError::ParameterEventCountTooLarge {
                actual: context.original_parameter_events().len(),
                maximum: process.max_parameter_events(),
            });
        }
        if context.note_events().frame_count() != frame_count {
            return Err(ProcessBlockError::NoteEventFrameCountMismatch {
                expected: frame_count,
                actual: context.note_events().frame_count(),
            });
        }
        let maximum_note_events = usize::try_from(process.max_note_events())
            .map_err(|_| ProcessBlockError::NoteEventLimitNotRepresentable)?;
        if context.note_events().len() > maximum_note_events {
            return Err(ProcessBlockError::NoteEventCountTooLarge {
                actual: context.note_events().len(),
                maximum: process.max_note_events(),
            });
        }
        let midi = context.midi_events();
        if midi.frame_count() != frame_count {
            return Err(ProcessBlockError::MidiEventFrameCountMismatch {
                expected: frame_count,
                actual: midi.frame_count(),
            });
        }
        let limits = process.midi_input_limits();
        let maximum = usize::try_from(limits.max_events())
            .map_err(|_| ProcessBlockError::MidiEventLimitNotRepresentable)?;
        if midi.len() > maximum {
            return Err(ProcessBlockError::MidiEventCountTooLarge {
                actual: midi.len(),
                maximum: limits.max_events(),
            });
        }
        if midi.sysex_bytes() > limits.max_sysex_bytes() {
            return Err(ProcessBlockError::MidiSysExBytesTooLarge {
                actual: midi.sysex_bytes(),
                maximum: limits.max_sysex_bytes(),
            });
        }
        if let Some(minimum) = process.guaranteed_min_frames()
            && frame_count < minimum.get()
        {
            return Err(ProcessBlockError::BelowGuaranteedMinimum {
                actual: frame_count,
                minimum: minimum.get(),
            });
        }

        if frame_count > process.max_frames().get() {
            return Err(ProcessBlockError::ExceedsActivatedMaximum {
                actual: frame_count,
                maximum: process.max_frames().get(),
            });
        }

        usize::try_from(frame_count).map_err(|_| ProcessBlockError::FrameCountNotRepresentable)
    }

    /// Return the actual number of frames in this callback.
    #[must_use]
    pub const fn frame_count(&self) -> u32 {
        self.frame_count
    }

    /// Return the scheduling/quality mode for this callback.
    #[must_use]
    pub const fn mode(&self) -> ProcessMode {
        self.context.mode()
    }

    /// Return the complete semantic context for this callback.
    #[must_use]
    pub const fn context(&self) -> ProcessContext<'a> {
        self.context
    }

    /// Return the block-start transport snapshot.
    #[must_use]
    pub const fn transport(&self) -> &'a TransportSnapshot {
        self.context.transport()
    }

    /// Return validated borrowed parameter automation events.
    #[must_use]
    pub const fn parameter_events(&self) -> ParameterEvents<'a> {
        self.context.parameter_events()
    }

    /// Return validated borrowed semantic note events.
    #[must_use]
    pub const fn note_events(&self) -> NoteEvents<'a> {
        self.context.note_events()
    }

    /// Iterate authoritative mixed input order when the source supplied one.
    #[must_use]
    pub fn ordered_events(&self) -> Option<impl Iterator<Item = InputEventRef<'a>> + use<'a, S>> {
        self.context.ordered_events()
    }

    /// Return validated MIDI input without copying payloads.
    #[must_use]
    pub const fn midi_events(&self) -> MidiEvents<'a> {
        self.context.midi_events()
    }

    pub(crate) fn set_event_sink(&mut self, sink: Option<&'a mut dyn EventSink>) {
        self.event_sink = sink;
    }

    /// Attempt semantic note output. All attempts consume the note budget,
    /// including invalid events and backpressure. End feedback addresses INPUT
    /// note ports; other notes address OUTPUT note ports. Wildcard ports require
    /// at least one compatible target. Successful insertions across ALL output
    /// families must have nondecreasing offsets.
    ///
    /// # Errors
    /// Returns explicit budget, sink, offset/order, or target errors. Nothing is
    /// allocated, retried, or silently dropped by the runtime.
    pub fn try_push_note_event(&mut self, event: NoteEvent) -> Result<(), EventOutputError> {
        if self.remaining_note_attempts == 0 {
            return Err(EventOutputError::LimitExceeded);
        }
        self.remaining_note_attempts -= 1;
        self.push_output(OutputEvent::Note(event))
    }

    /// Attempt opaque MIDI output on a concrete, dialect-compatible OUTPUT port.
    /// Every call consumes a MIDI attempt. `SysEx` bytes are charged before validation/dispatch
    /// and never refunded, even on target rejection. An over-budget payload is
    /// not dispatched. Payloads are borrowed only until the sink returns.
    ///
    /// # Errors
    /// Returns count/byte budget, sink, offset/order, or target rejection errors.
    pub fn try_push_midi_event(&mut self, event: MidiEvent<'_>) -> Result<(), EventOutputError> {
        if self.remaining_midi_attempts == 0 {
            return Err(EventOutputError::LimitExceeded);
        }
        self.remaining_midi_attempts -= 1;
        if let MidiMessage::SysEx(bytes) = event.message() {
            let length =
                u32::try_from(bytes.len()).map_err(|_| EventOutputError::SysExBytesExceeded)?;
            self.remaining_sysex_bytes = self
                .remaining_sysex_bytes
                .checked_sub(length)
                .ok_or(EventOutputError::SysExBytesExceeded)?;
        }
        self.push_output(OutputEvent::Midi(event))
    }

    fn push_output(&mut self, event: OutputEvent<'_>) -> Result<(), EventOutputError> {
        let sink = self.event_sink.as_mut().ok_or(EventOutputError::NoSink)?;
        if event.offset() >= self.frame_count {
            return Err(EventOutputError::OffsetOutOfRange);
        }
        if self
            .previous_output_offset
            .is_some_and(|previous| event.offset() < previous)
        {
            return Err(EventOutputError::NonMonotonicOffsets);
        }
        let (target, direction, dialect) = match event {
            OutputEvent::Note(note) => (
                note.kind().address().port(),
                if matches!(note.kind(), NoteEventKind::End { .. }) {
                    EventPortDirection::Input
                } else {
                    EventPortDirection::Output
                },
                EventDialect::Notes,
            ),
            OutputEvent::Midi(midi) => (
                Some(midi.port()),
                EventPortDirection::Output,
                midi.message().dialect(),
            ),
        };
        let compatible = |port: &EventPortDescriptor| {
            port.direction == direction && port.dialects.contains(&dialect)
        };
        let valid_target = match target {
            Some(index) => usize::try_from(index.get())
                .ok()
                .and_then(|index| self.event_ports.get(index))
                .is_some_and(compatible),
            None => self.event_ports.iter().any(compatible),
        };
        if !valid_target {
            return Err(EventOutputError::TargetRejected);
        }
        sink.try_push(event)?;
        self.previous_output_offset = Some(event.offset());
        Ok(())
    }

    /// Return the current validated base/control parameter values.
    #[must_use]
    pub const fn parameters(&self) -> &'a ParameterStore {
        self.parameters
    }

    /// Return the consumed held amount at block start, before timed events.
    ///
    /// This is the frozen signal when custom resources await reactivation,
    /// not an independently sampled desired control value.
    #[must_use]
    pub fn modulation(&self, parameter: crate::parameters::ParameterIndex) -> Option<f64> {
        self.modulation
            .get(usize::try_from(parameter.get()).ok()?)
            .copied()
    }

    /// Borrow an iterator for cross-channel algorithms, retaining any number of
    /// disjoint channel views together inside the closure without allocating.
    /// Sample references cannot escape the closure.
    ///
    /// Sample views cannot be retained outside the traversal:
    ///
    /// ```compile_fail
    /// use chassis_runtime::process::ProcessBlock;
    /// fn retain(block: &mut ProcessBlock<'_, f32>) {
    ///     let mut escaped = None;
    ///     block.with_channels(|channels| escaped = channels.next());
    ///     drop(escaped);
    /// }
    /// ```
    ///
    /// # Panics
    /// Panics if a buffer source violates its exactly-once visitor contract.
    pub fn with_channels<R>(
        &mut self,
        mut visitor: impl FnMut(&mut dyn Iterator<Item = ChannelBuffer<'_, S>>) -> R,
    ) -> R {
        let mut result = None;
        self.source.with_channels(&mut |channels| {
            assert!(
                result.is_none(),
                "buffer source must invoke its visitor exactly once"
            );
            result = Some(visitor(channels));
        });
        result.expect("buffer source must invoke its visitor exactly once")
    }

    /// Visit every channel, with simultaneous disjoint input/output access by
    /// matching [`ChannelBuffer::Separate`], or one mutable in-place slice.
    pub fn for_each_channel(&mut self, mut visitor: impl FnMut(ChannelBuffer<'_, S>)) {
        self.with_channels(|channels| {
            for channel in channels {
                visitor(channel);
            }
        });
    }

    /// Visit channels until the first error, without requesting later channels.
    ///
    /// # Errors
    /// Returns the visitor's first error. Earlier sample writes are not rolled back.
    pub fn try_for_each_channel<E>(
        &mut self,
        mut visitor: impl FnMut(ChannelBuffer<'_, S>) -> Result<(), E>,
    ) -> Result<(), E> {
        self.with_channels(|channels| {
            for channel in channels {
                visitor(channel)?;
            }
            Ok(())
        })
    }
}

/// Invalid process callback dimensions or semantic context relative to the active configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessBlockError {
    /// A callback was smaller than a positive minimum guaranteed at activation.
    BelowGuaranteedMinimum {
        /// Actual callback frame count.
        actual: u32,
        /// Guaranteed positive minimum.
        minimum: u32,
    },
    /// A callback exceeded the maximum storage provisioned at activation.
    ExceedsActivatedMaximum {
        /// Actual callback frame count.
        actual: u32,
        /// Activated maximum frame count.
        maximum: u32,
    },
    /// The `u32` callback frame count cannot be represented by platform `usize`.
    FrameCountNotRepresentable,
    /// A safe channel view did not match the callback frame count.
    BufferFrameCountMismatch {
        /// Expected sample count for each channel view.
        expected: usize,
        /// Actual sample count exposed by one channel view.
        actual: usize,
    },
    /// Parameter automation did not match the active schema.
    InvalidParameterEvents(ParameterAutomationError),
    /// Parameter events were validated for another block size.
    ParameterEventFrameCountMismatch {
        /// Actual process block size.
        expected: u32,
        /// Block size used to validate the events.
        actual: u32,
    },
    /// The activation event bound cannot be represented by platform `usize`.
    ParameterEventLimitNotRepresentable,
    /// The callback supplied more parameter events than activation allows.
    ParameterEventCountTooLarge {
        /// Actual event count.
        actual: usize,
        /// Activation-owned maximum.
        maximum: u32,
    },
    /// Note events were validated for another block size.
    NoteEventFrameCountMismatch {
        /// Actual process block size.
        expected: u32,
        /// Block size used to validate the events.
        actual: u32,
    },
    /// The activation note-event bound cannot be represented by platform `usize`.
    NoteEventLimitNotRepresentable,
    /// The callback supplied more note events than activation allows.
    NoteEventCountTooLarge {
        /// Actual event count.
        actual: usize,
        /// Activation-owned maximum.
        maximum: u32,
    },
    /// MIDI events were admitted for a different block size.
    MidiEventFrameCountMismatch {
        /// Actual block size.
        expected: u32,
        /// Admitted block size.
        actual: u32,
    },
    /// MIDI event count bound cannot be represented on this platform.
    MidiEventLimitNotRepresentable,
    /// MIDI count exceeds the activation bound, regardless of admission bounds.
    MidiEventCountTooLarge {
        /// Admitted event count.
        actual: usize,
        /// Activation maximum.
        maximum: u32,
    },
    /// Aggregate `SysEx` bytes exceed the activation bound.
    MidiSysExBytesTooLarge {
        /// Admitted bytes.
        actual: u32,
        /// Activation maximum.
        maximum: u32,
    },
}

impl fmt::Display for ProcessBlockError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BelowGuaranteedMinimum { actual, minimum } => write!(
                formatter,
                "process block has {actual} frames below guaranteed minimum {minimum}"
            ),
            Self::ExceedsActivatedMaximum { actual, maximum } => write!(
                formatter,
                "process block has {actual} frames above activated maximum {maximum}"
            ),
            Self::FrameCountNotRepresentable => {
                formatter.write_str("frame count is not representable on this platform")
            }
            Self::BufferFrameCountMismatch { expected, actual } => write!(
                formatter,
                "channel view has {actual} samples but process block requires {expected}"
            ),
            Self::InvalidParameterEvents(error) => {
                write!(formatter, "invalid parameter events: {error}")
            }
            Self::ParameterEventFrameCountMismatch { expected, actual } => write!(
                formatter,
                "parameter events were validated for {actual} frames but process block has {expected}"
            ),
            Self::ParameterEventLimitNotRepresentable => {
                formatter.write_str("parameter event limit is not representable on this platform")
            }
            Self::ParameterEventCountTooLarge { actual, maximum } => write!(
                formatter,
                "process block has {actual} parameter events but activation allows {maximum}"
            ),
            Self::NoteEventFrameCountMismatch { expected, actual } => write!(
                formatter,
                "note events were validated for {actual} frames but process block has {expected}"
            ),
            Self::NoteEventLimitNotRepresentable => {
                formatter.write_str("note event limit is not representable on this platform")
            }
            Self::NoteEventCountTooLarge { actual, maximum } => write!(
                formatter,
                "process block has {actual} note events but activation allows {maximum}"
            ),
            Self::MidiEventFrameCountMismatch { expected, actual } => write!(
                formatter,
                "MIDI events were validated for {actual} frames but process block has {expected}"
            ),
            Self::MidiEventLimitNotRepresentable => {
                formatter.write_str("MIDI event limit is not representable")
            }
            Self::MidiEventCountTooLarge { actual, maximum } => write!(
                formatter,
                "process block has {actual} MIDI events but activation allows {maximum}"
            ),
            Self::MidiSysExBytesTooLarge { actual, maximum } => write!(
                formatter,
                "process block has {actual} SysEx bytes but activation allows {maximum}"
            ),
        }
    }
}

impl std::error::Error for ProcessBlockError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidParameterEvents(error) => Some(error),
            Self::BelowGuaranteedMinimum { .. }
            | Self::ExceedsActivatedMaximum { .. }
            | Self::FrameCountNotRepresentable
            | Self::BufferFrameCountMismatch { .. }
            | Self::ParameterEventFrameCountMismatch { .. }
            | Self::ParameterEventLimitNotRepresentable
            | Self::ParameterEventCountTooLarge { .. }
            | Self::NoteEventFrameCountMismatch { .. }
            | Self::NoteEventLimitNotRepresentable
            | Self::NoteEventCountTooLarge { .. }
            | Self::MidiEventFrameCountMismatch { .. }
            | Self::MidiEventLimitNotRepresentable
            | Self::MidiEventCountTooLarge { .. }
            | Self::MidiSysExBytesTooLarge { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        audio::{AudioPortIndex, DEFAULT_EFFECT_CONFIGURATION},
        automation::{ParameterEvent, ParameterEventValue},
        buffer::{ChannelBuffer, InputEndpoint, OutputEndpoint},
        events::{
            EventPortIndex, NormalizedValue, NoteAddress, NoteEvent, NoteEventKind, NoteEvents,
        },
        parameters::ParameterIndex,
    };

    fn maximum(value: u32) -> NonZeroU32 {
        NonZeroU32::new(value).expect("test maximum is non-zero")
    }

    fn empty_parameters() -> ParameterStore {
        ParameterStore::new(&[]).expect("empty parameter schema is valid")
    }

    fn default_effect_schema() -> ComponentSchema {
        ComponentSchema::stereo_effect(vec![]).expect("default effect schema is valid")
    }

    fn note_address() -> NoteAddress {
        NoteAddress::new(EventPortIndex::new(0), None, None, None)
    }

    fn velocity(value: f64) -> NormalizedValue {
        NormalizedValue::new(value).expect("test note velocity is valid")
    }

    struct SplitChannelBuffers<'buffers, 'samples, S> {
        first: &'buffers mut [ChannelBuffer<'samples, S>],
        second: &'buffers mut [ChannelBuffer<'samples, S>],
    }

    impl<S> ProcessBufferSource<S> for SplitChannelBuffers<'_, '_, S> {
        fn with_channels(
            &mut self,
            visitor: &mut dyn FnMut(&mut dyn Iterator<Item = ChannelBuffer<'_, S>>),
        ) {
            visitor(
                &mut self
                    .first
                    .iter_mut()
                    .chain(self.second.iter_mut())
                    .map(ChannelBuffer::reborrow),
            );
        }
    }

    #[test]
    fn rejects_invalid_sample_rate() {
        assert_eq!(
            ProcessConfig::new(0.0, None, maximum(512), 0),
            Err(ProcessConfigError::InvalidSampleRate)
        );
    }

    #[test]
    fn rejects_minimum_above_maximum() {
        let minimum = NonZeroU32::new(1024).expect("test minimum is non-zero");

        assert_eq!(
            ProcessConfig::new(48_000.0, Some(minimum), maximum(512), 0),
            Err(ProcessConfigError::InvalidFrameRange)
        );
    }

    #[test]
    fn accepts_unknown_minimum() {
        let config = ProcessConfig::new(48_000.0, None, maximum(2048), 0)
            .expect("test configuration is valid");

        assert!((config.sample_rate() - 48_000.0).abs() <= f64::EPSILON);
        assert_eq!(config.guaranteed_min_frames(), None);
        assert_eq!(config.max_frames(), maximum(2048));
        assert_eq!(config.max_parameter_events(), 0);
        assert_eq!(config.max_note_events(), 0);
    }

    #[test]
    fn process_block_does_not_require_one_flat_slice() {
        let process =
            ProcessConfig::new(48_000.0, None, maximum(8), 0).expect("test configuration is valid");
        let schema = default_effect_schema();
        let activation = ActivationConfig::new(process, DEFAULT_EFFECT_CONFIGURATION, &schema);
        let parameters = empty_parameters();
        let input_left = [1.0_f32, 2.0];
        let input_right = [3.0_f32, 4.0];
        let mut output_left = [0.0_f32; 2];
        let mut output_right = [0.0_f32; 2];
        let mut first = [ChannelBuffer::separate(
            InputEndpoint::new(AudioPortIndex::new(0), 0),
            &input_left,
            OutputEndpoint::new(AudioPortIndex::new(1), 0),
            &mut output_left,
            2,
        )
        .expect("left buffer is valid")];
        let mut second = [ChannelBuffer::separate(
            InputEndpoint::new(AudioPortIndex::new(0), 1),
            &input_right,
            OutputEndpoint::new(AudioPortIndex::new(1), 1),
            &mut output_right,
            2,
        )
        .expect("right buffer is valid")];
        let mut source = SplitChannelBuffers {
            first: &mut first,
            second: &mut second,
        };
        let context = ProcessContext::new(
            ProcessMode::Realtime,
            &TransportSnapshot::UNKNOWN,
            ParameterEvents::empty_for_block(2),
        );
        let mut block =
            ProcessBlock::from_validated(&activation, &parameters, &[], 2, context, &mut source);

        block.for_each_channel(|channel| {
            let ChannelBuffer::Separate {
                input_samples,
                output_samples,
                ..
            } = channel
            else {
                panic!("split source channels are separate");
            };
            for (input, output) in input_samples.iter().zip(output_samples) {
                *output = *input * 0.5;
            }
        });

        let mut visits = 0;
        assert_eq!(
            block.try_for_each_channel(|_| {
                visits += 1;
                Err("stop")
            }),
            Err("stop")
        );
        assert_eq!(visits, 1, "errors must not visit later channels");
        // Retain both lazy views together for a true cross-channel transform,
        // without collecting channels or copying either input into scratch.
        block.with_channels(|channels| {
            let Some(ChannelBuffer::Separate {
                input_samples: left,
                output_samples: mid,
                ..
            }) = channels.next()
            else {
                panic!("left channel is separate");
            };
            let Some(ChannelBuffer::Separate {
                input_samples: right,
                output_samples: side,
                ..
            }) = channels.next()
            else {
                panic!("right channel is separate");
            };
            assert!(channels.next().is_none());
            for ((left, right), (mid, side)) in left.iter().zip(right).zip(mid.iter_mut().zip(side))
            {
                *mid = *left + *right;
                *side = *left - *right;
            }
        });

        assert_eq!(
            output_left.map(f32::to_bits),
            [4.0_f32, 6.0].map(f32::to_bits)
        );
        assert_eq!(
            output_right.map(f32::to_bits),
            [-2.0_f32, -2.0].map(f32::to_bits)
        );
    }

    #[test]
    fn process_context_carries_optional_transport_and_events() {
        let transport = TransportSnapshot::new(crate::transport::TransportValues {
            playing: Some(true),
            recording: Some(false),
            tempo_bpm: Some(120.0),
            sample_position: Some(12),
            ..crate::transport::TransportValues::default()
        })
        .expect("transport snapshot is valid");
        let context = ProcessContext::new(
            ProcessMode::Offline,
            &transport,
            ParameterEvents::empty_for_block(4),
        );

        assert_eq!(context.mode(), ProcessMode::Offline);
        assert_eq!(context.transport().values().playing, Some(true));
        assert_eq!(context.transport().values().recording, Some(false));
        assert_eq!(context.transport().values().tempo_bpm, Some(120.0));
        assert_eq!(context.transport().values().sample_position, Some(12));
        assert!(context.parameter_events().is_empty());
        assert_eq!(context.parameter_events().frame_count(), 4);
        assert!(context.note_events().is_empty());
        assert_eq!(context.note_events().frame_count(), 4);
    }

    #[test]
    fn process_block_exposes_bounded_note_events() {
        let process = ProcessConfig::new(48_000.0, None, maximum(8), 0)
            .expect("test configuration is valid")
            .with_max_note_events(2);
        let schema = default_effect_schema();
        let activation = ActivationConfig::new(process, DEFAULT_EFFECT_CONFIGURATION, &schema);
        let parameters = empty_parameters();
        let raw_notes = [NoteEvent::new(
            1,
            NoteEventKind::On {
                address: note_address(),
                velocity: velocity(0.75),
            },
        )];
        let notes = NoteEvents::new(&raw_notes, 2, 2).expect("note events are valid");
        let context = ProcessContext::with_note_events(
            ProcessMode::Realtime,
            &TransportSnapshot::UNKNOWN,
            ParameterEvents::empty_for_block(2),
            notes,
        );
        let mut buffers: [ChannelBuffer<'_, f32>; 0] = [];
        let mut source = ChannelBufferSlice::new(&mut buffers);
        let block =
            ProcessBlock::from_validated(&activation, &parameters, &[], 2, context, &mut source);

        assert_eq!(block.note_events().len(), 1);
        assert_eq!(block.note_events().iter().next(), Some(&raw_notes[0]));
    }

    #[test]
    fn process_block_rejects_events_for_another_block_size() {
        let process =
            ProcessConfig::new(48_000.0, None, maximum(8), 0).expect("test configuration is valid");
        let context = ProcessContext::new(
            ProcessMode::Realtime,
            &TransportSnapshot::UNKNOWN,
            ParameterEvents::empty_for_block(1),
        );

        assert!(matches!(
            ProcessBlock::<f32>::validate_context(process, 2, context),
            Err(ProcessBlockError::ParameterEventFrameCountMismatch {
                expected: 2,
                actual: 1,
            })
        ));

        let raw_events = [ParameterEvent::set(
            0,
            ParameterIndex::new(0),
            ParameterEventValue::Float(0.5),
        )];
        let events = ParameterEvents::new(&raw_events, 2, 1).expect("event shape is valid");
        let context =
            ProcessContext::new(ProcessMode::Realtime, &TransportSnapshot::UNKNOWN, events);
        assert!(matches!(
            ProcessBlock::<f32>::validate_context(process, 2, context),
            Err(ProcessBlockError::ParameterEventCountTooLarge {
                actual: 1,
                maximum: 0,
            })
        ));
    }

    #[test]
    fn process_block_rejects_invalid_note_event_context() {
        let process =
            ProcessConfig::new(48_000.0, None, maximum(8), 0).expect("test configuration is valid");
        let raw_notes = [NoteEvent::new(
            0,
            NoteEventKind::On {
                address: note_address(),
                velocity: velocity(0.5),
            },
        )];
        let wrong_frames = NoteEvents::new(&raw_notes, 1, 1).expect("note event shape is valid");
        let context = ProcessContext::with_note_events(
            ProcessMode::Realtime,
            &TransportSnapshot::UNKNOWN,
            ParameterEvents::empty_for_block(2),
            wrong_frames,
        );

        assert!(matches!(
            ProcessBlock::<f32>::validate_context(process, 2, context),
            Err(ProcessBlockError::NoteEventFrameCountMismatch {
                expected: 2,
                actual: 1,
            })
        ));

        let notes = NoteEvents::new(&raw_notes, 2, 1).expect("note event shape is valid");
        let context = ProcessContext::with_note_events(
            ProcessMode::Realtime,
            &TransportSnapshot::UNKNOWN,
            ParameterEvents::empty_for_block(2),
            notes,
        );
        assert!(matches!(
            ProcessBlock::<f32>::validate_context(process, 2, context),
            Err(ProcessBlockError::NoteEventCountTooLarge {
                actual: 1,
                maximum: 0,
            })
        ));
    }

    #[test]
    fn process_modes_are_per_call_semantics() {
        assert_ne!(ProcessMode::Realtime, ProcessMode::BufferedRealtime);
        assert_ne!(ProcessMode::BufferedRealtime, ProcessMode::Offline);
    }
}
