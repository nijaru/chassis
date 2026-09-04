# Roadmap

This roadmap is ordered by architectural proof and real-client pressure, not feature count.

## Current position

Chassis is in **Phase 2: native CLAP qualification**.

The format-independent runtime/state/parameter architecture, generic process-buffer source, mapped f32/f64 CLAP audio path, render-mode projection, typed parameter/state projection, audible exported automation, and activation-scoped latency are implemented. Current Rust CI is green through the CLAP latency projection.

The last recorded native validator/REAPER baseline predates the current audible-automation and latency changes. It remains useful regression history, but current head must be rebuilt and requalified natively before those results are described as current plugin evidence.

## Phase 0 — semantic foundation

Status: complete enough for executable work; APIs remain pre-alpha.

- format-independent component/processor/runtime ownership;
- stable semantic identity versus backend/runtime indices;
- whole-component I/O representation;
- explicit process modes and activation bounds;
- safe buffer alias relationships;
- parameter/base/automation/state ownership model;
- conservative dependency/licensing policy.

## Phase 1 — format-independent conformance

Status: substantially complete.

Implemented:

- durable `InstanceRuntime<P>` lifecycle/state ownership;
- safe in-place/separate/input-only/output-only buffers;
- generic allocation-free `ProcessBufferSource<S>`;
- f32/f64 processor capability model;
- typed parameters and dense realtime indices;
- deterministic bounded state + adjacent migrations;
- complete semantic transactional state replacement;
- borrowed sample-accurate automation and transport;
- adversarial state corruption/resource-limit coverage;
- activation-failure recovery with the runtime remaining reusable;
- executable post-activation process allocation detection;
- activation-scoped processing latency captured from `Processor`.

Remaining promotion work:

- representative work-bound and adapter-overhead measurements;
- additional lifecycle/failure cases exposed by deployment rather than core construction;
- semantic whole-I/O policy for products that accept multiple legal configurations when a real product requires it.

## Phase 2 — native CLAP

Status: implementation is ahead of the last native qualification checkpoint.

Implemented:

- Clack-backed CLAP adapter isolated from core;
- stable explicit audio-port and parameter IDs;
- setup-owned mapped process slots;
- arbitrary mapped f32/f64 mono/stereo ports supported by current core layout semantics;
- exact alias, separate, input-only, output-only/asymmetric paths;
- render/offline mode projection;
- bounded parameter event normalization;
- float/integer/boolean/choice parameter projection;
- exported `trim` automation driving deterministic f32/f64 DSP at sample offsets;
- generation-checked scalar publication with direct concurrency and Loom evidence;
- coherent completed-generation snapshots supporting active state save;
- CHSS state save/load and host value rescan;
- CLAP latency extension backed by activation-scoped core latency;
- exported deterministic conformance plugin.

Last recorded native baseline before the newest changes:

- `clap-validator` 0.4.1: 35 passed, 0 failed, 9 intentional skips;
- five-second two-worker validator fuzz: clean;
- REAPER 7.79/macOS-arm64: scan, instantiate, two-parameter state round trip, deterministic f32 render;
- native host `data64` dispatch unexercised because REAPER selected f32.

Next, in order:

1. rebuild current head and repeat full validator + bounded fuzz qualification;
2. render actual `trim` automation in REAPER and compare samples against the deterministic trajectory;
3. reproduce state save while automation is active and verify coherent saved endpoints/state;
4. retain f64 validator evidence and exercise native host `data64` when a host can be induced to select it;
5. verify nonzero host PDC behavior when a real delayed client or deliberate delayed conformance case is available;
6. finish reentrancy/repeated-host-lifecycle coverage and representative adapter work-bound/overhead evidence;
7. start the first real FX client and implement only capabilities its product semantics require.

Bitwig remains desirable reentrancy coverage when available; it is not installed in the current validation environment.

## Phase 3 — first real FX client

Start the mastering limiter before broadening the framework speculatively.

The limiter should pressure-test:

- real lookahead resources whose declared latency matches delayed audio;
- latency changes across activation/restart rather than live drift;
- offline/realtime parity;
- real sample-accurate automation;
- explicit smoothing policy;
- state under real product use;
- metering/telemetry publication;
- activation-time lookahead/oversampling resources;
- deterministic render/host reopen tests.

Graduate only repeated or clearly framework-owned behavior into Chassis.

A tonal EQ and compressor/dynamics client follow when they add distinct pressure: many controls/custom GUI for EQ, sidechain/routing/metering for dynamics.

## Phase 4 — desktop formats and editor lifecycle

After the CLAP semantics above are stable under a real client:

- project through a pinned/reviewed `clap-wrapper` release/revision to VST3/AUv2/AUv3;
- run Steinberg validator, `auval`, pluginval where useful, and real-host matrices;
- add differential audio/state/automation/identity/latency tests across formats;
- prove editor attach/detach, resize/scaling, recreation, gestures, and telemetry;
- select the first production GUI adapter without putting toolkit types in core;
- add packaging/signing/notarization hooks.

Replace wrapper projection with a native adapter only when concrete correctness/capability/maintenance evidence warrants ownership.

## Phase 5 — instruments and standalone

Driven by a real instrument/event client:

- note/MIDI/event input/output;
- note identity/expression/MPE where hosts support it;
- no-audio-input generators and multiple outputs;
- standalone runtime using the same processor/state/editor model;
- audio/MIDI device integration;
- high-event-count and voice-driven stress.

Voice allocation, sample streaming, oscillators/filters, etc. become optional utilities only when repeated products establish reusable semantics.

## Phase 6 — advanced routing / immersive

Driven by products that require it:

- labeled surround layouts;
- ambisonics ordering/normalization;
- high-channel-count validation;
- immersive channel beds;
- dynamic layout negotiation where formats allow it.

Dolby Atmos object/metadata/renderer workflows remain a separate capability.

## Phase 7 — optional audio-application runtime

Only with a real host/application client:

- `chassis-host` for plugin discovery/loading/hosting;
- `chassis-device` for audio/MIDI devices;
- `chassis-graph` for realtime graph/scheduling.

Timelines, projects, arrangements, media libraries, mixer UX, mastering workflows, and delivery remain application concerns.

## Later / conditional

- AAX when Avid/PACE access, licensing, and demand justify it;
- LV2 if Linux demand warrants it;
- sandboxed/out-of-process hosting when a host application needs crash isolation;
- additional GUI adapters based on real clients.

## Promotion rule

A feature enters the common framework only when:

1. owner/lifecycle is explicit;
2. resource/work bounds are appropriate;
3. a conformance case or real client demonstrates the need;
4. negative-space behavior is tested;
5. performance claims have representative measurements;
6. dependencies remain compatible with realtime/safety/licensing policy.
