# Open Questions and API Freeze Gates

This file records unresolved design decisions that still block API or production claims. Completed proof steps belong in `docs/design/validation.md`; execution order belongs in `docs/next-runtime-slice.md`.

## Runtime/API freeze

`InstanceRuntime<P>` is the sole durable format-independent authority for one component instance. `Processor` owns active DSP history; `Process<S>` expresses sample-representation capability. The temporary activation-local `Activated<P>` / free `runtime::activate()` lifecycle path has been removed.

Still unresolved before freezing the authoring surface:

- decide final ergonomics for constructing `InstanceRuntime` from a `Component` without hiding schema validation or ownership;
- prove semantic whole-I/O policy beyond structural port presence for products that negotiate multiple legal layouts;
- keep `Processor` non-`Send` in core unless a deployment requires transfer; adapters add the bound at their boundary;
- let real clients determine whether common activation/resource helpers are worth promoting.

Do not add proc macros/derives until real client code demonstrates the repeated declarations worth hiding.

## Process buffers

The generic safe source shape is implemented and the CLAP adapter traverses setup-mapped channels without callback-owned channel collections.

Freeze gates:

- ergonomic higher-level bus/port views only after real DSP clients demonstrate recurring access patterns;
- target-specific legality for inactive/null/zero-buffer cases as formats require;
- benchmark controlled copy/conversion paths before adding ownership or unsafe complexity to remove them;
- native f64 host-dispatch evidence remains desirable, but lack of a host choosing `data64` does not invalidate validator-qualified f64 support.

## Parameters and automation

Implemented: typed schema/store, dense realtime indices, borrowed sample-accurate events, CLAP float/integer/boolean/choice projection, generation-checked scalar publication, state value rescan after load, and exported `trim` DSP that follows sample-accurate automation for f32/f64 processing.

Freeze gates:

- finalize product-originated begin/change/end gesture semantics and echo suppression;
- modulation must remain distinct from base-state publication and must not overwrite durable values;
- parameter mapping/formatting helpers must have explicit round-trip/domain tests before becoming generic authoring conveniences;
- real-host automated-render evidence for current head remains required even though core/export tests already prove the trajectory locally.

## Active state save/load

The CLAP consistency contract is implemented locally:

- state save is a non-realtime operation and may wait for an in-progress scalar publisher;
- it serializes one coherent completed publication generation;
- a save racing a process block may observe the generation immediately before or after that block's endpoint publication, never a mixed generation;
- a newer control edit/state load wins over stale realtime publication by generation check;
- state load remains transactional and requests host value rescan after successful publication.

Direct executable publication tests cover completed snapshots, an in-progress multi-value write, stale realtime generation rejection, bounded realtime attempts, and terminal generation behavior. Before freezing production state claims, add a reproducible active-save-during-automation host scenario and cross-format round trips once VST3/AU exist.

The CHSS wire envelope remains pre-v1 and may change until these gates are complete.

## Latency / lookahead

Implemented:

- `LatencySamples` in core;
- `Processor::latency()` with zero default;
- activation-scoped latency snapshot in `InstanceRuntime`;
- lifecycle tests proving the value cannot drift within one activation and is recomputed on reactivation;
- CLAP `PluginLatency` projection plus host latency-change notification when a new activation changes the value.

Still open:

- qualify nonzero latency with DSP that actually delays output by the reported amount;
- verify host PDC/alignment behavior and restart/reactivation when a product configuration changes required latency;
- determine any higher-level lookahead/resource authoring conveniences from the mastering limiter rather than inventing them in advance;
- prove cross-format latency parity once VST3/AU projections exist.

The current conformance processor reports zero latency. Do not advertise metadata-only nonzero latency as proof.

## Realtime guarantees

Core now has a thread-local allocation/deallocation detector around repeated post-activation processing. It is evidence for that exact core callback path, not for the complete adapter or production workload.

Before promotion:

- instrument the full CLAP adapter process path under representative mapped topologies;
- stress configured maximum parameter-event counts and frame-count extremes;
- measure adapter-only overhead at representative block sizes;
- verify replaced-object reclamation cannot fall onto the callback when larger snapshot/telemetry facilities appear.

Do not introduce custom allocators, lock-free infrastructure, SIMD, padding, or unsafe zero-copy machinery merely to make benchmarks look sophisticated.

## CLAP production qualification

The last recorded native baseline passed the available validator + REAPER macOS-arm64 matrix, but it predates current-head audible automation and latency-extension changes. Current implementation therefore needs a fresh native qualification run before those baseline results become current evidence again.

Open:

- current-head `clap-validator` + bounded fuzz rerun;
- current-head REAPER scan/instantiate/state reopen;
- actual automated `trim` render differential against expected sample offsets;
- active state save while automation is running;
- nonzero PDC behavior once real delayed DSP exists;
- reentrant host callbacks under the pinned Clack model;
- repeated host lifecycle/sample-rate/block-size transitions;
- additional OS/architecture/host coverage;
- Bitwig coverage when available;
- native host f64 dispatch when a host can be induced to choose it.

Core activation-failure recovery is already covered and is no longer an open core gate.

Clack remains pinned to reviewed revision `c5975f9f89f0953b00768680357985d46178078a`. Any revision/release change is an audit and conformance event.

## Export metadata/API

Current CLAP export identity constants are temporary proof metadata. Before shipping a product:

- define one canonical product/export identity manifest;
- generate backend IDs/metadata from that owner rather than duplicating constants across adapters;
- support explicit import/legacy mappings where released compatibility requires them;
- freeze byte-order/identity fixtures.

Temporary names that encode obsolete proof limits should be renamed before real clients depend on them. `ClapStereoEffect` remains such a candidate if the first production clients make its name misleading relative to the generic mapped I/O it already supports.

## VST3/AU projection

Treat `clap-wrapper` output as a separate adapter qualification problem, not proof by construction.

Before claiming VST3/AU support, run the same product semantics through:

- native validators;
- state/identity fixtures;
- automation trajectory tests;
- buffer/layout tests;
- latency/PDC parity;
- editor lifetime/resize tests;
- real host save/reopen/automation scenarios, including Ableton where wrapper state restoration is host-sensitive;
- differential comparison against native CLAP semantics.

Own a native format adapter only when concrete wrapper limitations justify it.

## GUI

Do not make a toolkit part of the core contract prematurely.

Before a first-class GUI adapter:

- parent window attach/detach;
- resize/scale/high-DPI behavior;
- editor recreation/teardown;
- parameter observations and gestures;
- realtime telemetry ownership;
- accessibility path where practical;
- enough rendering/control flexibility for commercial product UIs.

## Background work / snapshots

No hidden global executor.

Before adding framework task services, specify instance/module lifetime, cancellation, stale-completion rejection, unload/shutdown behavior, result destruction ownership, and offline determinism.

Typed immutable control->DSP snapshots or meter publication may be implemented independently when the first real FX client proves a concrete need.

## Instruments / events

The core remains compatible with instruments, but note/MIDI helpers are deferred until a real instrument/event client exists.

Before stable instrument claims, prove note identity/expression, event output capacity/rejection, MIDI fallback where relevant, multiple outputs, high-event-count behavior, and standalone deployment.

## Surround / immersive

Do not freeze a closed speaker enum. Add labeled surround/ambisonics/immersive semantics only with a product that requires them and explicit ordering/normalization/mapping fixtures.

Dolby Atmos object/metadata/renderer workflows remain separate from channel-bed layouts.

## Governance

Before accepting substantive outside code or selling commercial licenses:

- establish contributor/relicensing terms;
- define commercial license terms;
- verify notices/attribution generation;
- re-check distributed dependency compatibility with AGPL and proprietary licensing.
