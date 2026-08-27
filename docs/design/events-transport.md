# Events, Notes, MIDI, and Transport

Status: design direction; public API not frozen.

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
raw MIDI / SysEx
future MIDI 2 / UMP
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

Output events are emitted in the order required by the target backend; where the target requires sample-sorted insertion, Chassis enforces/validates it.

## Transport

Expose the best-known block-start transport snapshot, with availability explicit for every field:

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

Initial semantic event families should leave room for:

- note on/off;
- forced termination/choke where supported;
- note end/output notification where meaningful;
- velocity;
- tuning;
- per-note expression.

Do not freeze a least-common-denominator type until CLAP/VST3/AU/MIDI mappings are tested. Adapters expose only capabilities whose semantics they can preserve or whose degradation policy is explicit.

## Note expression

Leave room from the beginning for per-note pressure, pitch/tuning, timbre/brightness, volume, pan, and format-defined expression dimensions.

Normalize only when conversion semantics/ranges are specified. Keep an adapter extension path for source-specific data rather than silently mapping unlike ranges.

## Raw MIDI

Raw MIDI remains available for controller/program/vendor messages and utilities that need bytes rather than semantic notes.

MIDI data retains sample offset and event-port identity. SysEx payloads should be borrowed from host/process storage where possible; receiving them must not allocate on the callback.

MIDI 2/UMP remains on the roadmap. Do not down-convert information destructively merely to fit a MIDI-1-shaped core.

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

Host trajectory reconstruction and product smoothing remain separate. Chassis adds no product smoothing by default.

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

The processor never triggers unbounded allocation merely because a host delivered many events. Where the host API permits querying event count, validate/bound work against the current process call and implementation limits with an explicit failure path.

## Output events

Instruments/MIDI processors need a realtime-safe output sink that:

- preserves sample offsets;
- reports target rejection/capability limits;
- does not allocate/block;
- exposes only event classes that can be represented;
- keeps product ownership of retry/drop policy when a target cannot accept output.

For semantically essential output, silent drop is not an acceptable generic framework default.

## Compatibility principle

Adapters lose semantics only when the target cannot represent them, and any degradation is explicit/capability-checked.

Valid examples may include semantic notes -> MIDI 1 when identity/expression loss is acceptable to the declared product capability. Invalid examples include silent block quantization of sample-accurate automation, discarding required note IDs, inventing cross-family order at equal offsets, or replacing source-defined ramps with an unrelated framework curve.

## Initial implementation

The first FX conformance path needs only:

- parameter set/linear-trajectory cursors;
- block-start transport snapshot;
- deterministic span segmentation by sample boundary;
- no promise of one total cross-family event ordering.

Design stable event-port/note identities before public API stabilization; prove them with an instrument later.
