# Roadmap

This roadmap is ordered by framework completeness and architectural proof, not by migration of any existing product.

## Current position

Chassis has a qualified format-independent runtime and native CLAP foundation. The core ownership/schema migration is substantially complete: one coherent component schema owns audio/event/parameter metadata, whole-audio-I/O policy, and semantic state identity; active processing uses dense audio/event/parameter identities; audio/event metadata may be runtime-owned; complete runtime state has one schema-owned identity authority; and the same runtime model has been exercised by effects, event-only processors, instruments, multi-output components, layout/sidechain reactivation, dynamic hosted metadata, and CLAP deployment.

Core lifecycle/state/parameter ownership, generic process buffers, f32/f64 processing, sample-accurate automation, bounded telemetry, semantic note foundations, offline/render mode, activation-scoped latency, restart signaling, and CLAP projection are implemented and tested.

The project is still pre-alpha because the framework is not yet broadly usable for new audio software without bespoke infrastructure. The remaining work is organized around a complete Rust audio-software foundation rather than an old plugin migration or one product class.

Plugins remain the first and likely most common deployment path, but they are not the architectural boundary. A stable Chassis should also serve instruments, standalone processors, plugin hosts, audio engines/DAWs, offline tools, analyzers, and embedded Rust audio applications through the same core semantics.

## Completion bar

Chassis is meaningfully complete when a new project can use coherent, documented, tested framework layers for:

- neutral effects/instruments/event-processors/embedded/offline component semantics;
- mono/stereo/multi-bus/sidechain/multi-output audio I/O;
- typed parameters, automation, modulation, gestures, formatting/mapping, and versioned state;
- sample-accurate note/MIDI/expression input and bounded output events;
- realtime, buffered-realtime, and offline processing with latency, tail, bypass, and deterministic behavior;
- bounded control -> DSP publication and DSP -> control/editor telemetry;
- explicit background-task lifecycle and safe result/reclamation publication;
- common reusable DSP utilities/integrations without forcing products to assemble avoidable plumbing;
- production editor lifecycle and toolkit adapters without making a GUI toolkit part of core;
- CLAP, VST3, and Audio Unit with differential semantic qualification;
- standalone audio/MIDI device deployment using the same component implementation;
- processing graph/routing/scheduling with latency propagation/compensation;
- third-party plugin hosting integrated with the same graph/runtime model;
- audio-file/media and offline-render integration suitable for general audio applications;
- reproducible packaging, identity, validation, compatibility, examples, documentation, and release tooling.

A DAW/application document model is explicitly outside this completion bar: tracks, clips, arrangements, project workflows, editor commands, mixer UX, and content/media-library policy remain application concerns.

AAX, ARA, immersive/object workflows, sandboxed hosting, network/distributed audio, and specialized ML infrastructure remain later or demand-driven unless they become necessary for a general framework invariant.

## Foundation — implemented

The following are substantially implemented and remain regression-protected:

- format-independent `Component` / `ComponentSchema` / `InstanceRuntime<P>` / `Processor` ownership;
- one coherent schema authority for semantic state identity, audio ports, `AudioIoPolicy`, event ports, and parameters;
- neutral component semantics with conventional effect helpers above core;
- safe process-buffer relationships and generic buffer sources;
- dense audio callback endpoints with activation-owned endpoint legality validation;
- owned-capable audio/event metadata with dense process-time projection;
- explicit whole-audio-I/O policy and reactivation across accepted layouts;
- arbitrary mapped mono/stereo ports supported by current layout semantics;
- multi-output routing independent of callback buffer order;
- f32/f64 processor capability model;
- typed parameter schema/store and dense realtime indices;
- borrowed sample-accurate automation trajectories;
- versioned transactional complete semantic state with schema-owned identity, migration/resource-limit coverage, and proof-era runtime compatibility APIs removed;
- activation failure recovery and panic containment at adapter boundaries;
- post-activation allocation/deallocation checks;
- activation-scoped processing latency and restart requests;
- realtime/buffered/offline process mode semantics;
- bounded coherent `f32` DSP -> control telemetry with meter/runtime fixtures;
- stable event-port schema/identity and bounded semantic note-event input in core;
- runtime ownership/validation of event-port schemas and note-event port capabilities;
- event-only and event-driven instrument runtime fixtures;
- runtime-owned dynamic/hosted metadata fixture;
- native CLAP adapter through pinned Clack;
- three-platform headless validation, packaged CLAP conformance, bounded fuzz, and in-process host coverage;
- representative adapter-overhead measurements;
- REAPER 7.79/macOS-arm64 scan/instantiate, render, state, active-save, and PDC evidence.

Existing conformance components, the delayed probe, and Tonal EQ are retained where they provide useful regression evidence. They are not roadmap authorities.

## Slice A — core semantic/API closure

Status: **ownership/schema migration complete; remaining audio-semantic contracts are current work.**

Completed architecture work:

- neutral core defaults: a `Component` is not inherently a stereo effect or plugin;
- conventional effect helpers above neutral semantics;
- one immutable component-schema authority and coherent runtime construction;
- stable audio/event identity with dense activation-local processing identity;
- owned metadata for dynamic/hosted components;
- semantic whole-I/O policy for multiple legal layouts;
- canonical semantic state identity ownership independent of backend deployment IDs;
- runtime removal of proof-order constructors and duplicate state-identity compatibility APIs;
- heterogeneous proof clients spanning effect, event-only, instrument, multi-output, sidechain/layout switching, embedded/direct use, dynamic metadata, and CLAP deployment.

Remaining Slice A work:

- parameter formatting/value mapping with tested round trips;
- explicit sample-accurate modulation distinct from durable/base state and ordinary automation;
- common tail semantics;
- bypass semantics where environments can represent them faithfully;
- recurring bus/port access ergonomics only if real clients show enough repeated cost to justify an abstraction;
- explicit capability queries only where deployment/application environments genuinely require them.

Adapter concepts such as `ClapStereoEffect` should be generalized when implementing a real non-effect CLAP deployment, not renamed speculatively in isolation.

The project is unpublished pre-alpha. Prefer a breaking simplification now over permanent compatibility shims around weak abstractions, but do not reopen candidate-stable core ownership merely for theoretical generality.

## Slice B — realtime communication and non-realtime work

Complete the small reusable concurrency/lifecycle toolbox repeatedly needed by audio processors:

- bounded DSP -> UI/control telemetry with explicit coalescing/overflow behavior;
- immutable control -> DSP snapshots for non-parameter state;
- safe deferred destruction/reclamation of replaced large objects;
- explicit background-task lifecycle: ownership, cancellation, generation/stale-result rejection, unload/shutdown, and offline determinism;
- larger/high-rate analyzer transport only if a fixed snapshot is measurably the wrong representation.

Unused facilities must impose no process-time work. Do not add a hidden global executor or generalized lock-free container library.

## Slice C — events, instruments, MIDI, and modulation

Finish the executable event model across core and deployment adapters.

Implemented core proof:

- semantic note on/off/choke/end and note addressing;
- dense event-port identity and validation;
- bounded semantic note input;
- zero-audio event processor;
- event-driven output-only instrument;
- multi-output runtime component.

Remaining:

- velocity/tuning/pressure/timbre/brightness/pan/volume expression where semantics are defined;
- raw MIDI 1 and SysEx;
- non-destructive MIDI 2 / UMP path;
- deployment projection of sample-accurate parameter modulation;
- bounded realtime output event sinks with explicit rejection behavior;
- note-end/output events;
- typed span/change-boundary processing without inventing cross-family total ordering;
- high-event-count allocation/work bounds;
- CLAP note-port/input/output projection;
- fuller synth and event passthrough/transform fixtures through deployment adapters.

A non-effect CLAP fixture should also drive generalization of the current effect-shaped CLAP facade.

## Slice D — common DSP foundation

Build a coherent set of broadly reusable DSP utilities and dependency integrations needed across audio software.

Candidate families, added from concrete requirements and tests rather than checklist completion:

- gain/dB/unit helpers;
- ramps/smoothing/interpolation;
- delay/ring/scratch buffers;
- metering and simple analysis;
- filters/crossovers and oscillator/envelope primitives where a common API is justified;
- FFT/STFT/window integration;
- oversampling/resampling integration;
- convolution/common channel-buffer operations;
- deterministic utility behavior shared between realtime and offline paths.

Use strong existing Rust libraries when appropriate. Do not reimplement mature FFT, resampling, codec, or device facilities merely to make them Chassis-owned. Package splits follow dependency/optionality boundaries, not this conceptual list.

## Slice E — editor/UI integration

Chassis owns editor lifecycle/integration, not visual design.

Complete:

- product-originated begin/change/end gestures and echo suppression;
- parameter observation/binding;
- telemetry subscription/publication;
- parent-window attach/detach;
- editor recreation/teardown;
- resize, scale, and high-DPI behavior;
- focus/input semantics required by supported environments;
- accessibility path where practical;
- optional GUI toolkit adapters with no headless dependency leak.

Validation fixture: `EditorDemo`, deliberately simple visually but exhaustive in lifecycle/interaction behavior.

## Slice F — plugin deployment parity

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

## Slice G — standalone and device runtime

Add first-class application/device infrastructure using the same component/runtime model:

- audio device enumeration/open/close/reconfiguration;
- sample-rate/block-size negotiation;
- audio input/output and duplex operation;
- MIDI device input/output and timestamp integration where available;
- xruns, device loss, errors, and recovery policy;
- standalone application bootstrap;
- reuse of the same processor/editor/state implementation used by plugins and embedded applications.

Use reviewed platform/device dependencies behind Chassis-owned semantic boundaries rather than building OS audio backends unnecessarily.

## Slice H — graph, routing, and scheduling

Add `chassis-graph` above the component core:

- ordinary Chassis processors as nodes;
- audio/event fan-in/fan-out;
- routing and bus connectivity;
- graph validation and immutable/transactional execution-plan publication;
- latency propagation and compensation;
- realtime-safe bounded execution;
- topology/resource mutation off the callback;
- deterministic offline graph execution;
- parallel scheduling only when measurements justify it.

The graph does not own tracks, clips, timelines, arrangements, mixer UX, mastering workflows, media libraries, or product-specific semantics.

## Slice I — plugin hosting

Add reusable hosting infrastructure for DAWs, hosts, test tools, and audio applications:

- discovery/scanning and format capability metadata;
- loading/instantiation/lifecycle;
- audio/event processing integration with `chassis-graph`;
- parameters/automation/state access;
- editor hosting;
- failure/crash policy at the supported isolation level;
- CLAP first where useful, then other supported formats without forcing one plugin ABI into core.

Sandboxed/out-of-process hosting is a later capability unless required by a concrete host application.

## Slice J — media, transport, and offline application infrastructure

Provide reusable infrastructure needed by general audio applications without becoming a DAW document model:

- audio source/sink metadata and stream/read/write/seek semantics;
- codec integration through reviewed libraries;
- sample-rate/channel adaptation through reviewed DSP dependencies;
- deterministic offline rendering of components/graphs;
- application transport/time primitives needed to drive processors and graphs;
- file/asset processing integration;
- a separate random-access/multi-pass processing abstraction if sequential block processing proves insufficient.

Project media libraries, clip/region editing, arrangements, project formats, and workflow semantics remain outside Chassis.

## Slice K — tooling, documentation, and stability

Make new projects cheap to create, validate, and ship:

- curated public facade that hides internal crate topology for common users;
- canonical identity/export manifest and generated backend metadata;
- plugin/standalone packaging;
- macOS signing/notarization and Windows signing hooks where release targets require them;
- reproducible release builds and versioning;
- validator/host/device smoke-test commands;
- state/identity compatibility fixtures;
- performance/regression harnesses;
- concise examples/templates for effect, sidechain, meter/analyzer, instrument, event processor, multi-output, worker-backed processor, standalone, graph, and host use;
- API/reference documentation with explicit realtime and ownership contracts.

Only after supported surfaces are exercised across these layers should Chassis freeze compatibility-sensitive public APIs and state formats.

## Later / conditional

- AAX when Avid/PACE access, licensing, and demand justify it;
- ARA/random-access host integration when region/asset semantics require host-level integration beyond generic offline processing;
- LV2 if useful for supported Linux workflows;
- sandboxed/out-of-process hosting when crash isolation becomes a supported requirement;
- surround/ambisonics/immersive channel semantics with explicit ordering/normalization fixtures;
- Dolby Atmos object/metadata/renderer workflows as a separate capability;
- network/distributed audio infrastructure;
- additional GUI adapters;
- specialized ML/model-runtime infrastructure only if repeated audio clients demonstrate a common lifecycle/API.

## Validation strategy

Prefer focused conformance fixtures over product-migration gates:

- `Gain` — parameter automation/state;
- `Delay` — latency/PDC/restart;
- `Meter` — scalar telemetry;
- `Analyzer` — larger bounded telemetry;
- `Sidechain` — routing and auxiliary input / layout policy;
- `Synth` — notes/MIDI/expression;
- `EventTransform` — bounded input/output events;
- `MultiOut` — multiple output buses;
- `DynamicMetadata` — runtime-owned hosted/embedded metadata;
- `WorkerFx` — background work/cancellation/stale-result rejection;
- `EditorDemo` — editor lifecycle/gestures/scaling;
- `DeviceLoop` — device negotiation/xrun/recovery;
- `GraphProbe` — routing/topology/PDC;
- `HostProbe` — plugin loading/state/editor/processing;
- `OfflineRender` — deterministic graph/component rendering.

Real products and applications remain valuable integration tests, but no old product architecture is preserved merely to make Chassis look compatible with it.

## Promotion rule

A facility belongs in the common framework when:

1. it is broadly useful across audio software or required to satisfy the completion bar;
2. owner/lifecycle and failure semantics are explicit;
3. realtime work/resources are bounded where applicable;
4. negative-space behavior is tested;
5. deployment/application layers can project the semantics faithfully or reject unsupported use;
6. performance claims have representative measurements;
7. dependencies remain compatible with safety/licensing/maintenance policy;
8. the abstraction is more durable than simply exposing a dependency's accidental API.

Framework completion should be deliberate, but not timid: implement known cross-cutting audio infrastructure, refactor weak pre-alpha abstractions aggressively, and leave application/product semantics to clients.
