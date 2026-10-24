# Parameters, Automation, and State

Status: typed parameter schema/store, dense realtime identity, borrowed sample-accurate automation, durable `InstanceRuntime` ownership, complete semantic state replacement, CLAP float/integer/boolean/choice projection, and generation-checked complete CLAP state publication are implemented. Public API and the CHSS wire format remain pre-alpha.

## Identity

Persistent parameter identity is a stable `ParameterKey`. Rust names, declaration order, display labels, backend IDs, and runtime indices are not persistence identity.

Realtime processing uses a schema-local dense projection:

```text
ParameterKey (persistent) -> ParameterIndex (runtime only)
```

Adapters resolve backend IDs to dense indices during setup. `ParameterIndex` is never serialized.

Choice option identities are stable semantic IDs for control/persistence. Setup resolves them to parameter-local `ChoiceIndex` values for events; callbacks validate/index options without scanning strings. These indices are never serialized. CLAP uses stepped plain option indices, advertises enum metadata, and rejects labels that cannot round-trip unambiguously. Per [CLAP's stepped contract](https://github.com/free-audio/clap/blob/main/include/clap/ext/params.h), in-range host doubles truncate toward zero consistently in process/flush/display conversion. Typed semantic stores and decoded state remain strict; host quantization does not weaken them.

## Four parameter concepts

Keep these distinct:

1. **base state** — current persistent/control value;
2. **automation trajectory** — timed host/application changes for the block;
3. **modulation** — process-time contribution that does not overwrite base state;
4. **effective DSP value** — the result after combination/product policy.

Base values and automation are implemented. Modulation and its combination contract remain pending.

`ParameterEvents` and trajectory cursors borrow fixed-width event slices, not choice text or another persistent state store. The accepted event count therefore bounds shape-validation work independently of identifier length.

Explicit host ramps are reproduced without automatic extra smoothing. Smoothing remains product DSP policy unless real clients prove a common helper.

## Base-state authority

`InstanceRuntime<P>` owns the canonical format-independent `ParameterStore` for a retained core instance. Base values survive processor deactivate/reactivate cycles.

CLAP host lifetimes split long-lived shared/main-thread objects from the active audio processor. `chassis-clap` therefore keeps one adapter-local desired parameter/custom-state projection. Activation admits a coherent complete snapshot into a fresh `InstanceRuntime`; subsequent scalar synchronization is allowed only while its custom-resource generation matches. This projection exists because of deployment lifetime, not as permission for a second semantic authority.

Outside the pending custom-state transition described below, a successful process call retains each parameter's final automation point in `InstanceRuntime`'s durable store. Untouched parameters remain unchanged; equal-offset source order determines the last value. Validation failure or a DSP panic commits no endpoints. Direct clients therefore carry values into event-free blocks, state export, and reactivation without replaying events themselves. Subsequent control edits use exclusive runtime access and override that completion.

Linear points are block-local endpoints, not future ramp targets. A ramp spanning blocks must be clipped by its source into local points; provide the next block's offset-zero value when needed to preserve the original slope. No pending ramp or hidden extrapolation is retained.

Choice values in the store share schema-owned IDs, including values accepted from control edits or decoded state. Each stored choice also caches its parameter-local dense index at the sole mutation boundary. `get_event_value` and `set_event_value` project durable base values through fixed-width payloads without scanning stable identities; CLAP uses them for synchronization and completed publication, and flush keeps already-normalized dense values. Control/state lookup remains off-thread. Realtime choice replacement/drop is safe because an event never owns the last ID allocation reference. Serialization still writes stable choice IDs, never the cached index.

## Complete CLAP state publication

Realtime automation endpoint publication follows:

```text
synchronize scalars and the activation's custom epoch from completed generation G
 -> process the block and retain endpoints in the runtime
 -> project its completed values into preallocated scalar scratch
 -> attempt publication from G
 -> succeed only if G is still current
```

A newer control edit or state load advances the generation and causes stale realtime completion to be discarded. The audio thread never waits for control ownership.

The protocol uses even completed generations and odd in-progress writer tokens. Scalar values and a fixed-width custom epoch participate in the same transaction. Every accepted custom payload change assigns its completed generation as the new epoch, including reversion to earlier contents; scalar-only writes preserve it. Realtime writer acquisition is one-shot; realtime snapshots use a fixed retry bound. Non-realtime control/state operations may wait for an in-progress writer.

Custom entries stay behind a control-only ownership mutex. Complete load prepares and validates a candidate before taking that lock, publishes its scalars/epoch, then swaps custom ownership without a fallible step. Save and activation hold the same lock while capturing a completed scalar/epoch snapshot and copying custom entries. Encoding, stream I/O, host calls, and retired payload destruction occur after unlocking. The callback never takes this lock or adopts/reclaims custom payloads.

The adapter caches the generation its whole local store represents. Equal completed generations permit skipping an unchanged snapshot. The publication token comes from start-of-block synchronization, never from a new snapshot after DSP. Local completion invalidates that cache; a successful CAS establishes the new generation, while rejection leaves no token. End-of-block synchronization restores the whole newer scalar generation only if its custom epoch still matches this activation. Snapshot/CAS payload ordering remains Release/Acquire and is Loom-qualified. Active save reads the coherent desired projection, not partially completed local values.

An epoch mismatch is checked before mutating runtime parameters and latches a resource-transition gate until deactivation. Subsequent controls/loads change saveable desired state only; reset and custom reversion cannot reopen the gate. DSP keeps old resources and their parameter projection, with parameter automation withheld and no endpoint publication. A block already in flight when a load publishes may finish its old-generation DSP and retain local endpoints; that last old block establishes the frozen DSP baseline, but its stale CAS cannot overwrite desired state.

If a bounded snapshot cannot establish a matching baseline, withhold parameter trajectories and completion for that block too: a custom load may have completed immediately before another writer acquired ownership. Contention alone does not latch the gate. Original event domains, shape, and bounds are checked before withholding events. Parameter-only changes remain live before a custom transition, and processor-originated restart requests do not impose this gate.

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

Invalid decode, migration, framework values, unknown parameters, incomplete snapshots, or product validation leave live state and restart status unchanged. Typed insertion and decode enforce semantic-value validity before a document can be published.

`Component` supplies custom defaults, complete validation, and registered adjacent migrations. Construction validates descriptor/custom defaults together, even for identity-free components. Complete loads borrow that component, reject schema drift, and cannot substitute a caller-provided validator or migration list. Current-schema missing fields are errors, not requests to merge defaults or live state.

The first accepted active custom-state change freezes the pre-load DSP parameter projection and requests reactivation. While the host/environment defers it, DSP keeps old resources and frozen base values; all parameter events are validated but discarded, with no endpoint completion. Desired parameter/custom state remains saveable. Later loads/control edits update desired state only; reset or state reversion cannot unfreeze DSP. Parameter-only changes remain live before this gate, and processor-originated restart requests do not impose it.

Direct `parameters_mut()` edits and realtime automation validate descriptor domains, not arbitrary product cross-field invariants. Use complete state replacement for coupled control edits; activation checks complete component policy again. Processors must safely handle every admitted automation value with their current resources, requesting restart/retaining valid resources when necessary.

Migration/decoding are non-realtime operations. Retain fixtures for every released schema.

## Active state save

The complete CLAP save contract is locally executable:

- audio processing never waits for state save;
- the non-realtime save path obtains one coherent **completed** parameter/custom generation;
- a save racing a process block may linearize immediately before or immediately after the block's final endpoint publication;
- save cannot mix parameter values or independently sampled parameter/custom generations;
- accepted desired state remains saveable while old resources await reactivation;
- a newer control edit/state load defeats stale realtime publication;
- terminal generation exhaustion still permits coherent save of the final generation.

The [validation guide](validation.md) records production-helper, Loom, and actual CLAP-entry evidence for this contract.

A dated REAPER active-save/render baseline exists in the [validation guide](validation.md). Refresh it against changed exports; it is not automatic qualification of current head or other hosts.

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

- refresh native automated-render and active-save scenarios against changed exported artifacts;
- define/qualify product-originated gesture and echo semantics;
- add cross-format state/automation round trips once VST3/AU projections exist;
- freeze CHSS v1 only after released-product fixtures and cross-format evidence justify compatibility;
- do not generalize the adapter publication helper into core or add callback resource adoption until typed publication/reclamation has a concrete client need.

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
