# Roadmap

This roadmap is ordered by framework completeness and architectural proof, not by migration of any existing product.

## Current position

Chassis has a qualified format-independent runtime and native CLAP foundation. Core lifecycle/state/parameter ownership, generic process buffers, f32/f64 processing, sample-accurate automation, offline/render mode, activation-scoped latency, restart signaling, and CLAP projection are implemented and tested. REAPER/macOS host evidence exists for automation, state round-trip, active-save, and PDC through the delayed probe.

The project is still pre-alpha because the framework is not yet broadly usable for new professional audio projects without bespoke infrastructure. The remaining work is now organized around that completion bar rather than Tonal EQ or limiter migration parity.

## Completion bar

Chassis is meaningfully complete for general audio work when a new project can use one coherent component/runtime model for:

- effects, instruments, event processors, embedded processing, and standalone deployment;
- mono/stereo/multi-bus/sidechain/multi-output audio I/O;
- typed parameters, automation, modulation, gestures, formatting/mapping, and versioned state;
- sample-accurate note/MIDI/expression input and bounded output events;
- realtime and offline processing, latency, tail, bypass, lookahead/oversampling-friendly activation resources, and deterministic rendering;
- bounded control -> DSP snapshots and DSP -> control/editor telemetry;
- explicit background-task lifecycle when non-realtime analysis or preparation is required;
- production editor lifecycle and bindings without putting a GUI toolkit in core;
- CLAP, VST3, and Audio Unit with differential semantic qualification;
- standalone audio/MIDI device deployment using the same component implementation;
- application-level graph/scheduling infrastructure without moving DAW/application semantics into core;
- reproducible packaging, signing/notarization, validation, examples, and release tooling.

AAX, ARA, immersive/object workflows, plugin hosting, sandboxing, and specialized ML/DSP infrastructure remain later or demand-driven unless they become necessary to satisfy the general completion bar above.

## Foundation — implemented

The following are substantially implemented and remain regression-protected:

- format-independent `Component` / `InstanceRuntime<P>` / `Processor` ownership;
- safe process-buffer relationships and generic buffer sources;
- arbitrary mapped mono/stereo ports supported by current layout semantics;
- f32/f64 processor capability model;
- typed parameter schema/store and dense realtime indices;
- borrowed sample-accurate automation trajectories;
- versioned transactional state with migration/resource-limit coverage;
- activation failure recovery and panic containment at adapter boundaries;
- post-activation allocation/deallocation checks;
- activation-scoped processing latency and restart requests;
- realtime/offline process mode projection;
- native CLAP adapter through pinned Clack;
- three-platform headless validation, packaged CLAP conformance, bounded fuzz, and in-process host coverage;
- representative adapter-overhead measurements;
- REAPER 7.79/macOS-arm64 scan/instantiate, render, state, active-save, and PDC evidence.

Existing conformance components, the delayed probe, and Tonal EQ are retained where they provide useful regression evidence. They are not roadmap authorities.

## Slice A — core authoring/API closure

Before stabilizing the pre-v1 authoring surface:

- make `Component` -> runtime construction ergonomic without hiding validation/ownership;
- define semantic whole-I/O configuration for components with multiple legal layouts;
- add higher-level bus/port access only where it removes repeated product boilerplate without obscuring buffer legality;
- finish canonical product/export identity ownership and backend projection;
- complete parameter value mapping/formatting helpers with round-trip tests;
- finish explicit parameter modulation semantics without corrupting durable/base state;
- define common tail and bypass semantics where formats can represent them faithfully;
- retain activation-time resource ownership for lookahead, oversampling, convolution, model state, and similar delayed DSP;
- keep product-specific DSP and visual semantics outside core.

Do not freeze convenience macros or derives until the underlying authoring surface is coherent.

## Slice B — realtime communication and non-realtime work

Add a small set of reusable primitives that professional processors repeatedly need:

- bounded DSP -> UI/control telemetry with explicit publication, overflow/coalescing, and reclamation behavior;
- immutable control -> DSP snapshots where scalar parameters are not enough;
- deferred destruction of replaced large state away from the audio callback;
- explicit background-task lifecycle for analysis/preparation: ownership, cancellation, generation/stale-result rejection, unload/shutdown, and offline determinism.

Unused facilities must impose no process-time work. Do not add a hidden global executor or generic lock-free container library.

Prove these with purpose-built fixtures such as a meter/analyzer and worker-backed processor rather than an old product migration.

## Slice C — events, instruments, and modulation

Promote the existing event design into executable core/adapters:

- stable event-port identities;
- semantic note on/off/choke/end and note addressing;
- velocity, tuning, and per-note expression;
- raw MIDI/SysEx and a path for MIDI 2 / UMP without destructive down-conversion;
- sample-accurate parameter modulation distinct from base automation/state;
- bounded realtime output event sinks with explicit rejection behavior;
- typed borrowed cursors/span processing without forcing one fabricated cross-family total order.

Prove with small fixtures: a basic synth, an event passthrough/transform, and a multi-output instrument.

## Slice D — production editor contract

Chassis owns editor lifecycle/integration, not product visual design.

Complete:

- product-originated begin/change/end gestures and echo suppression;
- parameter observation/binding;
- telemetry subscription/publication;
- parent-window attach/detach;
- editor recreation/teardown;
- resize, scale, and high-DPI behavior;
- focus/input semantics required by supported formats;
- accessibility path where practical;
- enough rendering/control flexibility for commercial product UIs.

Select GUI adapters based on evidence. Toolkit types stay out of product-facing core contracts.

## Slice E — desktop format parity

Bring VST3 and Audio Unit to the same semantic standard as CLAP, initially through a pinned/reviewed wrapper if it is faithful enough.

For each format prove:

- identity and metadata;
- parameters/automation/modulation;
- state round-trip and migration fixtures;
- audio/event I/O and layout negotiation;
- latency/PDC, tail, and bypass;
- editor lifetime/resize/scaling;
- real-host save/reopen and automated rendering;
- differential output/state behavior against native CLAP where semantics are equivalent.

Own native adapters only when wrapper limitations are concrete and material.

## Slice F — standalone and device runtime

Add a first-class standalone deployment using the same component/runtime implementation:

- audio device enumeration/open/close/change handling;
- sample-rate/block-size negotiation;
- MIDI device input/output;
- xrun/error reporting and recovery policy;
- component/editor/state reuse rather than a parallel standalone DSP path.

This is the threshold at which Chassis becomes useful beyond plugin-only projects.

## Slice G — graph/scheduling layer

Add `chassis-graph` only above the component core:

- nodes backed by ordinary Chassis components/processors;
- fan-in/fan-out and bus/event routing;
- graph validation and immutable/transactional topology publication;
- latency propagation and compensation;
- realtime-safe execution planning;
- topology/resource mutation off the callback;
- parallel scheduling only if measurement and workloads justify it.

The graph does not own timelines, arrangements, mixer UX, mastering workflows, media libraries, or product-specific semantics.

## Slice H — release/tooling completion

Make new projects cheap to create and ship:

- canonical product/export manifest and generated backend metadata;
- packaging layouts for supported plugin formats and standalone apps;
- macOS signing/notarization and Windows signing hooks where release targets require them;
- reproducible release builds and versioning;
- validator/host smoke-test commands;
- state/identity compatibility fixtures;
- performance/regression harnesses;
- concise examples/templates for effect, sidechain, meter/analyzer, instrument, multi-output, worker-backed processor, and standalone deployment.

The goal is not a large CLI for its own sake; it is removing repeated release and integration boilerplate.

## Later / conditional

- `chassis-host` for third-party plugin discovery/loading/editor hosting when a real application needs it;
- AAX when Avid/PACE access, licensing, and demand justify it;
- ARA/random-access host integration when a product needs region/asset semantics beyond sequential offline processing;
- LV2 if Linux demand warrants it;
- sandboxed/out-of-process hosting when crash isolation is a product requirement;
- surround/ambisonics/immersive channel semantics with explicit ordering/normalization fixtures;
- Dolby Atmos object/metadata/renderer workflows as a separate capability;
- additional GUI adapters where supported products need them.

## Validation strategy

Prefer focused conformance fixtures over product-migration gates:

- `Gain` — parameter automation/state;
- `Delay` — latency/PDC/restart;
- `Meter` — scalar telemetry;
- `Analyzer` — larger bounded telemetry;
- `Sidechain` — routing and auxiliary input;
- `Synth` — notes/MIDI/expression;
- `MultiOut` — multiple output buses;
- `WorkerFx` — background work/cancellation/stale-result rejection;
- `EditorDemo` — editor lifecycle/gestures/scaling.

Real products remain valuable integration tests, but no old product architecture is preserved merely to make Chassis look compatible with it.

## Promotion rule

A facility belongs in the common framework when:

1. it is broadly required to satisfy the completion bar or repeated across materially different component classes;
2. owner/lifecycle and failure semantics are explicit;
3. realtime work/resources are bounded where applicable;
4. negative-space behavior is tested;
5. adapters can project the semantics faithfully or reject unsupported use;
6. performance claims have representative measurements;
7. dependencies remain compatible with realtime/safety/licensing policy.

Framework completion should be deliberate, but not speculative: implement known cross-cutting audio-runtime requirements now; leave domain-specific algorithms and application semantics to clients.
