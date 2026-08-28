# Parameters, Automation, and State

Status: schema/store and borrowed process-automation prototype implemented; the CLAP scalar bridge now has generation-checked publication and complete state replacement, while the durable `InstanceRuntime` authority remains to implement. Public API and persistence wire format are not frozen.

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

A separate dense runtime index is also appropriate once the process API graduates beyond the semantic prototype:

```text
stable ParameterKey -> validated schema-local ParameterIndex -> realtime event/process view
```

The dense index is never persistent identity. It exists to remove repeated string/schema lookup from bounded realtime event handling and cursor evaluation.

## Plain values

Product semantics use meaningful plain values. Host-normalized values are adapter representations.

Common mappings may include linear, logarithmic/exponential, skew/power, stepped/discrete, and explicit custom monotonic mappings. Mapping code must have property tests for bounds, monotonicity, finite values, and useful round trips.

Formatting/parsing/unit metadata is shared by host UI, generic editors, and product bindings where target APIs support it.

## One base-state authority

Chassis owns one framework `ParameterStore`-like authority per instance for the **current base/control values**.

Product `MainThread`, editor bindings, state serialization, and `Processor` do not maintain independently mutable semantic copies of those values.

The current core `Activated` shell owns an activation-local `ParameterStore`; because that store is recreated/destroyed with activation, it is not sufficient as the durable instance authority. The current CLAP scalar adapter therefore has a provisional synchronized parameter projection that preserves host-visible values across activation. That bridge is useful implementation evidence, but the next `InstanceRuntime` slice must make the durable framework authority explicit and reduce adapter/shared state to a projection of it.

The final storage/synchronization primitive is deliberately not frozen yet. It must satisfy:

- non-blocking delivery of host automation/control changes relevant to realtime processing;
- non-RT observation/editing without exposing mutable `Processor` state;
- typed validation at the authority boundary;
- multi-parameter state loads published as one generation/transaction from the processor's point of view;
- coherent state-save semantics while processing/automation may be active;
- generation ordering that rejects stale completion rather than silently overwriting newer state.

Do **not** implement this as unrelated atomics and then claim state serialization is an atomic snapshot. A cross-parameter snapshot needs an explicit writer/snapshot protocol. Any subtle atomic primitive promoted into shared core infrastructure requires model/property testing of its ordering/progress behavior before adoption.

## Process-time values are derived views

Processing distinguishes:

1. **base state** — current persistent/control value in the framework authority;
2. **automated trajectory** — block/sample-time values implied by host automation;
3. **effective value** — trajectory after applicable modulation/product control semantics.

The processor currently receives a borrowed `ParameterEvents` view and can create lazy floating-point set/linear cursors derived from the active base projection plus normalized events. Those cursors are not a second persistent authority.

The current event prototype names parameters by stable string key to make semantic tests explicit. Before the process API freezes or high-count automation is promoted, adapters/runtime setup should resolve those keys to dense schema-local indices so the audio path does not repeatedly compare strings or rescan schema mappings.

## Automation-to-base publication

Host automation affects both the current process trajectory and, after the relevant event endpoint, the persistent/base value observed by later blocks and state/control APIs. Publication back to base state must preserve causality.

The generation rule is:

```text
observe base generation G
  -> process host automation for the block
  -> derive final base endpoint
  -> publish endpoint only if canonical generation is still G
```

If a control edit or state replacement publishes generation `G+1` before the realtime endpoint is committed, the stale realtime publication is discarded. Newer canonical state wins; the audio thread never waits to reclaim authority.

The current CLAP scalar bridge implements that rule locally. Its writer token serializes control/state publications so a reader cannot accept a mixed cross-parameter snapshot, and realtime full-snapshot publication uses compare/exchange against the exact generation it observed. This behavior still requires native host qualification and model testing before the mechanism itself is generalized into core.

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

A normal plugin/project/preset state load is a **complete replacement**, not an implicit patch against whatever values happened to be live previously. After migration to the current schema, the accepted state must materialize every framework-managed parameter and required persistent field. A newly introduced field may obtain a value from an explicit migration/default rule; otherwise missing required state is rejected. If partial parameter patches are useful later, model them as a distinct operation with distinct semantics.

The current CLAP scalar state projection enforces complete parameter snapshots. The older core `ParameterStore::apply_state_for_product` prototype still has patch-like internal mechanics; replace that path as part of the `InstanceRuntime` authority transition rather than treating it as the final persistence contract.

## Transactional load

State bytes are untrusted input.

Load path:

```text
bytes
  -> bounded decode into temporary document
  -> migrate semantic schema
  -> materialize complete current semantic state
  -> validate current parameter/custom domains
  -> build accepted state generation
  -> publish canonical state
  -> make Processor observe that generation at a defined safe boundary
```

Failure before publication leaves current state unchanged. Migration and decoding never run on the audio thread.

The runtime must specify what happens if state replacement races with product edits, automation, or an older asynchronous preparation. Generation/ownership rules decide which result may publish; stale work cannot silently overwrite newer authority.

## State save while active

Each adapter must document when save can occur relative to processing and Chassis must provide a matching snapshot contract.

Required properties:

- no blocking of the audio thread;
- no arbitrary access to live `Processor` internals;
- one completed canonical generation is serialized, never an undefined cross-generation mixture;
- stable handling of a save racing a state load/replacement;
- tests that exercise active automation/save where the host format permits it.

The current CLAP scalar bridge lets the non-realtime save path wait for a coherent completed parameter generation while realtime readers/writers remain bounded/nonblocking. That is provisional adapter evidence, not yet the final framework API.

## Migrations

Product schema versions are monotonic migration versions, separate from marketing/package versions.

Adjacent sequential migrations are the conventional path:

```text
v1 -> v2 -> v3 -> current
```

Keep fixtures from every public schema. Loading a newer unknown schema fails safely by default unless a product deliberately defines a proven forward-compatibility policy.

A migration is also where a newly required persistent field obtains an explicit historical default or transformation. The post-migration current document is complete before publication.

## Presets

A preset is named product state plus metadata. Chassis may provide common storage/serialization helpers later; browsers, tags, cloud sync, and product-specific UX remain outside core.

## Testing requirements

The current core conformance path covers bounded borrowed event validation,
nondecreasing source order, sample-accurate set/linear cursor evaluation,
parameter-domain rejection before product DSP, optional block-start transport
values, and deterministic/bounded state codec behavior. The CLAP scalar bridge
also has adapter-level tests for stale-generation rejection and complete
parameter-state replacement.

Still required before the parameter/state API can freeze:

- canonical/derived ID uniqueness and frozen mapping fixtures;
- dense runtime-index mapping invariants;
- mapping bounds/monotonicity/round trips;
- formatting/parsing where applicable;
- step and linear automation trajectories across block boundaries;
- modulation not mutating base state;
- base-state publication after automation according to each adapter contract;
- control/state/automation generation-race tests;
- model testing for any shared atomic publication primitive;
- gesture ordering and host echo suppression;
- coherent active state-save semantics in native host tests;
- transactional complete state load and generation replacement;
- deterministic state bytes and migration fixtures;
- corrupt/truncated/oversized state rejection;
- cross-format state round trips.

## Ergonomics

The eventual authoring API may use derives/macros, but only after the explicit semantic API works through the conformance component and first adapter.

The normal flow should be:

```text
declare typed controls + stable keys + range/default/display/smoothing policy
        ↓
InstanceRuntime owns canonical base state and stable schema
        ↓
adapter/runtime projects bounded dense realtime views + host semantics
        ↓
Processor consumes explicit realtime trajectories/views
```

Convention removes plumbing; it does not hide ownership, timing, or compatibility.
