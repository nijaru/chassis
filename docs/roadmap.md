# Roadmap

This roadmap is ordered by architectural proof and real-client pressure, not feature count.

## Current position

Chassis is in **Phase 2: native CLAP qualification**.

The format-independent runtime/state/parameter architecture, generic process-buffer source, mapped f32/f64 CLAP audio path, render-mode projection, and float/integer/boolean/choice parameter/state projection are implemented. The current parametered conformance artifact passes the local Rust gate, `clap-validator` 0.4.1 with 35 passes / 0 failures / 9 intentional skips, bounded fuzzing, and a REAPER 7.79 state round-trip + deterministic render on macOS arm64.

Remaining Phase-1 semantic gates are active-save consistency, complete automation endpoint evidence through the exported artifact, and mechanical realtime allocation/work-bound evidence.

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
- adversarial state corruption/resource-limit coverage.

Remaining promotion work:

- active state-save consistency contract and tests;
- realtime allocation/deallocation instrumentation;
- representative work-bound/adapter-overhead measurements;
- remaining lifecycle/failure negative-space cases exposed by real deployment.

## Phase 2 — native CLAP

Status: current mapped parameter/state/audio artifact is qualified on the available validator/REAPER matrix.

Implemented:

- Clack-backed CLAP adapter isolated from core;
- stable explicit audio-port and parameter IDs;
- setup-owned mapped process slots;
- arbitrary mapped f32/f64 mono/stereo ports supported by current core layout semantics;
- exact alias, separate, input-only, output-only/asymmetric paths;
- render/offline mode projection;
- bounded parameter event normalization;
- float/integer/boolean/choice parameter projection;
- generation-checked scalar publication with Loom evidence;
- CHSS state save/load and host value rescan;
- exported deterministic conformance plugin.

Next, in order:

1. make exported `trim` audibly drive DSP and requalify automation;
2. qualify active save while automation publication is concurrent;
3. finish lifecycle/activation-failure/reentrancy negative-space evidence;
4. add realtime allocation/work-bound instrumentation;
5. start the first real FX client and implement only the CLAP capabilities it actually requires.

Bitwig remains desirable reentrancy coverage when available; it is not installed in the current validation environment.

## Phase 3 — first real FX client

Start the mastering limiter before broadening the framework speculatively.

The limiter should pressure-test:

- latency declaration/change behavior;
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

- project through `clap-wrapper` to VST3/AUv2/AUv3;
- run Steinberg validator, `auval`, pluginval where useful, and real-host matrices;
- add differential audio/state/automation/identity tests across formats;
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
