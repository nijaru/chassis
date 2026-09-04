# Open Questions and API Freeze Gates

This file records unresolved design decisions that still block API or production claims. Completed proof steps belong in `docs/design/validation.md`; execution order belongs in `docs/next-runtime-slice.md`.

## Runtime/API freeze

`InstanceRuntime<P>` is the durable format-independent authority for one component instance. `Processor` owns active DSP history; `Process<S>` expresses sample-representation capability.

Still unresolved before freezing the authoring surface:

- remove the temporary activation-local `Activated<P>` / free `runtime::activate()` compatibility path after remaining tests migrate;
- decide the final spelling/ergonomics for creating `InstanceRuntime` from a `Component` without duplicating schema validation boilerplate;
- prove semantic whole-I/O policy beyond structural port presence for products that negotiate multiple legal layouts;
- keep `Processor` non-`Send` in core unless a deployment requires transfer; adapters add the bound at their boundary.

Do not add proc macros/derives until real client code demonstrates the repeated declarations worth hiding.

## Process buffers

The generic safe source shape is implemented and the CLAP adapter traverses setup-mapped channels without callback-owned channel collections.

Freeze gates:

- ergonomic higher-level bus/port views only after real DSP clients demonstrate recurring access patterns;
- target-specific legality for inactive/null/zero-buffer cases as formats require;
- benchmark controlled copy/conversion paths before adding ownership or unsafe complexity to remove them;
- native f64 host-dispatch evidence remains desirable, but lack of a host choosing `data64` does not invalidate validator-qualified f64 support.

## Parameters and automation

Implemented: typed schema/store, dense realtime indices, borrowed sample-accurate events, CLAP float/integer/boolean/choice projection, generation-checked scalar publication, and state value rescan after load.

Freeze gates:

- exported conformance DSP must audibly exercise sample-accurate automation rather than keeping parameters process-inert;
- finalize product-originated begin/change/end gesture semantics and echo suppression;
- modulation must remain distinct from base-state publication and must not overwrite durable values;
- parameter mapping/formatting helpers must have explicit round-trip/domain tests before becoming generic authoring conveniences.

## Active state save/load

The intended CLAP consistency contract is:

- state save is a non-realtime operation and may wait for an in-progress scalar publisher;
- it serializes one coherent completed publication generation;
- a save racing a process block may observe the generation immediately before or after that block's endpoint publication, never a mixed generation;
- a newer control edit/state load wins over stale realtime publication by generation check;
- state load remains transactional and requests host value rescan after successful publication.

Before freezing state APIs, add direct executable tests for the save/publication race and active host qualification during automation. Cross-format round trips remain required once VST3/AU exist.

The CHSS wire envelope remains pre-v1 and may change until these gates are complete.

## Realtime guarantees

The process architecture avoids explicit callback allocation and unbounded work, but inspection is not enough for a production claim.

Before promotion:

- instrument allocation/deallocation after activation;
- stress configured maximum parameter-event counts and representative mapped topologies;
- measure adapter-only overhead at representative block sizes;
- verify replaced-object reclamation cannot fall onto the callback when larger snapshot/telemetry facilities appear.

Do not introduce custom allocators, lock-free infrastructure, SIMD, padding, or unsafe zero-copy machinery merely to make benchmarks look sophisticated.

## CLAP production qualification

The current parametered artifact is qualified on the available validator + REAPER macOS-arm64 matrix. Production support still requires broader evidence.

Open:

- activation failure cleanup and malformed/partial lifecycle cases not covered by current validator runs;
- reentrant host callbacks under the pinned Clack model;
- additional OS/architecture/host coverage;
- active state save while automation is running;
- actual automated render differential after `trim` becomes process-active;
- Bitwig coverage when available.

Clack remains pinned to reviewed revision `c5975f9f89f0953b00768680357985d46178078a`. Any revision/release change is an audit and conformance event.

## Export metadata/API

Current CLAP export identity constants are temporary proof metadata. Before shipping a product:

- define one canonical product/export identity manifest;
- generate backend IDs/metadata from that owner rather than duplicating constants across adapters;
- support explicit import/legacy mappings where released compatibility requires them;
- freeze byte-order/identity fixtures.

Temporary names that encode obsolete proof limits should be renamed before real clients depend on them.

## VST3/AU projection

Treat `clap-wrapper` output as a separate adapter qualification problem, not proof by construction.

Before claiming VST3/AU support, run the same conformance product through:

- native validators;
- state/identity fixtures;
- automation trajectory tests;
- buffer/layout tests;
- editor lifetime/resize tests;
- real host save/reopen/automation scenarios;
- differential comparison against native CLAP semantics.

Own a native format adapter only when concrete wrapper limitations justify it.

## GUI

Do not make Iced or another toolkit part of the core contract prematurely.

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
