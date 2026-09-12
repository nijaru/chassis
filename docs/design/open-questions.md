# Open Questions and API Freeze Gates

This file records unresolved design decisions that still block API or production claims. Completed proof steps belong in `docs/design/validation.md`; execution order belongs in `docs/next-runtime-slice.md`; the broad completion bar belongs in `docs/roadmap.md`.

Existing product migrations are evidence sources, not API-freeze gates.

## Runtime/API freeze

`InstanceRuntime<P>` is the sole durable format-independent authority for one component instance. `Processor` owns active DSP history; `Process<S>` expresses sample-representation capability.

Still unresolved before freezing the authoring surface:

- final ergonomics for constructing `InstanceRuntime` from a `Component` without hiding schema validation or ownership;
- semantic whole-I/O policy for products that negotiate multiple legal layouts;
- recurring higher-level bus/port access patterns that preserve explicit buffer legality;
- canonical product/export identity ownership independent of backend IDs;
- common tail/bypass semantics where formats can map them faithfully;
- parameter formatting/value-mapping helpers with explicit domain and round-trip tests;
- explicit modulation semantics separate from durable/base parameter publication;
- whether common activation/resource helpers for lookahead, oversampling, convolution, model state, etc. remove enough repeated boilerplate to justify framework API.

Keep `Processor` non-`Send` in core unless a deployment requires transfer; adapters add the bound at their boundary.

Do not add proc macros/derives until the underlying authoring contracts are coherent and repeated declarations are known.

## Process buffers and I/O

The generic safe source shape is implemented and the CLAP adapter traverses setup-mapped channels without callback-owned channel collections.

Freeze gates:

- semantic layout negotiation beyond structural port presence;
- ergonomic bus/port views without manufacturing invalid aliases;
- target-specific legality for inactive/null/zero-buffer cases as formats require;
- benchmark controlled copy/conversion paths before adding ownership or unsafe complexity to remove them;
- cross-format layout parity once VST3/AU projections exist.

Native f64 processing is qualified headlessly. Whether a product advertises native 64-bit wire preference remains product/deployment policy.

## Parameters, automation, modulation, and gestures

Implemented: typed schema/store, dense realtime indices, borrowed sample-accurate automation, CLAP float/integer/boolean/choice projection, generation-checked scalar publication, state value rescan after load, and sample-accurate exported automation for f32/f64.

Freeze gates:

- product-originated begin/change/end gesture semantics and echo suppression;
- modulation must remain distinct from base-state publication and must not overwrite durable values;
- mapping/formatting helpers require explicit round-trip/domain tests;
- adapter-specific automation/modulation semantics must be preserved rather than normalized to a weaker invented model;
- production-DAW automated-render evidence remains required despite executable core/export/in-process-host coverage.

## Active state save/load

The CLAP consistency contract is implemented and executable:

- state save is a non-realtime operation and may wait for an in-progress scalar publisher;
- it serializes one coherent completed publication generation;
- a save racing a process block may observe the generation immediately before or after endpoint publication, never a mixed generation;
- a newer control edit/state load wins over stale realtime publication by generation check;
- state load remains transactional and requests host value rescan after successful publication.

Direct tests, Loom, and an in-process CLAP host scenario cover this behavior while audio processing is active.

Remaining freeze gates:

- production-host confirmation across supported release hosts;
- cross-format state round trips once VST3/AU exist;
- canonical product/export identity fixtures;
- decide when the CHSS wire envelope is stable enough to become a compatibility promise.

## Latency, tail, bypass, and delayed processing

Implemented:

- `LatencySamples` in core;
- `Processor::latency()` with zero default;
- activation-scoped latency snapshot in `InstanceRuntime`;
- `Processor::restart_requested()` exposed through runtime and forwarded by CLAP after completed parameter publication;
- state-load restart request behavior for activation-resource changes;
- lifecycle tests proving latency cannot drift inside one activation;
- CLAP latency projection and host notification;
- delayed probe proving reported latency corresponds to delayed audio;
- recorded REAPER PDC alignment evidence.

Still open:

- common tail semantics;
- bypass semantics and whether hard/soft/product bypass need distinct capability surfaces;
- authoring conveniences for activation-time lookahead/oversampling resources if repeated use proves them worthwhile;
- cross-format latency/tail/bypass parity once VST3/AU exist.

## Realtime guarantees and telemetry

Core and full CLAP adapter callback allocation/deallocation evidence are executable. The adapter benchmark has representative Apple M3 Max results recorded in the validation guide.

`chassis-core::telemetry::F32Telemetry` is the first framework-owned DSP -> non-realtime observation primitive. It provides fixed-width coherent `f32` snapshots with construction-time allocation, nonblocking publication, bounded reads, and no unsafe code.

Remaining promotion work:

- CI/concurrency evidence for telemetry publication and coherent reads;
- prove use through a meter fixture;
- determine whether large/high-rate analyzer payloads need a different bounded transport rather than scaling the scalar snapshot indefinitely;
- add immutable control -> DSP snapshots for non-parameter product state when a concrete representation can preserve realtime ownership and reclamation safely;
- verify replaced-object reclamation cannot fall onto the callback when larger snapshot/task facilities appear.

Do not introduce custom allocators, cache padding, generalized lock-free containers, SIMD, or unsafe zero-copy machinery merely to make the framework appear complete.

## Events, notes, MIDI, and output events

`docs/design/events-transport.md` contains the current semantic design. This is now active framework-completion work rather than something deferred indefinitely to a product migration.

Before stable instrument/event claims, implement and prove:

- stable event-port identities;
- semantic note on/off/choke/end and note addressing;
- velocity, tuning, and per-note expression;
- raw MIDI/SysEx and a non-destructive path toward MIDI 2 / UMP;
- sample-accurate modulation;
- bounded output event sinks with explicit rejection behavior;
- multiple event/audio outputs;
- source-order preservation without fabricating total order where a backend does not provide it;
- high-event-count and realtime allocation/work bounds.

Use focused synth/event/multi-output fixtures as evidence.

## Re-entrancy and panic boundary

The pinned Clack revision includes the required main-thread re-entrancy fixes. Chassis has in-process plugin -> host -> plugin tests through parameter-rescan and latency-change callbacks. Bitwig remains useful production confirmation, not a semantic blocker.

Pinned Clack catches plugin callback panics at its FFI wrapper. Executable behavior remains:

- product activation panic -> activation failure, plugin remains reusable;
- product process panic -> host-visible processing failure followed by clean stop/deactivation.

Do not promise continued processing after arbitrary DSP panic.

## CLAP production qualification

The three-platform headless workflows cover:

- Linux/macOS/Windows workspace portability tests/checks;
- packaged `.clap` artifacts;
- pinned `clap-validator` 0.4.1: 35 passed, 0 failed, 9 intentional skips at the recorded baseline;
- bounded fuzz;
- in-process host coverage for allocation/work bounds, lifecycle failures, panic containment, repeated instances, reactivation, thread transfer, re-entrancy, active state save, latency notification, and delayed audio;
- dependency advisory/license/source/dead-dependency policy gates.

Recorded REAPER 7.79/macOS-arm64 evidence covers scan/instantiate/state reopen, automated render, active save during automation, and PDC alignment.

Additional release-host/architecture coverage should follow concrete support targets.

## Export metadata/API

Current CLAP export identity constants remain proof-oriented.

Before shipping products:

- define one canonical product/export identity manifest or owner;
- derive backend IDs/metadata from that owner rather than duplicating constants across adapters;
- support explicit legacy/import mappings where released compatibility requires them;
- freeze byte-order/identity fixtures;
- rename temporary adapter concepts whose names encode obsolete proof limits.

## VST3/AU projection

Treat wrapper output as a separate adapter qualification problem, not proof by construction.

Before claiming VST3/AU support, run the same product semantics through:

- native validators;
- state/identity fixtures;
- automation and modulation trajectory tests;
- audio/event buffer/layout tests;
- latency/PDC/tail/bypass parity;
- editor lifetime/resize/scaling tests;
- real-host save/reopen/automation scenarios, including Ableton where wrapper state restoration is host-sensitive;
- differential comparison against native CLAP where semantics are equivalent.

Own a native format adapter only when concrete wrapper limitations justify it.

## GUI/editor

Chassis owns editor lifecycle/integration but not visual design.

Before a first-class production editor claim:

- parent window attach/detach;
- resize/scale/high-DPI behavior;
- editor recreation/teardown;
- parameter observations and gestures;
- realtime telemetry ownership/subscription;
- focus/input behavior required by supported formats;
- accessibility path where practical;
- enough rendering/control flexibility for commercial product UIs.

Select toolkit adapters from evidence and keep toolkit/platform types out of core.

## Background work and larger snapshots

No hidden global executor.

Before adding framework task services, specify:

- instance/module lifetime;
- cancellation;
- generation/stale-completion rejection;
- unload/shutdown behavior;
- result destruction ownership;
- offline determinism;
- whether work may continue while transport is stopped or the editor is closed.

`F32Telemetry` covers small observational snapshots only. Larger immutable state publication and worker-result publication remain separate design problems.

## Standalone and devices

Before calling standalone deployment first-class, prove:

- audio device enumeration/open/close/change behavior;
- sample-rate/block-size negotiation;
- MIDI device input/output;
- xrun/error reporting and recovery policy;
- reuse of the same component/runtime/editor/state implementation used by plugin deployments.

Do not grow a second standalone DSP architecture.

## Graph/scheduling

`chassis-graph` belongs above the component core.

Before a stable graph claim, prove:

- ordinary Chassis processors as nodes;
- audio/event fan-in/fan-out and routing;
- graph validation and immutable/transactional execution-plan publication;
- latency propagation/compensation;
- realtime-safe execution bounds;
- topology/resource changes off the callback;
- parallel scheduling only where measurements justify it.

Timelines, projects, media libraries, mixer UX, mastering workflows, and delivery remain application concerns.

## Surround / immersive

Do not freeze a closed speaker enum. Add labeled surround/ambisonics/immersive semantics only with explicit ordering/normalization/mapping fixtures.

Dolby Atmos object/metadata/renderer workflows remain separate from channel-bed layouts.

## Governance and release

Before accepting substantive outside code or selling commercial licenses:

- establish contributor/relicensing terms;
- define commercial license terms;
- verify notices/attribution generation;
- re-check distributed dependency compatibility with AGPL and proprietary licensing.

Before calling Chassis generally production-usable, also establish repeatable packaging, signing/notarization hooks, release versioning, compatibility fixtures, validator/host smoke commands, and concise examples/templates for the supported component classes.
