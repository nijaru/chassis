# Events, Notes, MIDI, and Transport

Status: semantic notes, seven native expressions, opaque MIDI 1/SysEx and MIDI 2/UMP, bounded shared output, source-defined mixed ordering, and parameter automation/global float modulation are implemented in core and projected through CLAP. Per-note parameter modulation and span processing remain design direction; public API is not frozen.

## Goal

Provide sample-accurate process-time controls/events that work for effects and instruments without inventing ordering or transport semantics the source format did not provide.

## Time model

Every process-time item Chassis normalizes has a block-relative sample offset or a block-start snapshot semantic.

Relevant families include:

```text
parameter automation trajectories
parameter modulation
note on/off/choke/end
note expression
raw MIDI 1 / SysEx / MIDI 2 UMP
event/transport changes only where the backend actually supplies them
```

Do not require one giant materialized event enum/array merely for conceptual uniformity. The runtime can expose typed borrowed cursors plus optional merged/span views when that can be done without allocation or information loss.

## Ordering authority

Ordering guarantees come from the source backend.

- CLAP provides one input event list sorted in sample order. Preserve that order, including the backend's relative order for equal timestamps.
- VST3 supplies parameter changes and `IEventList` separately, plus `ProcessContext`. It therefore does **not** provide one authoritative cross-family ordering between a parameter point and note/event at the same sample merely because both have offsets.
- Other adapters must be reviewed similarly rather than forced into a CLAP-shaped total order.

Chassis guarantees nondecreasing sample offsets within each source sequence and preserves stronger source ordering when it exists. It must not invent causal ordering between independent backend sequences at the same sample.

A generic span helper may group all boundaries occurring at sample `N` and make their typed changes available for the subsequent audio span. If a product genuinely requires a total cross-family order at an identical sample, that is a capability requirement: an adapter lacking such source information cannot manufacture it faithfully.

`ProcessContext::with_events` borrows parameter, note, and MIDI views. `with_input_order` accepts family indices with exact coverage, preserved family order, and nondecreasing offsets. `ordered_events()` returns that source order, or `None` for independent streams. Withholding parameter trajectories during resource replacement filters this view without changing the remaining note/MIDI order.

Output events are emitted in the order required by the target backend; Chassis rejects decreasing offsets among successfully emitted events.

## Transport

The current `TransportSnapshot` exposes optional playing, recording, tempo, and sample position. CLAP currently maps the first three and leaves sample position unavailable. It does not yet expose beats, bars, time signatures, looping, or pre-roll.

The intended broader block-start snapshot retains explicit availability for every field:

- play/record/pre-roll state where available;
- tempo;
- song/sample/beat position;
- time signature;
- bar information;
- loop/cycle range/state.

Do not imply intra-block transport changes universally. Core CLAP `clap_process` supplies a transport snapshot at sample zero and a sample-sorted event list; VST3 provides `ProcessContext` separately for the process call. If a backend/extension genuinely supplies timestamped transport changes, Chassis may expose them as a capability-specific timed source.

Products needing only ordinary synchronization read the block snapshot. Products requiring intra-block tempo/transport changes can declare/check that capability rather than receiving fabricated events.

## Event ports

Audio and event/note ports are distinct schemas.

A component may be:

- audio-only;
- note/event input -> audio output instrument;
- event-only MIDI/note processor;
- multi-event-port where the backend supports it.

Event ports use stable product keys just like audio ports. Runtime/backend numeric indices are derived projections.

Port capabilities should describe dialects/semantics supported rather than forcing every product to claim MIDI, notes, and every future event type.

## Semantic note events

Ordinary instruments should receive semantic note events rather than decode MIDI bytes for note handling.

A note target/address needs enough identity for modern hosts:

```text
NoteAddress
├── stable event port / derived runtime port
├── optional note id
├── optional channel
└── optional key
```

Wildcard/unspecified targeting must use typed semantics rather than raw sentinel integers in product code.

The native semantic families are:

- note on/off;
- forced termination/choke where supported;
- note end/output notification where meaningful;
- velocity;
- tuning;
- per-note expression.

Do not freeze a least-common-denominator type until CLAP/VST3/AU/MIDI mappings are tested. Adapters expose only capabilities whose semantics they can preserve or whose degradation policy is explicit.

## Note expression

`NoteExpression` validates finite held sets for volume (`0 < x <= 4`), pan, tuning (`-120..=120` semitones), vibrato, expression, brightness, and pressure. All dimensions except volume/tuning use `0..=1`. These are note controls, not persistent parameters or per-note parameter modulation.

CLAP maps all seven native dimensions without rescaling. An onset-sample expression must affect all samples generated by the addressed new voice, even when it precedes the note-on in the list. The instrument fixture buffers those preceding expressions within the declared input bound. Expressions at later offsets apply when received.

Keep unlike format-specific dimensions separate unless their conversion semantics/ranges are specified.

## Raw MIDI

`MidiEvent` preserves sample offset, concrete event-port identity, and portable live/do-not-record annotations. `MidiMessage::Short` contains exactly three opaque bytes, including unused bytes supplied by the source. `MidiMessage::SysEx` borrows an opaque payload, including empty or unframed data. Neither path validates musical grammar, normalizes padding, converts zero-velocity note-on, nor parses or reassembles messages.

`MidiEvents` admits an immutable indexed `MidiEventSource` under event-count and aggregate SysEx-byte bounds. Length, metadata, and payloads must remain stable across traversals of one callback. Runtime admission checks the actual cached counts against the activation's `MidiEventLimits`, even if the source view was constructed with looser limits. Payload bytes are not traversed merely to validate lengths.

Payload loans cannot outlive their source. A receiver retaining data must copy it into independently owned bounded storage; callback copies require preallocated capacity. CLAP input payloads live through process, while output payloads need live only through `try_push`. A retaining host must copy both the descriptor and its payload before returning. Copying a Clack event wrapper or `EventBuffer` does not deep-copy SysEx.

`MidiMessage::Ump([u32; 4])` preserves four numeric storage words, including unused words. This is an opaque transport envelope, not a UMP stream parser: it does not infer packet length, validate message grammar, normalize trailing words, swap byte order, or convert protocols. UMP can encode MIDI 1 or MIDI 2 messages; either requires the port's `Midi2` transport capability. Short/SysEx instead require `Midi1`. Each input and output is checked against its actual message dialect and port direction.

All three representations use the same MIDI source, family order, event-count/attempt limits, and sink. Fixed UMP storage consumes event capacity, not SysEx bytes; MIDI 2-only input needs no SysEx budget. Variable-length streams, protocol interpretation, discovery, and MIDI 2 device compliance are outside this contract.

## Parameter automation trajectories

Parameter automation is process-time control data but need not be represented as a literal mixed `Event` enum.

Chassis preserves source semantics at minimum as:

```text
Set
  sample offset -> target value

Linear segment/ramp
  start sample/value -> end sample/value
```

Adapter rules:

- VST3 parameter queues are piecewise-linear approximations; preserve those segments.
- Audio Unit explicit ramp events remain linear ramps with supplied duration/end value.
- CLAP core parameter-value events are timestamped values without a ramp-duration primitive, so preserve them as sets unless another supported capability supplies richer semantics.

Never expand ramps into one allocated event per sample. Parameter cursors evaluate constant/linear spans lazily from bounded state.

Host trajectory reconstruction and product smoothing remain separate. Chassis adds no product smoothing by default. Boolean sets, including explicit bypass, use an allocation-free stepped cursor in this same ordered stream; they do not admit linear ramps or gain a separate timing lane.

## Global parameter modulation

Base sets, linear endpoints, and opted-in global float modulation share one `ParameterEvents` sequence and activation budget, preserving stronger source order such as CLAP's mixed list. Modulation events replace finite held plain-unit additive amounts without overwriting base values. Effective cursors add and clamp; base interpolation ignores modulation points. Reset/reactivation/state load preserve this external signal; saves exclude it. [Parameters/state](parameters-state.md#global-additive-float-modulation) owns the full contract.

CLAP accepts exact global addressing only: each raw note/port/channel/key field must equal `-1`. Unsupported targeting, non-finite amounts, and malformed recognized records fail processing before DSP; void flush rejects the invalid recognized batch before mutation. Do not flatten targeting or split the source list in ways that invent ordering. Per-note parameter modulation remains unsupported; native note expression is a separate supported stream.

## Span processing

A common ergonomic view can expose contiguous audio ranges bounded by any relevant change:

```text
current state for frames 0..17
boundary at 17:
  parameter changes for 17
  note/event changes for 17
frames 17..64
...
```

This groups by timestamp without claiming an undefined order between independent event families at one timestamp.

Within a boundary, typed family views preserve the source order they actually possess. Products that only care about parameters do not pay to materialize notes/MIDI, and vice versa.

A ramp does not create a boundary at every sample; its cursor/trajectory remains evaluable/vectorizable across the span.

## Storage and bounded work

Prefer borrowed adapters/cursors over copying host event arrays.

If a backend requires normalization/merge scratch, allocate it before processing from declared/observed activation requirements. Capacity/overflow behavior must follow the semantic importance of the data:

- do not silently drop automation or note events merely because an arbitrary framework queue filled;
- where loss would violate product correctness, fail/contain according to the adapter contract rather than produce knowingly wrong output;
- display-only telemetry has different drop semantics and belongs to the telemetry contract, not this input-event path.

Choice events carry setup-resolved parameter-local `ChoiceIndex`, not stable identifier text. Stable IDs remain control/persistence metadata; event validation does not scan host-controlled strings.

The processor never triggers unbounded allocation merely because a host delivered many events. Where the host API permits querying event count, validate/bound work against the current process call and implementation limits with an explicit failure path.

## Output events

Instruments/MIDI processors need a realtime-safe output sink that:

- preserves sample offsets;
- reports target rejection/capability limits;
- does not allocate/block;
- exposes only event classes that can be represented;
- keeps product ownership of retry/drop policy when a target cannot accept output.

`ProcessBlock::try_push_note_event` and `try_push_midi_event` share one caller-owned `EventSink` accepting `OutputEvent`. One successful-output offset orders both families; neither can bypass the other. Both validate block offsets and port direction/dialect. MIDI targets a concrete output port supporting the message's `Midi1` or `Midi2` dialect.

`ProcessConfig::with_max_output_note_events` and `with_midi_output_limits` bound each family's attempts, including invalid or rejected attempts; defaults are zero. Attempted SysEx bytes are charged before target validation/dispatch and are not refunded on failure. There is no hidden retry or payload copy. With a positive budget, processing without a sink reports `NoSink` rather than dropping events.

Ordinary output notes/expression address output ports. Voice-end feedback instead addresses the original input port and note tuple. CLAP `NOTE_END` is plugin-to-host only and does not require a fabricated output port. Products must not end a matching voice that remains alive after a same-block retrigger.

For semantically essential output, silent drop is not an acceptable generic framework default. A product may propagate rejection through `ProcessError`; CLAP then returns processing failure without logging or endpoint completion. Already written audio, DSP history, and delivered events are not rolled back.

## Compatibility principle

Adapters lose semantics only when the target cannot represent them, and any degradation is explicit/capability-checked.

Valid examples may include semantic notes -> MIDI 1 when identity/expression loss is acceptable to the declared product capability. Invalid examples include silent block quantization of sample-accurate automation, discarding required note IDs, inventing cross-family order at equal offsets, or replacing source-defined ramps with an unrelated framework curve.

## Current implementation and remaining projection

CLAP requires one explicit `ClapNotePort` binding per schema event port, including one supported preferred dialect. Stable IDs are unique within a direction; direction-local host indices are derived by ID order and translated to schema-wide dense indices. Ports may support any combination of native notes, MIDI 1/SysEx, and MIDI 2/UMP; supported bits are advertised independently with no implicit conversion. Native-capable host indices must fit nonnegative `i16`; MIDI-only indices use `u16`. Unsupported dialects, unrepresentable names/IDs, and incomplete mappings fail setup.

The adapter validates the complete raw list before DSP, with independent total-input, parameter, note, MIDI, and aggregate input SysEx-byte bounds. Declared MIDI inputs require a positive common event bound; MIDI 1 additionally requires a positive SysEx-byte bound. Known records require exact sizes. Native addressing accepts only `-1` as a wildcard; note-on requires concrete port/channel/key, including at zero velocity. Off velocity is validated; choke's unused velocity is ignored. Unknown event spaces/types remain ignored; host-sent note-end, malformed records, incompatible port dialects, and unknown MIDI header-flag bits fail processing. Both defined MIDI annotations are preserved. Parameter flush remains a separate parameter-only control path.

Activation scratch stores MIDI metadata and SysEx host indices/sizes, never callback-borrowed payloads or host pointers. A callback-local source reacquires checked records and loans payloads only after byte/representability/null guards. Empty null payloads need no dereference. Valid live host storage, extent, initialization, and immutability remain CLAP provider obligations; nonnull pointers alone cannot establish them.

The exported [note probe](../../examples/clap-note-probe/src/lib.rs) is an output-only stereo instrument with two native input ports, both precisions, mixed-order-sensitive strike controls, all seven expressions, and essential voice-end feedback. It exercises a non-effect deployment without a new lifecycle or product synth framework.

The zero-audio [MIDI probe](../../examples/clap-midi-probe/src/lib.rs) routes three inputs to three outputs without interpreting bytes or words. Its primary pair supports MIDI 1 and MIDI 2, its secondary pair MIDI 1 only, and its third pair MIDI 2 only. Its timed Boolean enable control makes equal-offset parameter/MIDI ordering observable. ABI tests use a deep-copying output capture rather than retaining host payload pointers.

Remaining work includes per-note parameter modulation, product gestures, and span processing. Each needs its own deployment projection and qualification.
