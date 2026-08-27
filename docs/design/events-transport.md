# Events, Notes, MIDI, and Transport

Status: design direction; public API not frozen.

## Goal

Provide one coherent sample-accurate event model that works for effects and instruments without forcing every product to parse raw MIDI or every effect to care about note semantics.

## Timeline model

Process-time changes should be representable as events with a sample offset relative to the current block.

The core abstraction is conceptually:

```text
TimedEvent
├── sample_offset
└── Event
    ├── parameter set / automation trajectory
    ├── parameter modulation
    ├── note on/off/choke/end
    ├── note expression
    ├── MIDI 1
    ├── SysEx
    ├── MIDI 2 / UMP (later or capability-gated)
    ├── transport update
    └── future typed extension events
```

Input events are presented in nondecreasing sample order. Output events emitted by a component must also preserve sample order.

A single ordered timeline preserves relationships that separate parameter/note/MIDI iterators can otherwise obscure. Convenience filtered iterators/views can still be provided for products that only care about one event family.

## Block-start transport

The process context should expose the best-known transport state at sample zero of the block for the common case:

- playing/recording/pre-roll state;
- tempo;
- song position in samples/seconds/beats where available;
- time signature;
- bar start/number where available;
- loop range/state.

A timestamped transport event later in the block updates that state from its sample offset onward. Products that do not need sample-accurate transport changes can read only the block-start snapshot.

Do not pretend a field is present when the host did not provide it. Transport fields should represent availability explicitly.

## Event ports

Audio ports and event/note ports are separate concepts.

A component may have:

- audio only;
- event/note input plus audio output (instrument);
- event input and output with no audio (MIDI/note processor);
- multiple event ports where target formats support them.

Event ports need stable identity and direction similarly to audio ports, but their capabilities may differ by format.

## Semantic note events

Chassis should expose semantic note events instead of forcing instruments to decode MIDI bytes for ordinary note handling.

A note address should preserve enough identity for modern hosts:

```text
NoteAddress
├── event port
├── optional note id
├── optional channel
└── optional key
```

For a note-on, port/channel/key requirements can be stricter than for targeting/choke/expression events. Wildcard semantics should be modeled explicitly rather than overloading arbitrary sentinel integers in product code.

Note events should include at least:

- note on;
- note off;
- choke/forced termination where the source format distinguishes it;
- note end/output notification where useful;
- velocity;
- tuning where the source format provides it.

The exact cross-format subset and fallback rules need validation before the type is frozen.

## Note expression

Modern instrument support requires room for per-note expression from the beginning even if the first FX clients ignore it.

Common semantic expressions include pressure, tuning/pitch, timbre/brightness, volume, pan, and other host-defined or format-specific expression dimensions.

Chassis should normalize common semantic expressions where mappings are well defined while retaining an escape hatch for format-specific data. Do not silently reinterpret one format's note-expression range as another's without a specified conversion.

## Raw MIDI

Raw MIDI remains first-class because many products need messages with no higher-level Chassis interpretation:

- CC/program/controller messages;
- MIDI utilities;
- SysEx;
- vendor-specific data;
- legacy host compatibility.

MIDI 1 messages should retain their event-port/cable identity and sample offset.

SysEx data is borrowed from the process event buffer where possible; processing must not allocate merely to receive a SysEx event.

## MIDI 2

MIDI 2/UMP belongs on the roadmap and the event model must leave room for it. Do not reduce incoming MIDI 2 to MIDI 1 if that loses information.

Initial adapters may advertise only the event dialects that are actually implemented and validated.

## Parameters in the event timeline

Timestamped parameter base-value changes, automation trajectories, and modulation events participate in the same process timeline.

The framework may additionally construct efficient parameter cursors/views so DSP does not search a heterogeneous event stream for every sample. That optimization must preserve the event ordering semantics.

Parameter gesture begin/end is primarily a control/host communication concern. If a target format delivers gestures through the realtime event path, adapters can preserve them without requiring ordinary DSP to consume them.

## Automation trajectories and ramps

Chassis preserves the automation semantics supplied by the source format instead of flattening every change into a step event.

The semantic parameter trajectory needs at least two operations:

```text
Set
  at sample offset -> value changes immediately

LinearRamp
  start sample/value -> end sample/value
```

The representation may ultimately be exposed primarily through a parameter cursor/span API rather than as a literal public event enum, but the distinction is part of the framework contract.

Adapter behavior:

- **VST3:** an `IParamValueQueue` is specified as a piecewise-linear approximation of the host automation curve. Consecutive points, including the implicit previous value at the start of a block, therefore become linear trajectory segments. Jumps remain representable by adjacent points as defined by VST3.
- **Audio Unit:** a normal parameter event becomes `Set`; an explicit parameter-ramp event becomes a `LinearRamp` with the supplied duration and end value. A ramp may need continuation state if its duration crosses a process-block boundary.
- **CLAP:** `CLAP_EVENT_PARAM_VALUE` is a timestamped value change and has no core ramp-duration primitive, so it becomes `Set` unless a future CLAP extension/source semantic explicitly conveys a trajectory.

Adapters must not invent interpolation merely because another format uses it, and they must not discard an explicit ramp supplied by a format that has one.

Never expand a ramp into one heap-allocated event per sample. Parameter cursors/trajectories compute values or spans lazily using bounded process-time state.

### Interaction with product smoothing

Host automation trajectory reconstruction and product smoothing are separate layers.

Chassis should first reproduce the source automation trajectory correctly. Optional parameter smoothing is then a product-declared behavior applied to the control signal according to the chosen smoothing policy.

The default is **no product smoothing unless the parameter declares or the DSP implements it**. This avoids silently altering host automation.

A conventional smoothing helper may offer policies such as applying only to discontinuous changes or applying to every target change, but those semantics must be explicit before the API is stabilized. Products with custom control-rate behavior can consume the unsmoothed trajectory directly.

## Event storage

Process events must be borrowed or backed by bounded/preallocated storage. The audio callback cannot allocate in proportion to host event count.

Adapters should provide an iterator/view over host events directly when practical. If normalization requires temporary storage or merging multiple backend event sources, allocate bounded scratch during activation and define overflow behavior.

A host delivering more events than the configured bounded representation can hold must cause an explicit error/drop policy; silent memory growth on the realtime thread is not acceptable.

## Output events

Instruments and MIDI processors need a realtime-safe way to emit note/MIDI/parameter events to the host.

The output sink should:

- preserve sample offsets;
- report capacity/rejection explicitly;
- avoid allocation/blocking;
- expose only event classes the active adapter can represent;
- allow products to react when a host rejects or cannot represent an event.

## Process segmentation helpers

Many processors want to process contiguous audio ranges between event/trajectory boundaries. Chassis should provide or strongly consider a helper that yields spans conceptually like:

```text
frames 0..17 with current parameter trajectory
apply boundary/events at 17
frames 17..64
apply boundary/events at 64
...
```

This is especially useful for sample-accurate instruments and automation without forcing a branch over the full event list for every sample.

A ramp need not create a boundary at every sample; the span exposes the trajectory needed to evaluate or vectorize it.

Such helpers should be optional; block-based DSP can consume parameter trajectories or events differently.

## Compatibility principle

Adapters may lose capabilities only when the target format/host cannot represent them. Any degradation should be explicit and testable.

Examples:

- converting semantic note events to MIDI 1 where that is the only host path can be valid if the semantics survive;
- discarding note IDs or per-note modulation silently is not valid when the product declares that capability as required;
- block-quantizing sample-accurate automation without disclosure is not valid;
- replacing a source-defined linear automation segment with an arbitrary product/framework ramp is not valid.

## Initial implementation

The first conformance effect needs only:

- parameter set and linear-trajectory semantics;
- process/block-start transport;
- the ordered event abstraction;
- bounded output plumbing sufficient for host communication tests.

Note/MIDI types should be designed before stable publication, then proven by an instrument client later.
