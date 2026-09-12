# Current Execution Plan

This file tracks the next implementation order on `main`. It is not an API compatibility promise.

## Current checkpoint

The format-independent runtime and native CLAP adapter are implemented and qualified enough that the next work should be driven by the complete audio-framework architecture, not by an existing product migration.

Current executable foundation includes:

- `Component -> InstanceRuntime<P> -> Processor` lifecycle/state ownership;
- generic safe audio buffer sources with arbitrary mapped audio ports in current mono/stereo layout semantics;
- f32/f64 processing;
- typed parameters, sample-accurate automation, transactional state, activation-scoped latency, restart signaling, and realtime/buffered/offline process modes;
- `chassis-core::telemetry::F32Telemetry`, concurrency tests, measured callback allocation/deallocation coverage, and a public meter fixture;
- stable event-port schema/identity, runtime-owned event-port validation, dense activation-local indices, wildcard-aware semantic note addressing, bounded note input in `ProcessContext`/`ProcessBlock`, and public runtime fixtures;
- native CLAP deployment through pinned Clack with headless conformance and recorded REAPER evidence.

Existing Tonal EQ, delayed-probe, and conformance coverage remain regressions/evidence. They are not architectural authorities.

The [validation guide](design/validation.md) owns executed evidence. The high-level framework completion bar lives in [roadmap.md](roadmap.md).

## Execution principle

Chassis is unpublished pre-alpha. Refactor aggressively when existing implementation choices encode temporary proof limits, effect/plugin assumptions, duplicated authority, or weak ownership semantics.

Do not rewrite working code merely because it is old. Preserve implementations whose semantics already fit the target architecture and whose tests provide useful evidence.

Use small purpose-built fixtures to prove framework behavior. Product-specific DSP, synth engines, restoration/ML algorithms, DAW document models, timelines, arrangements, mixer UX, and product workflows stay outside Chassis.

## Slice 0 — semantic audit and proof-era cleanup

Status: **immediate.**

Audit the existing public surface against `docs/architecture.md` before adding more layers.

Primary targets:

1. remove the assumption that a core `Component` is inherently a conventional stereo effect;
2. retain convenient conventional effect helpers/constants above neutral core semantics;
3. identify/rename temporary adapter concepts such as `ClapStereoEffect` whose names or APIs encode the first proof target rather than actual format capability;
4. ensure event/audio/parameter schemas have one durable authority and dense indices remain derived process-time identities;
5. remove duplicated product/state identity authority from deployment adapters as the canonical identity/tooling path becomes executable;
6. delete compatibility shims that exist only for unpublished intermediate APIs;
7. update examples/tests to use the intended author-facing shape rather than relying on accidental defaults.

Gate every refactor with workspace fmt/test/Clippy, realtime allocation evidence where relevant, CLAP conformance, and existing behavioral fixtures.

## Slice 1 — core authoring/API closure

After the semantic audit, close the remaining cross-cutting pre-v1 core gaps:

- ergonomic `Component` -> `InstanceRuntime` construction while preserving explicit validation;
- semantic whole-I/O configuration for multiple legal layouts;
- recurring bus/port access ergonomics without hiding buffer legality;
- canonical product/state/export identity ownership independent of adapters;
- parameter formatting/value mapping with explicit round-trip/domain tests;
- sample-accurate modulation distinct from durable/base state;
- common tail/bypass semantics where environments can map them faithfully;
- explicit process/application capability queries only where semantics genuinely vary by environment.

Do not add proc macros or derives until these contracts settle.

Validation fixtures: gain, delay, sidechain, zero-audio event processor, and layout-switching processor.

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

## Slice 3 — events, MIDI, instruments, and modulation

Status: **core note-input foundation implemented; adapters/output/expression remain.**

Implemented:

- stable event-port keys/schema and runtime authority;
- input/output directions and semantic dialect declarations;
- dense activation-local event-port lookup;
- note IDs/channels/keys and wildcard-aware addressing;
- semantic note on/off/choke/end events;
- bounded borrowed note-event validation and process exposure;
- validation against runtime event-port direction/dialect capabilities;
- zero-audio/public runtime fixtures.

Remaining:

- CLAP note-port projection and note input translation preserving source order;
- note tuning and expression dimensions;
- raw MIDI 1/SysEx;
- MIDI 2/UMP representation without destructive down-conversion;
- sample-accurate parameter modulation;
- bounded output event sink and note-end output;
- typed span/change-boundary processing without inventing one cross-family total order;
- synth, event-transform, and multi-output fixtures.

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
- explicit stability policy and semver/MSRV policy.

Do not freeze public APIs or CHSS compatibility merely because implementation exists. Freeze after the supported surface has been exercised across materially different clients.

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
- semantic note/event runtime fixtures.

Tonal EQ product redesign or parity beyond framework evidence is not a Chassis milestone.

## Processor restart requests

A processor whose observed controls require different activation resources sets `Processor::restart_requested()`. It preserves the active resources and latency until the environment deactivates/reactivates it. Deployment/application layers translate that semantic request into the appropriate host/runtime mechanism.

A request does not guarantee immediate reactivation; processing must remain valid with the current resources until the lifecycle transition actually occurs.
