# Parameters, Automation, and State

Status: typed parameter schema/store, borrowed process automation, durable core `InstanceRuntime` parameter ownership, complete runtime parameter-state replacement, and generation-checked CLAP scalar publication are implemented in source. The post-refactor Rust gate and current native CLAP qualification are still pending. Public API and persistence wire format are not frozen.

## Identity and schema

Persistent parameter identity is a stable human-readable `ParameterKey`. Rust names, declaration order, display labels, backend IDs, and future runtime indices are not persistence identity.

Supported semantic types are float, integer, boolean, and choice. Choice options also have stable identities.

Backend numeric IDs are format projections of canonical keys. Compatibility mappings must be deterministic/frozen before stable release.

The next process-path change is a dense schema-local identity:

```text
ParameterKey (persistent) -> ParameterIndex (runtime only)
```

`ParameterIndex` is never serialized. Setup resolves keys/backend IDs once; realtime event validation and trajectory matching should use the dense value instead of repeated string lookup.

## Plain values

Product semantics use meaningful plain values. Host-normalized values remain adapter representations.

Mappings such as linear, logarithmic, skewed, stepped, or custom monotonic mappings need explicit bounds/round-trip/property tests. Formatting/parsing/unit metadata should be reusable across host and editor surfaces.

## Base-state ownership

`InstanceRuntime<P>` now owns the durable `ParameterStore` for a directly retained core instance. Its base values survive processor deactivate/reactivate cycles because the processor is only an optional active child.

`Component::activate_with_parameters()` receives the current validated base values during processor preparation. The component definition itself stays outside the runtime so a deployment that transfers the active processor does not accidentally require `Component: Send`.

The older `Activated<P>` path remains temporarily for compatibility and is activation-local. New core clients and the CLAP audio path use `InstanceRuntime`.

A plugin host can impose lifetimes that split durable control state from the active audio object. CLAP does this: the audio-processor object exists only while active. The current adapter therefore keeps a synchronized scalar publication bridge across CLAP domains, synchronizes it into a fresh `InstanceRuntime` before activation, and keeps the runtime as the semantic audio-domain projection while active.

This is an explicit deployment projection, not permission for unrelated mutable authorities. Cross-domain updates require one defined publication order and precedence rule.

## Process-time views

Processing distinguishes:

1. base state — current persistent/control value;
2. automated trajectory — host automation over the block;
3. effective value — trajectory after modulation/product semantics.

`ParameterEvents` and trajectory cursors are derived block views, not persistent state.

Current events still carry stable string keys. Dense `ParameterIndex` conversion is the next API/performance step. Start with dense IDs only; add more elaborate per-parameter event indexing only if measurements justify it.

## Automation publication

Automation endpoint publication follows this causality rule:

```text
observe generation G
 -> process block automation
 -> derive final base endpoint
 -> publish only if generation is still G
```

If a newer control edit/state replacement has already published, stale realtime completion is discarded. The audio thread never waits for control ownership.

The CLAP scalar bridge implements writer serialization, coherent snapshots, and exact-generation stale-write rejection locally. Before extracting a generic core synchronization primitive, model-test its ordering/progress behavior and design typed-value/reclamation semantics rather than standardizing CLAP's scalar `f64` representation.

Modulation remains separate and must not overwrite base state.

## Smoothing

Smoothing is product DSP policy. Chassis should reproduce host trajectories without extra smoothing by default. Optional helpers may be added when real clients prove reusable behavior, but explicit host ramps must not be silently double-smoothed.

## Product-originated edits

Future editor/control changes use a gesture path equivalent to begin/set/end edit. Framework code validates base-state changes and host notification ordering without exposing unrestricted mutable `Processor` access.

Realtime-originated host notifications require an explicit bounded capability; DSP cannot invoke arbitrary main-thread APIs.

## Persistent state

Persistent state is a format-independent semantic document:

```text
State
├── Chassis envelope version
├── product schema version
├── parameter base values by stable key
└── typed/custom product persistent fields
```

Transient DSP history is not persisted by convention.

State encoding is deterministic, bounded, portable, explicitly versioned, and independent of Rust layout. `state-format.md` owns the current wire-format prototype.

## Complete transactional load

Normal plugin/project/preset state is a complete replacement, not an implicit patch.

`InstanceRuntime::apply_parameter_state_for_product()` currently:

- builds a temporary candidate from schema defaults;
- validates product identity/schema and every parameter value;
- rejects unknown parameters;
- requires every current framework-managed parameter to be represented;
- swaps the candidate only after validation succeeds.

Failure leaves current runtime state unchanged.

The lower-level `ParameterStore::apply_state_for_product()` remains patch-like and is a lower-level semantic helper, not the final persistence boundary. If partial patches become useful, expose them as a separately named operation.

The migration-aware load path is:

```text
bytes
 -> bounded decode
 -> migrate product schema
 -> materialize complete current state
 -> validate parameter/custom domains
 -> publish one accepted generation
```

Migration/decoding never run on the audio thread. Adjacent schema migrations should retain fixtures from every released version.

## State save while active

Each adapter must define state-save timing relative to processing. Required behavior:

- the audio thread never blocks;
- arbitrary live processor internals are not serialized;
- one completed semantic generation is saved, not a mixed snapshot;
- save/load races have deterministic precedence;
- native tests cover active automation/save where supported.

The current CLAP scalar bridge lets the non-realtime save path wait for a coherent completed scalar generation while realtime paths remain bounded/nonblocking. This remains provisional adapter evidence.

## Testing gates

Current source/test coverage includes:

- bounded/sorted automation event shape and domain validation;
- sample-accurate set/linear cursor behavior;
- durable core base state across deactivate/reactivate;
- activation observing current base state;
- owned dynamic activation I/O;
- complete transactional runtime parameter replacement;
- deterministic/bounded CHSS behavior and empty-key rejection;
- CLAP stale-generation rejection and complete scalar state replacement.

Still required before freezing this API:

- local post-refactor fmt/test/Clippy/deny/machete/release-build gate;
- dense `ParameterIndex` conversion and mapping tests;
- automation/control/state race tests;
- model testing for any generalized atomic publication primitive;
- migration fixtures and corruption/exhaustion fuzzing;
- gesture/echo semantics;
- native active state-save tests;
- cross-format state round trips.

## Authoring direction

The eventual ergonomic flow is:

```text
declare typed controls + stable identities + policy
        ↓
InstanceRuntime / deployment publication owns base-state semantics
        ↓
setup resolves stable identities into bounded runtime projections
        ↓
Processor consumes explicit realtime trajectories/views
```

Macros/derives come only after these explicit contracts are proven.
