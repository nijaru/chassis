# Parameters, Automation, and State

Status: schema/store and borrowed process-automation prototype implemented; public API and persistence wire format are not frozen.

## Goal

Most products should declare controls and persistent fields rather than implement host plumbing. Chassis must preserve sample-accurate automation/modulation while keeping one clear authority for persistent/base state.

## Immutable schema

A component's parameter schema is immutable for the lifetime of an instantiated component unless a future explicit dynamic-parameter capability is added.

Each parameter has a stable human-readable key such as:

```text
input.gain
compressor.threshold
band.low.frequency
output.ceiling
```

Rust field names, declaration order, display names, and backend IDs are not persistent identity.

The conventional types are float, integer, boolean, and enum/choice. Enum variants also need stable identities; reordering Rust variants must not reinterpret presets.

Backend numeric IDs are deterministic derived projections of canonical keys. Their mapping algorithm and collision behavior must be frozen and covered by golden fixtures before a stable adapter release. Explicit legacy/backend overrides remain an escape hatch.

## Plain values

Product semantics use meaningful plain values. Host-normalized values are adapter representations.

Common mappings may include linear, logarithmic/exponential, skew/power, stepped/discrete, and explicit custom monotonic mappings. Mapping code must have property tests for bounds, monotonicity, finite values, and useful round trips.

Formatting/parsing/unit metadata is shared by host UI, generic editors, and product bindings where target APIs support it.

## One base-state authority

Chassis owns one framework `ParameterStore`-like authority per instance for the **current base/control values**.

Product `MainThread`, editor bindings, state serialization, and `Processor` do not maintain independently mutable copies of those values.

The current pre-alpha storage is an owned `ParameterStore`. The final storage/synchronization primitive is deliberately not frozen yet. It must satisfy:

- non-blocking delivery of host automation/control changes relevant to realtime processing;
- non-RT observation/editing without exposing mutable `Processor` state;
- typed validation at the authority boundary;
- a defined way to publish multi-parameter state loads as one generation/transaction from the processor's point of view;
- a defined state-save consistency model while processing/automation may be active.

Do **not** implement this as unrelated atomics and then claim state serialization is an atomic snapshot. If state save needs a coherent cross-parameter snapshot, provide and test an epoch/snapshot protocol or another explicit mechanism. If the product contract permits weaker consistency, document that precisely instead of implying transactional behavior.

## Process-time values are derived views

Processing distinguishes:

1. **base state** — current persistent/control value in the framework authority;
2. **automated trajectory** — block/sample-time values implied by host automation;
3. **effective value** — trajectory after applicable modulation/product control semantics.

The processor currently receives a borrowed `ParameterEvents` view and can create lazy floating-point set/linear cursors derived from the authority plus normalized events. Those cursors are not a second persistent authority.

The framework must define how a host automation event updates the current base value exposed to control/state APIs while preserving the exact process-time event ordering. This behavior must be verified per format before the parameter API is frozen.

## Automation and modulation

Base automation and modulation are separate semantics. Modulation must not overwrite the base value.

Chassis preserves source trajectories:

- an instantaneous change is a timed set;
- VST3 point queues produce their specified piecewise-linear trajectory;
- Audio Unit ramps remain linear spans with duration/end value;
- CLAP core value events remain timestamped sets unless another supported extension provides richer semantics.

Reconstructing the source trajectory is adapter/framework correctness, not parameter smoothing.

Per-note/key/channel/port modulation must remain possible without redesigning canonical parameter identity, even if initial FX support is global only.

## Smoothing

Smoothing is common enough for optional helpers but is product DSP behavior.

Default: reproduce host control trajectory with **no extra Chassis smoothing** unless the product declares a smoothing policy.

Useful helpers may include linear-time and exponential slew, but their interaction with explicit host ramps must be visible. Avoid silently double-smoothing a trajectory the host already specified.

Products can consume raw trajectories/events and implement detector/control-rate behavior themselves.

## Product-originated edits

Editor/control changes use one standard gesture path conceptually equivalent to:

```text
begin_edit(param)
set_value(param, value)
end_edit(param)
```

The framework validates and updates the base-state authority, notifies observers, and informs the host without feedback loops.

Realtime-originated host notifications require a separately modeled bounded/capability-checked path. DSP cannot call arbitrary main-thread host APIs.

## Persistent state

Persistent state is a Chassis/product semantic document shared across CLAP, VST3, AU, standalone, and embedded deployment.

Logical contents:

```text
State
├── Chassis envelope version
├── product schema version
├── typed parameter base values by canonical key
└── typed/custom product persistent fields
```

Runtime DSP history—delay lines, detector envelopes, oscillator phase, lookahead buffers, transient caches—is not persisted by convention.

State encoding must be deterministic, bounded/defensive, portable, explicitly versioned, independent of Rust memory layout, and suitable for migrations. The separate `state-format.md` owns the wire-format prototype.

## Transactional load

State bytes are untrusted input.

Load path:

```text
bytes
  -> bounded decode into temporary document
  -> migrate semantic schema
  -> validate current parameter/custom domains
  -> build accepted state generation
  -> publish canonical state
  -> make Processor observe that generation at a defined safe boundary
```

Failure before publication leaves current state unchanged. Migration and decoding never run on the audio thread.

The runtime must specify what happens if state replacement races with product edits, automation, or an older asynchronous preparation. Generation/ownership rules decide which result may publish; stale work cannot silently overwrite newer authority.

## State save while active

Before stable release, each adapter must document when save can occur relative to processing and Chassis must provide a matching snapshot contract.

Required properties:

- no blocking of the audio thread;
- no arbitrary access to live `Processor` internals;
- no undefined mixture presented as a coherent transactional snapshot;
- stable handling of a save racing a state load/replacement;
- tests that exercise active automation/save where the host format permits it.

## Migrations

Product schema versions are monotonic migration versions, separate from marketing/package versions.

Adjacent sequential migrations are the conventional path:

```text
v1 -> v2 -> v3 -> current
```

Keep fixtures from every public schema. Loading a newer unknown schema fails safely by default unless a product deliberately defines a proven forward-compatibility policy.

## Presets

A preset is named product state plus metadata. Chassis may provide common storage/serialization helpers later; browsers, tags, cloud sync, and product-specific UX remain outside core.

## Testing requirements

The current core conformance path covers bounded borrowed event validation,
nondecreasing source order, sample-accurate set/linear cursor evaluation,
parameter-domain rejection before product DSP, and optional block-start
transport values. It does not yet define host gesture translation,
automation-to-base publication, or active state-save consistency.

`chassis-test` should eventually cover:

- canonical/derived ID uniqueness and frozen mapping fixtures;
- mapping bounds/monotonicity/round trips;
- formatting/parsing where applicable;
- step and linear automation trajectories across block boundaries;
- modulation not mutating base state;
- base-state publication after automation according to each adapter contract;
- gesture ordering and host echo suppression;
- coherent/defined active state-save semantics;
- transactional state load and generation replacement;
- deterministic state bytes and migration fixtures;
- corrupt/truncated/oversized state rejection;
- cross-format state round trips.

## Ergonomics

The eventual authoring API may use derives/macros, but only after the explicit semantic API works through the conformance component and first adapter.

The normal flow should be:

```text
declare typed controls + stable keys + range/default/display/smoothing policy
        ↓
Chassis provides host metadata, base-state authority, state, gestures, and automation
        ↓
Processor consumes explicit realtime trajectories/views
```

Convention removes plumbing; it does not hide ownership, timing, or compatibility.
