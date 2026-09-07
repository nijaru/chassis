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
- production-host f64 wire dispatch is qualified headlessly (validator + in-process host cover both double-precision process cases); whether a client advertises `PREFERS_64BITS` is a first-client product decision, not a framework freeze gate.

## Parameters and automation

Implemented: typed schema/store, dense realtime indices, borrowed sample-accurate events, CLAP float/integer/boolean/choice projection, generation-checked scalar publication, state value rescan after load, and exported `trim` DSP following sample-accurate automation for f32/f64.

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

Direct tests, Loom, and an in-process CLAP host scenario cover this behavior while audio processing is active. Remaining freeze gates are production-DAW confirmation and cross-format round trips once VST3/AU exist.

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

Core and full CLAP adapter callback allocation/deallocation evidence are executable. The adapter test covers stereo main + stereo sidechain, the configured 64-event maximum, f32/f64, and repeated callbacks without measured callback-thread allocation/deallocation. Frame-count minimum/maximum and over-bound rejection are covered.

A manual adapter benchmark now exercises 32/64/128/512 frames, f32/f64, and zero/max events through the actual adapter.

Remaining promotion work:

- run that benchmark on representative stable hardware and record CPU/OS/toolchain/workload;
- verify replaced-object reclamation cannot fall onto the callback when larger snapshot/telemetry facilities appear.

Do not introduce custom allocators, lock-free infrastructure, SIMD, padding, or unsafe zero-copy machinery merely to make benchmarks look sophisticated.

## Re-entrancy

The exact Clack pin includes the v0.2 main-thread re-entrancy fixes. Chassis now has an in-process plugin -> host -> plugin test through both parameter-rescan and latency-change callbacks.

The semantic re-entrancy assumption is therefore executable rather than inferred from dependency source alone. Bitwig remains useful production-host confirmation when available, especially because it was one of the motivating real-world re-entrant hosts.

Any Clack revision/source change remains an audit + conformance event.

## Panic boundary

Pinned Clack catches plugin callback panics at its FFI wrapper.

Executable Chassis behavior:

- product activation panic -> activation failure, plugin remains inactive, same instance can activate again;
- product process panic -> host-visible processing failure, then clean stop/deactivation.

Do not promise that arbitrary DSP state is valid for continued processing after a process panic. The production contract should treat that callback failure as terminal for the current processing run. Real-host logging/restart behavior remains host qualification rather than a core semantic gap.

## CLAP production qualification

The three-platform headless workflows cover the following gates. Consult the [validation record](validation.md) and workflow results for the revision being qualified:

- Linux/macOS/Windows workspace portability tests/checks;
- Linux/macOS/Windows packaged `.clap` artifacts;
- pinned `clap-validator` 0.4.1: 35 passed, 0 failed, 9 intentional skips;
- bounded five-second two-worker fuzz on each platform;
- in-process Clack host exercises allocation/work bounds, activation failure/retry, panic containment, repeated instances, reactivation, thread transfer, re-entrancy, active state save, latency notification, and delayed audio;
- dependency advisory/license/source/dead-dependency policy gates are automated.

Current production-host evidence (2026-09-06, REAPER 7.79/macOS-arm64):

- DAW scan/instantiate/state reopen: recorded 2026-09-06 baseline;
- automated `trim` render differential: sample-exact at expected sample offsets;
- active state save during DAW automation: coherent, replays sample-exact;
- real-host PDC/alignment: sample-exact through the exported delayed probe.

Remaining production-host work:

- wire-precision preference (`PREFERS_64BITS`/`data64`) when the first real client's product semantics justify it;
- additional release architectures/hosts as targets become concrete;
- Bitwig confirmation when available.

The pre-2026-09 REAPER baseline predates current head and remains regression history only.

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
