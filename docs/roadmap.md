# Roadmap

This roadmap is ordered by architectural proof, not feature count or release promises. A feature advances when its owner/lifecycle is clear and a conformance or real-product requirement can validate it.

## Phase 0 — semantic foundation

Current phase.

- Maintain Edition 2024 / `stable` Rust baseline without prematurely freezing an MSRV.
- Define deliberate module/public-API boundaries in `chassis-core`.
- Establish stable product/parameter/port identity versus derived backend/runtime indices.
- Establish Processor / framework instance runtime / optional MainThread / Shared ownership.
- Define whole-component I/O configuration, mono/stereo/discrete layouts, and default stereo-effect policy.
- Define process scheduling modes and activation bounds without encoding one plugin format.
- Define safe process-buffer aliasing rules before any FFI adapter constructs Rust references.
- Define parameter base-state authority, process trajectories, and transactional state replacement semantics.
- Keep licensing/dependency policy compatible with AGPL + commercial dual licensing.
- Use local explicit validation commands; no hosted CI is currently authoritative.

Exit condition: the small explicit core is locally buildable/testable, its mutable authorities are named, and it does not encode FX-only, stereo-only, plugin-only, or backend-specific assumptions.

## Phase 1 — conformance runtime without format complexity

Build the smallest deterministic component/test harness that proves the author-facing semantics before macros or GUI work.

- explicit `Component`/`Processor` lifecycle API;
- default stereo effect + optional sidechain configuration;
- safe planar process-buffer views;
- a few typed parameters with stable keys;
- block/sample-time automation trajectories;
- minimal transport/process context;
- canonical state document + one migration fixture;
- activation/reset/deactivation and state replacement tests;
- realtime allocation/work-bound checks for framework-owned paths.

Do not add background executors, generalized analyzers, voice allocation, immersive layouts, or elaborate tooling merely to make the conformance component look complete.

Exit condition: the semantic runtime is ergonomic enough that the conformance effect mostly contains product processing rather than framework plumbing.

## Phase 2 — first real plugin boundary: CLAP

- `chassis-clap` using Clack for the low-level safe CLAP boundary.
- product/port/parameter identity mapping and frozen adapter fixtures;
- lifecycle/activation/process translation;
- audio/event buffers and optional sidechain;
- parameter automation/modulation and gestures;
- state save/load while active according to a documented consistency contract;
- fixed latency/tail metadata as needed by the conformance path;
- CLAP validator and synthetic lifecycle/invalid-input tests;
- Miri/model testing for applicable adapter/support code and sanitizers where native FFI requires them.

Exit condition: the conformance component behaves reproducibly as a CLAP plugin and survives negative-space lifecycle/state/buffer tests.

## Phase 3 — desktop formats, editor boundary, and packaging

- VST3 projection through `clap-wrapper` while its semantics pass Chassis tests; replace with native adapter only if evidence warrants it.
- AUv2/AUv3 projection and validation on macOS under the same rule.
- macOS/Windows/Linux packaging where formats apply.
- editor lifecycle, parent window, resize/scaling, and parameter-binding contract.
- evaluate the first production GUI adapter, likely Iced/wgpu, without making the core depend on Iced.
- generic/debug parameter editor for framework testing.
- native validators (`auval`, Steinberg validator, pluginval where useful) plus real-host matrix.
- identity manifests and signing/notarization/package hooks.

Exit condition: the same conformance product has cross-format semantic parity and a stable editor lifecycle on the supported desktop matrix.

## Phase 4 — first commercial-grade FX clients and common conveniences

Use real effects to graduate only repeated framework behavior:

1. mastering limiter — latency, automation, offline parity, realtime safety;
2. tonal EQ — many controls/state plus polished custom GUI;
3. compressor/dynamics — sidechain, metering, routing.

Candidate conveniences that may move into Chassis based on evidence:

- parameter smoothing helpers;
- latest-value meter publication;
- bounded analyzer/scope transport;
- preset serialization/storage primitives;
- typed immutable DSP snapshot publication/reclamation;
- background preparation with a proven module/instance lifecycle;
- reusable bypass helpers where semantics actually converge.

Product-specific DSP remains in the products.

## Phase 5 — product-grade standalone and instruments

The core is instrument-compatible earlier; this phase proves it.

- standalone runtime using the same component/processor/state/editor model;
- audio device and MIDI device integration;
- note/MIDI input as a primary processing source;
- no-audio-input generators;
- multiple audio outputs;
- MIDI/event output;
- note expression/MPE according to host capabilities;
- high-event-count and voice-driven stress tests.

Voice allocation, sample streaming, oscillator/filter libraries, etc. are optional utilities only if repeated products establish a reusable abstraction.

## Phase 6 — advanced routing and immersive audio

- labeled surround layouts;
- ambisonics with explicit ordering/normalization;
- high-channel-count and generalized multi-bus validation;
- channel-based immersive beds such as 7.1.4/9.1.6 where hosts/formats support them;
- dynamic inactive layout negotiation where formats permit it;
- expanded high-channel-count performance evidence.

Dolby Atmos object/metadata/renderer workflows are a separate capability and are added only if a product requires them.

## Phase 7 — optional audio-application runtime

Pursue only with a real host/application client.

Potential layers:

- `chassis-host`: plugin discovery/loading/hosting;
- `chassis-device`: reusable audio/MIDI device ownership/runtime;
- `chassis-graph`: realtime processing graph/scheduling.

These may support live processors, modular hosts, mastering applications, or DAW-like software while leaving timelines, projects, arrangements, media libraries, mixer UX, mastering workflows, and delivery semantics to the application.

## Later / conditional

- AAX once Avid/PACE integration, licensing, and demand justify it.
- LV2 if Linux ecosystem demand warrants it.
- MIDI 2.0 when target hosts and Rust dependencies are production-ready.
- sandboxed/out-of-process hosting if an application/host layer needs crash isolation.
- additional GUI adapters based on actual users/products.

## Promotion rule

A Chassis feature graduates into the common framework only when:

1. its semantic owner/lifecycle is explicit;
2. its resource/work bounds are appropriate for the domain;
3. a conformance or real client demonstrates the need;
4. negative-space tests cover failure/replacement/teardown where relevant;
5. any performance claim has representative measurements;
6. its dependencies remain compatible with the licensing and critical-path policies.
