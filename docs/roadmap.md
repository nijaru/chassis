# Roadmap

This roadmap is ordered by architectural proof, not feature count or release promises. A feature advances when its owner/lifecycle is clear and a conformance or real-product requirement can validate it.

## Current position

Chassis currently spans **late Phase 1 / early Phase 2** deliberately.

The format-independent lifecycle/buffer conformance slice is implemented and was locally validated before the first adapter work. The first narrow `chassis-clap` source slice now exists, but it has **not yet been locally compiled/validated or format-qualified** after adding Clack dependencies.

Parameter/state/event semantics remain intentionally behind the first lifecycle/buffer CLAP proof. This is not a roadmap reversal: exercising the smallest real host boundary first gives better evidence for the core ownership/buffer model before more semantic layers depend on it.

## Phase 0 — semantic foundation

Status: initial foundation complete enough to support executable slices; contracts remain pre-alpha and can change.

- Maintain Edition 2024 / `stable` Rust baseline without prematurely freezing an MSRV.
- Define deliberate module/public-API boundaries in `chassis-core`.
- Establish stable product/parameter/port identity versus derived backend/runtime indices.
- Establish Processor / framework instance runtime / optional MainThread / Shared ownership.
- Define whole-component I/O configuration, mono/stereo/discrete layouts, and default stereo-effect policy.
- Define process scheduling modes and activation bounds without encoding one plugin format.
- Define safe process-buffer aliasing rules before any FFI adapter constructs Rust references.
- Define parameter base-state authority, process trajectories, and transactional state replacement semantics at the design level.
- Keep licensing/dependency policy compatible with AGPL + commercial dual licensing.
- Use local explicit validation commands; no hosted CI is currently authoritative.

The initial core no longer encodes a CLAP dependency and remains usable outside plugin deployment.

## Phase 1 — conformance runtime without format complexity

Status: lifecycle/buffer slice implemented and validated; parameter/state/event portions remain future work.

Implemented/validated before CLAP work:

- explicit `Component`/`Processor`/`Process<S>` lifecycle API;
- default stereo effect + optional sidechain structural configuration;
- safe planar process-buffer views for exact alias, disjoint, input-only, and output-only relationships;
- deterministic external conformance component;
- activation/reset/deactivation and malformed configuration/callback tests;
- f32 processor proof without freezing the architecture to one sample precision.

Still to add through the same conformance path:

- typed parameters with stable keys;
- block/sample-time automation trajectories;
- minimal transport/process context;
- canonical state document + one migration fixture;
- state replacement tests;
- realtime allocation/work-bound instrumentation.

Do not add background executors, generalized analyzers, voice allocation, immersive layouts, or elaborate tooling merely to make the conformance component look complete.

## Phase 2 — first real plugin boundary: CLAP

Status: **initial source proof implemented; local validation and CLAP qualification are next**.

Current narrow slice:

- `chassis-clap` using exact Clack 0.2.0 dependencies;
- Clack types isolated from `chassis-core`;
- `Send` required only at the CLAP processor deployment boundary;
- one f32 stereo main input/output pair;
- exact in-place/disjoint Clack `ChannelPair` -> Chassis `ChannelBuffer` translation using safe Rust on the Chassis side;
- CLAP factory/main-thread construction -> Chassis component;
- activate/process/reset/deactivate lifecycle mapping;
- exported `examples/clap-conformance` rlib/cdylib probe;
- explicit rejection of unsupported port/sample configurations rather than silent semantic coercion.

Immediate qualification work:

1. regenerate/review `Cargo.lock` locally and pass fmt/test/clippy/deny/machete;
2. build the conformance cdylib;
3. package an actual platform-correct `.clap` artifact;
4. run current CLAP validator/lifecycle/buffer stress;
5. smoke-test in a real CLAP host;
6. use those results to decide the general multibus/sidechain borrowing model.

Then extend Phase 2 with:

- product/port/parameter identity mapping and frozen adapter fixtures;
- general audio-port/configuration/optional sidechain support;
- f64 capability advertisement/dispatch where useful;
- render/offline mode semantics;
- audio/event buffers;
- parameter automation/modulation and gestures;
- state save/load while active according to a documented consistency contract;
- latency/tail/status metadata as required by the conformance path;
- negative-space lifecycle/input tests and applicable Miri/sanitizer evidence.

Exit condition: the conformance component behaves reproducibly as a CLAP plugin and survives negative-space lifecycle/state/buffer tests. The existence of `chassis-clap` source alone does not satisfy this condition.

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
