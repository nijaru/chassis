# Roadmap

This roadmap is ordered by architectural proof and real-client pressure, not feature count.

## Current position

Chassis is in **Phase 2: native CLAP qualification**.

The format-independent runtime/state/parameter architecture, generic process-buffer source, mapped f32/f64 CLAP audio path, render-mode projection, typed parameter/state projection, audible exported automation, activation-scoped latency, active-save publication, and adapter realtime/lifecycle semantics are implemented.

Current head is qualified through the automated Linux/macOS/Windows Rust and CLAP headless matrix. Production DAW behavior and representative hardware performance remain separate gates.

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

- representative performance measurements where performance claims are made;
- semantic whole-I/O policy for products that accept multiple legal configurations when a real product requires it;
- ergonomics/conveniences only when real clients demonstrate recurring needs.

## Phase 2 — native CLAP

Status: current implementation is qualified on the three-platform headless validator + in-process host matrix; production DAW coverage remains incomplete.

Implemented and executable:

- Clack-backed CLAP adapter isolated from core;
- stable explicit audio-port and parameter IDs;
- setup-owned mapped process slots;
- arbitrary mapped f32/f64 mono/stereo ports supported by current core layout semantics;
- exact alias, separate, input-only, output-only/asymmetric paths;
- render/offline mode projection;
- bounded parameter-event normalization;
- float/integer/boolean/choice parameter projection;
- exported `trim` automation driving deterministic f32/f64 DSP at sample offsets;
- generation-checked scalar publication with direct concurrency and Loom evidence;
- coherent active state save through the CLAP state extension while processing is active;
- CHSS state save/load and host value rescan;
- CLAP latency extension backed by activation-scoped core latency;
- deliberate nonzero-latency probe whose reported latency matches its actual delayed impulse;
- full adapter-path allocation/deallocation probe at the configured maximum event count for f32/f64 and main + sidechain topology;
- CLAP minimum/maximum frame-count processing and over-bound rejection;
- result-based product activation failure + retry on the same instance;
- activation panic containment + same-instance retry;
- process panic containment as host-visible processing failure;
- repeated inactive instance construction/destruction;
- sample-rate/block-size reactivation;
- audio-thread transfer without simultaneous processor mutation;
- plugin -> host -> plugin main-thread re-entry through parameter-rescan and latency-change callbacks;
- full workspace tests/checks on Linux, macOS, and Windows;
- dependency advisory/license/source/dead-dependency policy gates;
- packaged Linux/macOS/Windows CLAP artifacts passing pinned `clap-validator` 0.4.1 and bounded fuzz;
- validator suite: 35 passed, 0 failed, 9 intentional skips.

Remaining, in order:

1. ~~Measure adapter-only overhead on representative stable hardware with the existing benchmark harness.~~ Complete (2026-09-03): recorded in `docs/design/validation.md`; median 49–53 ns/callback no-event, 477–499 ns at 64 events, flat across frames/precision; no material adapter cost, no optimization warranted. (One-time fix: the bench target needed `harness = false` to actually execute.)
2. ~~Refresh real-DAW current-head evidence when a suitable host is available~~ Complete (2026-09-06): REAPER 7.79/macOS-arm64 (M3 Max, 48 kHz) headless evidence recorded for scan/instantiate, sample-exact automated `trim` render, state save/reload render, active-save during automation, and PDC alignment with the exported delayed probe (`docs/design/validation.md` host matrix).
3. ~~Exercise native host f64 dispatch~~ Not a DAW-qualification gap: f64 semantics are implemented and qualified through the in-process host matrix and three-platform validator (both double-precision process cases pass); no available host selects `data64` (REAPER chooses f32). Wire-precision preference (`PREFERS_64BITS`) moves to first-client pressure: the first real client decides from its product semantics.
4. Add further host/architecture coverage when release targets make it useful; Bitwig remains valuable real-world re-entrancy coverage even though re-entrant semantics are now executable in-process.
5. Start the first real FX client and implement only capabilities its product semantics require.

The 2026-09-06 REAPER 7.79/macOS-arm64 result is current-head evidence for scan/instantiate, automation render, state round-trip, active-save, and PDC alignment. Native f64 wire dispatch moves to first-client pressure rather than blocking Phase 2.

## Phase 3 — first real FX client

The first real client is the **Tonal EQ** port from Truce `audio-plugins`, opted in 2026-09-06 (`audio-plugins` `docs/migration.md`, commit `92c2b52`). Port order is fixed on the client side: DSP parity against the frozen JUCE four-band oracle (`plugins-juce/eq`, 28 parameters) first, then five-slot product changes as separate verified changes. Do not duplicate or silently migrate Truce; the port moves only as the recorded opt-in directs.

The EQ is the right first client for the current framework surface: zero-latency static-coefficient DSP, parameters/state/automation/latency already qualified, and no lookahead/oversampling/telemetry requirements. It pressures real scale the conformance artifact does not: a 28-parameter surface and per-band shape choice params. Chassis adds surface only where the port demonstrates recurring need.

A mastering-limiter class of client (Truce Invisibull) is the intended second client. When it opts in, it should pressure-test:

- production lookahead resources whose declared latency matches delayed audio;
- latency changes across activation/restart rather than live drift;
- offline/realtime parity;
- real sample-accurate automation and explicit smoothing policy;
- state/preset behavior under product use;
- metering/telemetry publication;
- activation-time lookahead/oversampling resources;
- deterministic render/host reopen tests.

Graduate only repeated or clearly framework-owned behavior into Chassis.

A compressor/dynamics client follows when it adds distinct pressure: sidechain/routing/metering.

## Phase 4 — desktop formats and editor lifecycle

After CLAP semantics are stable under a real client:

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
