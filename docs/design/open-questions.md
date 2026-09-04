# Open Questions and API Freeze Gates

This file records unresolved design decisions that still block API or production claims. Completed proof steps belong in `docs/design/validation.md`; execution order belongs in `docs/next-runtime-slice.md`.

## Runtime/API freeze

`InstanceRuntime<P>` is the sole durable format-independent authority for one component instance. `Processor` owns active DSP history; `Process<S>` expresses sample-representation capability.

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
- native production-host f64 dispatch remains desirable even though validator and in-process host coverage exercise f64 successfully.

## Parameters and automation

Implemented: typed schema/store, dense realtime indices, borrowed sample-accurate events, CLAP float/integer/boolean/choice projection, generation-checked scalar publication, state value rescan after load, and exported `trim` DSP that follows sample-accurate automation for f32/f64 processing.

Freeze gates:

- finalize product-originated begin/change/end gesture semantics and echo suppression when an editor client requires them;
- modulation must remain distinct from base-state publication and must not overwrite durable values;
- parameter mapping/formatting helpers must have explicit round-trip/domain tests before becoming generic authoring conveniences;
- production-DAW automated-render evidence remains required despite executable core/export/in-process-host trajectory coverage.

## Active state save/load

The CLAP consistency contract is implemented and executable:

- state save is a non-realtime operation and may wait for an in-progress scalar publisher;
- it serializes one coherent completed publication generation;
- a save racing a process block may observe the generation immediately before or after endpoint publication, never a mixed generation;
- a newer control edit/state load wins over stale realtime publication by generation check;
- state load remains transactional and requests host value rescan after successful publication.

Direct tests, Loom, and an in-process CLAP host scenario now cover this behavior while audio processing is active. Remaining freeze gates are production-DAW confirmation and cross-format round trips once VST3/AU exist.

The CHSS wire envelope remains pre-v1 until production-host and cross-format state behavior is established.

## Latency / lookahead

Implemented:

- `LatencySamples` in core;
- `Processor::latency()` with zero default;
- activation-scoped latency snapshot in `InstanceRuntime`;
- lifecycle tests proving the value cannot drift within one activation and is recomputed on reactivation;
- CLAP `PluginLatency` projection plus host latency-change notification when a new activation changes the value;
- deliberate delayed CLAP probe proving reported 64-sample latency corresponds to an impulse delayed by exactly 64 samples.

Still open:

- verify production-host PDC/alignment and restart/reactivation behavior when product configuration changes required latency;
- determine higher-level lookahead/resource authoring conveniences from a real delayed product rather than inventing them in advance;
- prove cross-format latency parity once VST3/AU projections exist.

## Realtime guarantees

Core and full CLAP adapter callback allocation/deallocation evidence are now executable. The adapter test covers stereo main + stereo sidechain, the configured 64-event maximum, f32 and f64, and repeated callbacks without measured callback-thread allocation/deallocation. Frame-count minimum/maximum and over-bound rejection are also covered.

Remaining promotion work:

- measure adapter-only overhead on representative stable hardware at relevant block sizes/event loads;
- verify replaced-object reclamation cannot fall onto the callback when larger snapshot/telemetry facilities appear.

Do not introduce custom allocators, lock-free infrastructure, SIMD, padding, or unsafe zero-copy machinery merely to make benchmarks look sophisticated.

## CLAP production qualification

Current Linux headless qualification is current:

- `clap-validator` 0.4.1: 35 passed, 0 failed, 9 intentional skips;
- bounded five-second two-worker fuzz: clean;
- in-process Clack host exercises allocation/work bounds, activation failure/retry, repeated instances, reactivation, thread transfer, active state save, latency notification, and delayed audio.

Open production-host work:

- current-head DAW scan/instantiate/state reopen;
- actual automated `trim` render differential against expected sample offsets in a production DAW;
- active state save during DAW automation;
- real-host PDC/alignment;
- additional OS/architecture/host coverage;
- native host f64 dispatch when a production host can be induced to choose it;
- Bitwig/reentrancy coverage where available.

The historical REAPER 7.79/macOS-arm64 baseline predates current head and remains regression history only.

Clack remains pinned to reviewed revision `c5975f9f89f0953b00768680357985d46178078a`. Any revision/release change is an audit and conformance event.

## Export metadata/API

Current CLAP export identity constants are temporary proof metadata. Before shipping a product:

- define one canonical product/export identity manifest;
- generate backend IDs/metadata from that owner rather than duplicating constants across adapters;
- support explicit import/legacy mappings where released compatibility requires them;
- freeze byte-order/identity fixtures.

Temporary names that encode obsolete proof limits should be renamed before real clients depend on them. `ClapStereoEffect` remains a candidate if its name becomes misleading relative to the generic mapped I/O already supported.

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
