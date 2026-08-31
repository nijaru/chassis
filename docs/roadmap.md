# Roadmap

This roadmap is ordered by architectural proof, not feature count or release promises. A feature advances when its owner/lifecycle is clear and a conformance or real-product requirement can validate it.

## Current position

Chassis currently spans **late Phase 1 / early Phase 2** deliberately.

The format-independent lifecycle/buffer conformance path, typed parameter/state model, adjacent-schema migrations, complete semantic instance-state ownership, borrowed automation/context, and generic no-allocation process-buffer source are implemented. The whole-state path is locally qualified through `62b96cc`; the generic process-source boundary and arbitrary mapped f32 CLAP audio path are locally Rust-1.98-qualified through `dfc137c`.

The CLAP adapter now owns explicit stable audio/parameter identity mapping, a weak-memory-model-qualified scalar publication bridge, setup-built process slots, and allocation-free lazy projection of arbitrary mapped f32/f64 audio ports. The expanded I/O topology has not yet repeated the earlier native `clap-validator`/REAPER host qualification, so source qualification and native-host qualification remain distinct evidence levels.

CLAP render/offline mode projection and optional f64 capability advertisement/dispatch are implemented and locally qualified. Active-state consistency, richer capabilities, and native requalification remain ahead. This is not a roadmap reversal: core semantic contracts continue to be proven before adapter conveniences are frozen.

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

Status: lifecycle/buffer, typed parameter/state/migration, automation/context, and generic process-source slices are implemented and locally validated. Remaining Phase 1 work is primarily consistency/host-behavior evidence rather than missing state ownership.

Implemented/validated through the current conformance path:

- explicit `Component`/`Processor`/`Process<S>` lifecycle API;
- safe planar channel views for exact alias, disjoint, input-only, and output-only relationships;
- generic `ProcessBufferSource<S>`/`ProcessChannel<S>` traversal with `ChannelBufferSlice` compatibility;
- a non-flat source conformance test proving process traversal does not require one materialized channel collection;
- deterministic external conformance component;
- activation/reset/deactivation and malformed configuration/callback tests;
- f32/f64 processor capability proof without freezing the architecture to one sample precision;
- typed parameters with stable keys and validated base values;
- bounded deterministic canonical state documents;
- adjacent-schema migrations with a checked-in legacy fixture;
- bounded decode → migrate → validate → transactional apply;
- complete semantic runtime state ownership for parameters plus canonical custom entries;
- adversarial state truncation/resource-limit/mutation coverage and failure atomicity;
- borrowed sample-sorted parameter events and lazy float set/linear trajectories;
- optional block-start transport snapshots.

Still to add through the same conformance path:

- host gesture semantics and automation-to-base-state publication policy;
- active state-save consistency contract and qualification;
- realtime allocation/work-bound instrumentation;
- additional capability evidence where a real adapter/client requires it.

Do not add background executors, generalized analyzers, voice allocation, immersive layouts, or elaborate tooling merely to make the conformance component look complete.

## Phase 2 — first real plugin boundary: CLAP

Status: **the initial native stereo proof is CLAP/REAPER-qualified; the expanded arbitrary mapped-f32 source is locally Rust-qualified and awaits repeated native host qualification**.

Current adapter foundation:

- `chassis-clap` using the reviewed post-fix Clack revision `c5975f9f89f0953b00768680357985d46178078a` (development workspace version 0.2.0);
- Clack types isolated from `chassis-core`;
- `Send` required only at the CLAP processor deployment boundary;
- explicit stable CLAP audio-port and parameter IDs rather than declaration-order-derived identity;
- setup-owned `ClapAudioConfiguration` plus `ClapProcessSlot` mapping from dense CLAP indices to stable Chassis endpoints;
- reciprocal declared in-place partners aligned semantically while unrelated same-index input/output ports remain independent;
- allocation-free `ClapBufferSource` traversal directly over safe Clack `ChannelPair` relationships;
- arbitrary mapped f32/f64 mono/stereo ports, auxiliary buses, asymmetric input/output tails, and input-only/output-only semantics within the current core layout model;
- explicit rejection before product DSP when host channel shape/sample type does not match activation or when unrelated ports are illegally aliased;
- activate/process/reset/deactivate lifecycle mapping;
- bounded scalar parameter automation and state projection through CLAP params/state;
- adapter-local generation-checked scalar publication with Loom-qualified publication/pending handoff;
- exported `examples/clap-conformance` rlib/cdylib probe.

Clack's repository has bumped its development workspace to 0.2.0, but that version is not currently published. Chassis uses the explicitly reviewed, full-SHA post-fix revision above rather than the affected crates.io 0.1.1 release; return to crates.io only after a suitable safety-fixed release is published and audited.

Qualification evidence is intentionally split:

1. the earlier conventional stereo artifact regenerated/reviewed `Cargo.lock`, passed fmt/test/clippy/deny/machete, and built the conformance cdylib in release mode;
2. that artifact was packaged as a platform-correct macOS `.clap`, passed `clap-validator` 0.4.1 (source commit `b2f1d9b79b1d264a5747f46707d72b1aa40a02ef`) lifecycle/buffer stress with 19 passes, 0 failures, and 25 intentional skips plus a five-second two-worker fuzz run, and rendered correctly in REAPER 7.78/macOS-arm64;
3. the expanded generic-source/arbitrary-mapped-f32 implementation passed Rust 1.98 fmt, full workspace tests, strict all-feature/all-target Clippy, and release conformance build at `dfc137c`;
4. the current f64-capable adapter artifact passed `clap-validator` 0.4.1 with 23 passes, 0 failures, and 21 intentional skips, including both double-precision process-audio cases, plus a five-second, two-worker fuzz run without errors;
5. the expanded topology still needs a new native validator/real-host qualification run before inheriting the earlier host evidence.

Current/follow-on Phase 2 work:

- native validator and real-host requalification of the expanded mapped-I/O path;
- targeted native render/offline-mode qualification;
- native host requalification of the f64 capability where a product implements `Process<f64>`;
- richer audio/event buffers and note/MIDI projections when a client requires them;
- choice/modulation/gesture semantics and richer parameter events;
- state save/load while active according to a documented consistency contract;
- latency/tail/status metadata as required by conformance or the first real effect;
- negative-space lifecycle/input tests and applicable Miri/sanitizer evidence.

Bitwig is not installed in the current environment, so it is not part of the present host matrix.

Exit condition: the conformance component behaves reproducibly as a CLAP plugin and survives negative-space lifecycle/state/buffer tests across the supported topology. The existence of `chassis-clap` source alone does not satisfy this condition.

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
