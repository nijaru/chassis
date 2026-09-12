# Current Execution Plan

This file tracks the next implementation order on `main`. It is not an API compatibility promise.

## Current checkpoint

The format-independent runtime and native CLAP foundation are implemented and qualified enough that the remaining work should be driven by the complete audio-framework architecture, not by an existing product migration.

Current executable foundation includes:

- one coherent `Component -> ComponentSchema -> InstanceRuntime<P> -> Processor` ownership model;
- neutral core semantics with explicit conventional effect helpers;
- owned-capable audio/event metadata with stable setup identity and dense callback identity;
- activation-owned `AudioIoPolicy`, structural validation, dense endpoint resolution, and callback endpoint legality;
- direct runtime proofs for conventional effects, zero-audio event processors, an event-driven instrument, multiple output buses, multiple legal layouts with sidechain, and runtime-owned dynamic/hosted metadata;
- f32/f64 processing and generic safe audio buffer sources;
- typed parameters, sample-accurate automation, transactional complete semantic state, activation-scoped latency, restart signaling, and realtime/buffered/offline process modes;
- schema-owned semantic state identity with proof-era runtime explicit-ID/parameter-only compatibility paths removed;
- `chassis-core::telemetry::F32Telemetry`, concurrency tests, measured callback allocation/deallocation coverage, and a public meter fixture;
- semantic note-port/note-input foundations with dense activation-local indices;
- native CLAP deployment through pinned Clack with headless conformance and recorded REAPER evidence.

Existing Tonal EQ, delayed-probe, and conformance coverage remain regressions/evidence. They are not architectural authorities.

The [validation guide](design/validation.md) owns executed evidence. The high-level framework completion bar lives in [roadmap.md](roadmap.md).

## Execution principle

Chassis is unpublished pre-alpha. Refactor aggressively when implementation choices encode duplicated authority, weak ownership semantics, or concrete proof limits.

Do not rewrite working code merely because it is old. Preserve implementations whose semantics fit the target architecture and whose tests provide useful evidence.

Use small purpose-built fixtures to prove framework behavior. Product-specific DSP, synth engines, restoration/ML algorithms, DAW document models, timelines, arrangements, mixer UX, and product workflows stay outside Chassis.

The core schema/runtime ownership model is now candidate-stable. Do not reopen it to add speculative policy languages, constructor families, or metadata abstractions without a concrete client that cannot be expressed cleanly.

## Slice 0 — semantic audit and proof-era cleanup

Status: **complete for the core ownership/schema migration.**

Completed:

- removed the implicit stereo-effect `Component` default;
- established one coherent `Component::schema()` authority;
- removed proof-order runtime constructors;
- moved audio/event processing identity to dense indices;
- made audio/event schema metadata runtime-ownable without callback strings;
- added schema-owned whole-audio-I/O policy and activation enforcement;
- separated semantic state identity from deployment identity;
- removed runtime explicit-product-ID and parameter-only state compatibility APIs;
- exercised event-only, instrument, multi-output, multi-layout/sidechain, direct embedded, dynamic-metadata, and CLAP clients through the same runtime model.

Remaining adapter names or facades that encode the first CLAP proof target, such as `ClapStereoEffect`, should be generalized when a real non-effect CLAP deployment is implemented. Do not churn them in isolation before the replacement capability is executable.

## Slice 1 — core authoring/API closure

Status: **current priority.**

The remaining cross-cutting core contracts are:

1. parameter formatting/value mapping with explicit domain and round-trip tests;
2. sample-accurate modulation as a process-time concept distinct from durable/base state and ordinary automation;
3. common tail semantics;
4. bypass semantics, including whether hard/soft/product bypass need distinct capabilities;
5. bus/port convenience only where current instrument/multi-output/layout proofs show repeated mechanical cost without hiding legality;
6. explicit capability queries only where an environment genuinely needs to distinguish semantics.

Do not add proc macros, derives, a richer I/O-policy language, or generalized component helper families unless these concrete contracts demonstrate the need.

Validation fixtures for the schema/runtime shape are already present. New Slice 1 fixtures should target the new behavior itself: formatting/mapping round trips, automation + modulation interaction, tail reporting, and bypass behavior.

## Slice 2 — realtime communication and background lifecycle

Status: **partially implemented.**

Implemented:

- fixed-width coherent `f32` DSP -> non-realtime telemetry;
- single-attempt nonblocking publication with explicit contention/generation behavior;
- bounded coherent readers;
- concurrency tests;
- measured-thread allocation/deallocation coverage;
- meter fixture through the public runtime.

Remaining:

- immutable control -> DSP publication for non-parameter state;
- deferred destruction/reclamation of replaced large immutable objects;
- explicit per-instance background-work lifecycle: cancellation, generation/stale-result rejection, shutdown/unload, result publication, and offline determinism;
- larger/high-rate analyzer transport only if a real analyzer fixture proves fixed snapshots inappropriate.

Do not generalize this into a lock-free container library or hidden global executor.

## Slice 3 — events, MIDI, instruments, and modulation projection

Status: **core note-input and instrument-shape foundations implemented; adapters/output/expression remain.**

Implemented:

- owned-capable event-port keys/schema and runtime authority;
- input/output directions and semantic dialect declarations;
- dense activation-local event-port lookup;
- note IDs/channels/keys and wildcard-aware addressing;
- semantic note on/off/choke/end events;
- bounded borrowed note-event validation and process exposure;
- validation against runtime event-port direction/dialect capabilities;
- zero-audio event-processor fixture;
- event-driven instrument with output-only stereo audio;
- multi-output runtime fixture.

Remaining:

- CLAP note-port projection and note input translation preserving source order;
- note tuning and expression dimensions;
- raw MIDI 1/SysEx;
- MIDI 2/UMP representation without destructive down-conversion;
- deployment projection of the Slice 1 modulation contract;
- bounded output event sink and note-end output;
- typed span/change-boundary processing without inventing one cross-family total order;
- high-event-count allocation/work evidence;
- event-transform and fuller synth fixtures through actual deployment adapters.

Implementing a non-effect CLAP fixture here is the right time to replace/generalize `ClapStereoEffect` and any remaining effect-shaped adapter facade.

## Slice 4 — common DSP foundation

Build common audio/DSP utilities from repeated requirements, not from a feature checklist.

Initial likely targets:

- dB/gain/units;
- ramps/smoothing/interpolation;
- delay/ring/scratch buffers;
- metering/basic analysis;
- filters and common channel/buffer operations;
- FFT/STFT/window integration;
- oversampling/resampling/convolution integrations.

Audit and use strong existing Rust dependencies where appropriate. Chassis owns stable audio semantics and ergonomics, not every low-level algorithm implementation.

Do not decide crate boundaries before dependency/optionality pressure requires them.

## Slice 5 — editor/UI integration

Define and implement framework-level editor integration:

1. product-originated begin/change/end gestures with echo suppression;
2. parameter observation/binding;
3. telemetry observation;
4. parent attach/detach and editor recreation;
5. resize, scale, high-DPI, focus/input, and destruction semantics;
6. accessibility path where practical;
7. optional toolkit adapters without pulling GUI dependencies into headless users.

Validation fixture: `EditorDemo`.

## Slice 6 — plugin deployment parity

Project the same semantic component model through CLAP, VST3, and Audio Unit.

Before claiming support for each format prove:

- identity/metadata;
- parameters/automation/modulation;
- state round-trip/migration;
- audio/event layouts;
- latency/PDC/tail/bypass;
- editor lifecycle/resize/scaling;
- real-host save/reopen/render scenarios;
- differential behavior where semantics overlap.

Use a reviewed wrapper while faithful; own native adapters only for concrete material limitations.

## Slice 7 — devices and standalone

Add reusable application/device infrastructure:

- audio device enumeration/open/close/reconfiguration;
- audio input/output/duplex and sample-rate/block-size negotiation;
- MIDI device input/output and timestamp integration where available;
- xrun/device-loss/error/recovery semantics;
- standalone bootstrap using the same component/runtime/editor/state model.

Use reviewed platform/device dependencies behind Chassis semantics rather than building OS backends without cause.

## Slice 8 — graph/routing/scheduling

Implement application-layer graph infrastructure:

- ordinary Chassis processors as nodes;
- audio/event fan-in/fan-out and bus routing;
- validated immutable/transactional execution plans;
- latency propagation/compensation;
- bounded realtime-safe execution;
- topology/resource changes away from the callback;
- deterministic offline graph execution;
- parallel scheduling only when measured workloads justify it.

Do not put tracks, clips, arrangements, project state, or mixer UX into the graph layer.

## Slice 9 — plugin hosting

Add hosting infrastructure integrated with the graph/runtime:

- discovery/scanning;
- loading/instantiation/lifecycle;
- audio/event processing;
- parameters/state/automation;
- editor hosting;
- failure/isolation policy at the supported level;
- format adapters without leaking plugin ABI types into core.

## Slice 10 — media, transport, and offline application infrastructure

Add reusable infrastructure needed by audio applications:

- audio source/sink metadata and streaming/seek/write abstractions;
- reviewed codec integrations;
- resampling/channel adaptation integrations;
- deterministic component/graph offline rendering;
- reusable transport/time primitives to drive processors/graphs;
- random-access/multi-pass processing only when sequential block processing proves insufficient.

Project media management, clip editing, arrangements, and DAW workflows remain application concerns.

## Slice 11 — tooling and stability

Before a stable public release:

- curated author-facing facade;
- canonical identity/export manifest;
- generated backend metadata;
- packaging/signing/notarization hooks;
- reproducible builds/versioning;
- validator/host/device smoke commands;
- compatibility fixtures for state/identity;
- performance/regression harnesses;
- examples/templates spanning plugins, instruments, standalone, graph, host, worker, and offline render;
- complete API/reference documentation with realtime/lifecycle guarantees;
- explicit stability policy and semver/MSRV/state policy.

Do not freeze the entire public API or CHSS compatibility merely because the schema/runtime ownership layer is now a candidate-stable architecture. Freeze each supported surface after the actual outer layers exercise it.

## Existing validation retained

Keep useful existing evidence:

- CLAP validator and bounded fuzz matrix;
- in-process Clack host tests;
- callback allocation/deallocation probes;
- adapter benchmarks;
- REAPER automation/state/PDC baseline;
- Tonal EQ parameter-scale/automation/state/latency/differential tests where they prove framework behavior;
- delayed probe for latency/PDC;
- meter telemetry fixture;
- semantic note/event runtime fixtures;
- instrument output-only fixture;
- multi-output dense-routing fixture;
- layout/sidechain reactivation + policy fixture;
- runtime-owned dynamic metadata fixture.

Tonal EQ product redesign or parity beyond framework evidence is not a Chassis milestone.

## Processor restart requests

A processor whose observed controls require different activation resources sets `Processor::restart_requested()`. It preserves the active resources and latency until the environment deactivates/reactivates it. Deployment/application layers translate that semantic request into the appropriate host/runtime mechanism.

A request does not guarantee immediate reactivation; processing must remain valid with the current resources until the lifecycle transition actually occurs.
