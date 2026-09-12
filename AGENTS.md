# AGENTS.md

## Purpose

Chassis is a Rust framework for building audio software. Its common authoring path is expected to be plugins, but the format-independent model must also support effects, instruments, event processors, standalone applications, plugin hosts, audio engines/DAWs, offline processing, and embedded use without making plugin-host or application semantics part of core.

Chassis owns broadly reusable audio infrastructure. Applications own product, document, workflow, and visual semantics. A DAW may use Chassis for processing, devices, graph/scheduling, hosted plugins, transport/audio-engine primitives, media/offline integration, and rendering; Chassis does not own tracks, clips, arrangements, project UX, editing workflows, or other DAW-specific document models.

The project priority is **Chassis itself**. Existing product ports and examples are validation clients, not roadmap authorities. Do not preserve an old product architecture, DSP implementation, API shape, or migration sequence merely for compatibility while the project is unpublished pre-alpha. Replace weak abstractions aggressively when the replacement is simpler, safer, more general, or better aligned with the framework's long-term scope.

Read `docs/architecture.md`, `docs/roadmap.md`, `docs/next-runtime-slice.md`, `docs/licensing.md`, and `docs/dependencies.md` before architectural/dependency changes. Design contracts live under `docs/design/`; current source/tests outrank stale prose, and the owning document must be updated when a decision changes.

## Scope rule

In scope when broadly reusable across audio software:

- component/processor lifecycle and realtime/offline processing;
- audio/event ports, layouts, parameters, automation, modulation, state, transport views, latency/tail/bypass;
- common realtime communication and background-work lifecycle;
- common DSP utilities and integrations where they remove repeated audio-specific plumbing;
- plugin deployment and hosting;
- editor/host integration without owning product visual design;
- audio/MIDI devices and standalone deployment;
- processing graph, routing, scheduling, and latency compensation;
- media/audio-file and offline-render infrastructure using suitable existing codec/resampling libraries where practical;
- validation, packaging, identity, compatibility, and release tooling.

Out of scope by default:

- product-specific processors, instruments, restoration/ML algorithms, or visual identity;
- DAW/application document models such as tracks, clips, arrangements, session views, project workflows, undo models, and mixer UX;
- sample/content libraries, marketplaces, project media management, or product business logic;
- reimplementing mature codecs, FFTs, resamplers, device backends, GUI toolkits, or ML runtimes merely so Chassis owns them.

Package boundaries are implementation decisions, not domain declarations. Add crates only for real dependency, safety, optionality, test, or release boundaries.

## Rust and API baseline

- Follow `rust-toolchain.toml`, Edition 2024, workspace lints, and current validation commands.
- Do not pin or raise an MSRV/dependency requirement without an explicit compatibility or reproducibility reason.
- Prefer domain newtypes/enums once values cross into semantic core logic.
- Treat every `pub` item and re-export as deliberate API design. These crates are unpublished pre-alpha: replace weak abstractions rather than carrying compatibility shims.
- Prefer typed library errors; avoid recoverable `.unwrap()`.
- Format-independent crates remain safe Rust. Adapter unsafe code is allowed only where an actual FFI boundary requires it, with explicit invariants and matching evidence.

## Architecture and ownership

- `Component` is immutable schema/capability/factory definition, not a synonym for plugin.
- `ComponentSchema` is the coherent immutable authority for semantic state identity, audio ports, whole-audio-I/O policy, event ports, parameters, and stable -> dense lookup.
- `InstanceRuntime<P>` is the durable format-independent authority for canonical parameter/custom semantic state and active lifecycle coordination.
- `Processor` owns active DSP history and reports activation-established processing latency.
- Active latency is snapshotted for one activation; changed latency requires reactivation/restart at the deployment boundary.
- Stable audio/event/parameter identities are setup/persistence identities; callback-facing audio/event/parameter identities are dense schema-local indices.
- Audio/event schema metadata may be borrowed static data or runtime-owned metadata; owned strings must not become callback identity.
- `AudioIoPolicy` is schema-owned whole-component configuration authority. Adapters must not infer independent cross-bus policy.
- `MainThread`, `Shared`, editor, device, graph, and host bridges expose deliberate orchestration/projections; they are not second authorities.
- Deployment adapters add `Send`/thread-transfer constraints only where the environment requires them.
- Keep CLAP/VST3/AU/Clack/clap-wrapper/toolkit/device-backend/platform types out of product-facing core APIs.
- Keep stable product/parameter/port identities independent from Rust names, labels, declaration order, runtime dense indices, and backend IDs.
- Map adapter semantics faithfully or reject unsupported input; never silently change meaning to satisfy a host.
- Effects, instruments, event processors, plugins, standalone applications, graph nodes, embedded processors, and offline processing share the same component/runtime model rather than parallel frameworks.
- Graph, device, host, media, and deployment layers sit above the semantic core and must not move application-specific concepts downward.
- The current `ComponentSchema` / `InstanceRuntime` ownership shape has passed heterogeneous effect/event/instrument/multi-output/layout/dynamic-metadata proofs. Treat it as candidate-stable architecture; do not reopen it for speculative generality without a concrete client that exposes a real mismatch.

## Realtime path

For deterministic callbacks:

- no heap allocation or deallocation after activation;
- no filesystem/network I/O or blocking calls;
- no contended/unbounded locks;
- bound queues, scratch, event counts, retries, and work from activation/component requirements;
- keep mutable DSP state processor-owned;
- defer destruction/reclamation of replaced large objects off the callback;
- preserve sample offsets/order for timed events;
- controlled copies are acceptable when they simplify ownership and measurement does not justify more complexity.

Do not add lock-free structures, cache padding, custom allocators, `no_std`, SIMD, zero-copy machinery, or unsafe code without a concrete framework requirement and evidence. Chassis completion is not permission for speculative infrastructure.

## DSP and dependencies

Chassis should provide a coherent audio/DSP authoring surface, but coherence does not require implementing every primitive internally.

- Prefer strong existing crates for FFTs, resampling, codecs, platform device access, and similar mature low-level facilities when their semantics, maintenance, safety, performance, and licensing fit.
- Wrap or integrate dependencies where Chassis needs stable audio-specific semantics, ownership, lifecycle, or ergonomics.
- Put common DSP building blocks in Chassis when they are broadly useful and their API can be specified independently of one product.
- Keep specialized/novel algorithms in clients until repeated use demonstrates a reusable abstraction.
- Do not force `no_std` or embedded-microcontroller constraints onto the main framework unless a concrete supported target requires them.

## Parameters, automation, modulation, and state

Keep four concepts distinct:

1. durable/base parameter state;
2. host/application automation trajectory for a block;
3. sample-accurate modulation;
4. effective DSP value after product policy.

The first two are implemented. Do not implement modulation by mutating durable/base state or pretending modulation is ordinary automation if a deployment distinguishes them.

Realtime automation endpoint publication must lose to newer control/state generations. State save while active must serialize one coherent completed semantic generation. Runtime state identity/version comes from `ComponentSchema`; do not reintroduce explicit product-ID state authority at `InstanceRuntime`. State load is decode -> migrate -> validate -> transactional publish. Do not automatically smooth explicit host ramps.

## Validation and claims

Separate Rust correctness, format conformance, realtime guarantees, lifecycle correctness, state/identity compatibility, performance, device/graph correctness, and production-host support.

Current automated headless evidence is split into:

- **Rust CI**: fmt, Linux workspace tests, strict Clippy, Loom, macOS/Windows portable tests/checks, and dependency-policy gates;
- **CLAP Conformance**: packaged Linux/macOS/Windows artifacts, pinned `clap-validator` 0.4.1, and bounded fuzz;
- **in-process Clack host tests**: actual Chassis CLAP entry for adapter allocation/work bounds, f32/f64, lifecycle, active state save, latency metadata/DSP consistency, main-thread re-entrancy, and panic containment.

The recorded validator baseline is 35 passed / 0 failed / 9 intentional skips. Do not equate the synthetic host or validator with production DAW qualification. The recorded baseline carries REAPER 7.79/macOS-arm64 evidence (2026-09-06) for scan/instantiate, sample-exact automation render, state round-trip, active-save, and PDC alignment with the exported delayed probe. Native f64 wire dispatch is not a gap — f64 semantics are qualified headlessly; wire-precision preference is a deployment policy.

`cargo-deny` and `cargo-machete` are automated policy gates. Record exact evidence; never report an unrun gate as passing.

## Panic boundary

Pinned Clack catches Rust panics at its plugin FFI wrappers. Chassis tests establish that an activation panic becomes activation failure and leaves the instance reusable, while a process panic becomes host-visible processing failure and allows clean stop/deactivation.

Do not promise continued audio processing after an arbitrary product DSP panic. Treat process failure as terminal for that processing run and let the host/application stop/deactivate/restart according to its policy.

## Dependencies and licensing

Chassis is AGPL-3.0-or-later with an intended commercial dual-license path. Dependencies must fit that distribution model and meet stricter realtime/safety review when they touch critical paths.

Clack is pinned to exact revision `c5975f9f89f0953b00768680357985d46178078a`, also the `v0.2` prerelease commit containing the required re-entrancy fixes. As of 2026-09-04, crates.io still exposes 0.1.1 as the latest registry release. Any Clack source/revision change is an audit + conformance event.

CI actions should be exact-version/commit pinned when practical. `actions/checkout` is currently v7.0.1 commit `3d3c42e5aac5ba805825da76410c181273ba90b1`.

## Current priority

Finish Chassis into a stable, broadly useful Rust audio framework. Work in the ordered completion slices in `docs/next-runtime-slice.md`; the high-level completion bar lives in `docs/roadmap.md`.

Immediate priorities are:

1. close the remaining core authoring contracts: parameter formatting/value mapping, sample-accurate modulation, tail, and bypass;
2. complete bounded realtime communication/background-work/reclamation primitives;
3. finish event/note/MIDI/expression input/output and deployment projection, using a real non-effect CLAP fixture to generalize effect-shaped adapter surfaces when needed;
4. establish a coherent common DSP utility layer without duplicating mature dependencies unnecessarily;
5. establish a production editor contract and toolkit adapters;
6. qualify CLAP, VST3, and Audio Unit against the same semantic contract;
7. add standalone/device deployment;
8. add graph/scheduling and plugin-hosting infrastructure for larger audio applications;
9. add media/offline-render integration needed by general audio applications;
10. finish packaging, validation, compatibility, examples, documentation, and release tooling;
11. only then freeze the supported public surface and make stability/compatibility promises.

Do not spend a new slice inventing richer component schema, I/O-policy, metadata, or constructor abstractions merely because the core is pre-alpha. The current heterogeneous fixtures are evidence that the explicit model is sufficient until a later real client proves otherwise.

Tonal EQ, the delayed probe, conformance plugins, and future limiter/restoration/instrument/DAW clients are evidence sources. They do not get to block framework work solely for parity with an old implementation. Preserve useful differential tests where they prove Chassis semantics; discard migration constraints that do not.

Do not duplicate or silently migrate separate audio-product repositories into Chassis. Product work remains explicit and separate.
