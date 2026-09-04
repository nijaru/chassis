# Parameters, Automation, and State

Status: typed parameter schema/store, dense realtime identity, borrowed sample-accurate automation, durable `InstanceRuntime` ownership, complete semantic state replacement, CLAP float/integer/boolean/choice projection, and generation-checked scalar publication are implemented. Public API and the CHSS wire format remain pre-alpha.

## Identity

Persistent parameter identity is a stable `ParameterKey`. Rust names, declaration order, display labels, backend IDs, and runtime indices are not persistence identity.

Realtime processing uses a schema-local dense projection:

```text
ParameterKey (persistent) -> ParameterIndex (runtime only)
```

Adapters resolve backend IDs to dense indices during setup. `ParameterIndex` is never serialized.

Choice option identities are stable semantic IDs; CLAP projects choices onto dense plain-value option indices with option-name display round trips.

## Three parameter views

Processing keeps these distinct:

1. **base state** — current persistent/control value;
2. **automation trajectory** — timed host changes for the block;
3. **effective DSP value** — trajectory after modulation/product policy.

`ParameterEvents` and trajectory cursors are borrowed block views, not another persistent state store.

Explicit host ramps are reproduced without automatic extra smoothing. Smoothing remains product DSP policy unless real clients prove a common helper.

## Base-state authority

`InstanceRuntime<P>` owns the canonical format-independent `ParameterStore` for a retained core instance. Base values survive processor deactivate/reactivate cycles.

CLAP host lifetimes split long-lived shared/main-thread objects from the active audio processor. `chassis-clap` therefore keeps an adapter-local synchronized scalar projection and synchronizes it into the active `InstanceRuntime`. This projection exists because of deployment lifetime, not as permission for a second semantic authority.

## Automation publication

Realtime automation endpoint publication follows:

```text
observe completed generation G
 -> process the block
 -> derive final endpoint values
 -> attempt publication from G
 -> succeed only if G is still current
```

A newer control edit or state load advances the generation and causes stale realtime completion to be discarded. The audio thread never waits for control ownership.

The current scalar protocol uses even completed generations and odd in-progress writer tokens. Realtime writer acquisition is one-shot; realtime snapshots use a fixed retry bound. Non-realtime control/state operations may wait for an in-progress writer.

Completion publishes the new even generation before the pending notification. Pending consumption uses compare/exchange so a consumer that did not observe a notification cannot erase a concurrent one. This handoff is Loom-qualified.

`u64::MAX - 1` is the terminal completed generation. New writes fail rather than wrap into ABA ambiguity; coherent reads remain valid.

## Persistent state

Persistent state is a format-independent semantic document:

```text
State
├── Chassis envelope version
├── product schema version
├── parameter base values by stable key
└── typed custom product fields
```

Transient processor history is not persisted by convention.

Normal project/preset load is a complete transactional replacement:

```text
bytes
 -> bounded decode
 -> adjacent schema migrations
 -> complete current parameter/custom candidate
 -> framework validation
 -> product validation
 -> publish accepted state together
```

Invalid decode, migration, framework values, unknown parameters, incomplete snapshots, or product validation leave live state unchanged.

Migration/decoding are non-realtime operations. Retain fixtures for every released schema.

## Active state save

The CLAP scalar save contract is now locally executable:

- audio processing never waits for state save;
- the non-realtime save path obtains one coherent **completed** scalar generation;
- a save racing a process block may linearize immediately before or immediately after the block's final endpoint publication;
- save cannot accept a mixed cross-parameter generation;
- a newer control edit/state load defeats stale realtime publication;
- terminal generation exhaustion still permits coherent save of the final generation.

The publication tests cover:

- exhaustive abstract reader/writer interleavings rejecting mixed accepted snapshots;
- a control snapshot after realtime publication observing the completed new generation;
- a control snapshot encountering an in-progress multi-value write and returning only the completed coherent values;
- stale realtime generation rejection after a newer control write;
- bounded realtime failure while another writer is active;
- terminal generation behavior and value-count errors.

Native active-save qualification in a real host remains required before production support is claimed.

## Exported automation evidence

The format-independent conformance processor already exercises sample-accurate float set/linear trajectories.

The exported CLAP conformance component now makes its `trim` float parameter drive f32/f64 DSP sample by sample through the same trajectory cursor. This turns future validator/host automation runs into audible/differential semantic evidence instead of parameter-plumbing-only evidence.

The `mode` choice remains process-inert until a stepped-DSP conformance case is useful.

## Product-originated edits and modulation

Future editor/control edits use explicit begin/change/end gesture semantics, host notification ordering, and echo suppression without unrestricted mutable processor access.

Modulation remains separate from base state and must not overwrite persistent values.

Realtime-originated host notifications require a narrow bounded capability; product DSP cannot invoke arbitrary main-thread host APIs.

## Remaining freeze gates

Before freezing parameter/state APIs:

- run native automated-render and active-save scenarios against the updated exported CLAP artifact;
- define/qualify product-originated gesture and echo semantics;
- add cross-format state/automation round trips once VST3/AU projections exist;
- freeze CHSS v1 only after released-product fixtures and cross-format evidence justify compatibility;
- do not generalize the CLAP scalar publication helper into core until typed non-scalar publication/reclamation has a concrete client need.

## Authoring direction

```text
declare typed controls + stable identities + policy
        ↓
InstanceRuntime / deployment publication owns base-state semantics
        ↓
setup resolves identities into bounded dense realtime projections
        ↓
Processor consumes explicit trajectories/views
```

Macros/derives come only after real product code proves repeated mechanical declarations.
