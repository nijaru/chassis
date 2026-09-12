# AGENTS.md

## Purpose

Chassis is a convention-first Rust framework for professional realtime audio components. Effects are the first qualified deployment surface, but the framework is intended to support effects, instruments, event processors, standalone deployment, embedded use, and optional host/device/graph layers without making plugin-host or application semantics part of core.

The project priority is now **Chassis itself**. Existing product ports and examples are validation clients, not roadmap authorities. Do not preserve an old product architecture, DSP implementation, or migration sequence merely to keep parity when it does not improve Chassis as a reusable framework.

Read `docs/architecture.md`, `docs/roadmap.md`, `docs/next-runtime-slice.md`, `docs/licensing.md`, and `docs/dependencies.md` before architectural/dependency changes. Design contracts live under `docs/design/`; current source/tests outrank stale prose, and the owning document must be updated when a decision changes.

## Rust and API baseline

- Follow `rust-toolchain.toml`, Edition 2024, workspace lints, and current validation commands.
- Do not pin or raise an MSRV/dependency requirement without an explicit compatibility or reproducibility reason.
- Prefer domain newtypes/enums once values cross into semantic core logic.
- Treat every `pub` item and re-export as deliberate API design. These crates are unpublished pre-alpha: replace weak abstractions rather than carrying compatibility shims.
- Prefer typed library errors; avoid recoverable `.unwrap()`.
- Format-independent crates remain safe Rust. Adapter unsafe code is allowed only where an actual FFI boundary requires it, with explicit invariants and matching evidence.

## Architecture and ownership

- `Component` is immutable schema/capability/factory definition.
- `InstanceRuntime<P>` is the durable format-independent authority for canonical parameter/custom semantic state and active lifecycle coordination.
- `Processor` owns active DSP history and reports activation-established processing latency.
- Active latency is snapshotted for one activation; changed latency requires reactivation/restart at the deployment boundary.
- `MainThread`, `Shared`, editor, and host bridges expose deliberate orchestration/projections; they are not second authorities.
- Deployment adapters add `Send`/thread-transfer constraints only where the host requires them.
- Keep CLAP/VST3/AU/Clack/clap-wrapper/toolkit/platform types out of product-facing core APIs.
- Keep stable product/parameter/port identities independent from Rust names, labels, declaration order, runtime dense indices, and backend IDs.
- Map adapter semantics faithfully or reject unsupported input; never silently change meaning to satisfy a host.
- Effects, instruments, event processors, standalone and embedded deployments should share the same component/runtime model rather than growing parallel frameworks.

## Realtime path

For deterministic callbacks:

- no heap allocation or deallocation after activation;
- no filesystem/network I/O or blocking calls;
- no contended/unbounded locks;
- bound queues, scratch, event counts, retries, and work from activation/product requirements;
- keep mutable DSP state processor-owned;
- defer destruction/reclamation of replaced large objects off the callback;
- preserve sample offsets/order for timed events;
- controlled copies are acceptable when they simplify ownership and measurement does not justify more complexity.

Do not add lock-free structures, cache padding, custom allocators, `no_std`, SIMD, zero-copy machinery, or unsafe code without a concrete framework requirement and evidence. Chassis completion is not permission for speculative infrastructure.

## Parameters, automation, and state

Keep three concepts distinct:

1. durable/base parameter state;
2. host automation trajectory for a block;
3. effective DSP value after modulation/product policy.

Realtime automation endpoint publication must lose to newer control/state generations. State save while active must serialize one coherent completed semantic generation. State load is decode -> migrate -> validate -> transactional publish. Do not automatically smooth explicit host ramps.

## Validation and claims

Separate Rust correctness, format conformance, realtime guarantees, lifecycle correctness, state/identity compatibility, performance, and production host support.

Current automated headless evidence is split into:

- **Rust CI**: fmt, Linux workspace tests, strict Clippy, Loom, macOS/Windows portable tests/checks, and dependency-policy gates;
- **CLAP Conformance**: packaged Linux/macOS/Windows artifacts, pinned `clap-validator` 0.4.1, and bounded fuzz;
- **in-process Clack host tests**: actual Chassis CLAP entry for adapter allocation/work bounds, f32/f64, lifecycle, active state save, latency metadata/DSP consistency, main-thread re-entrancy, and panic containment.

The current validator suite is 35 passed / 0 failed / 9 intentional skips. Do not equate the synthetic host or validator with production DAW qualification. The recorded baseline carries REAPER 7.79/macOS-arm64 evidence (2026-09-06) for scan/instantiate, sample-exact automation render, state round-trip, active-save, and PDC alignment with the exported delayed probe. Native f64 wire dispatch is not a gap — f64 semantics are qualified headlessly; wire-precision preference is a product/deployment policy.

`cargo-deny` and `cargo-machete` are automated policy gates. Record exact evidence; never report an unrun gate as passing.

## Panic boundary

Pinned Clack catches Rust panics at its plugin FFI wrappers. Chassis tests establish that an activation panic becomes activation failure and leaves the instance reusable, while a process panic becomes host-visible processing failure and allows clean stop/deactivation.

Do not promise continued audio processing after an arbitrary product DSP panic. Treat process failure as terminal for that processing run and let the host stop/deactivate/restart according to format/host policy.

## Dependencies and licensing

Chassis is AGPL-3.0-or-later with an intended commercial dual-license path. Dependencies must fit that distribution model and meet stricter realtime/safety review when they touch critical paths.

Clack is pinned to exact revision `c5975f9f89f0953b00768680357985d46178078a`, also the `v0.2` prerelease commit containing the required re-entrancy fixes. As of 2026-09-04, crates.io still exposes 0.1.1 as the latest registry release. Any Clack source/revision change is an audit + conformance event.

CI actions should be exact-version/commit pinned when practical. `actions/checkout` is currently v7.0.1 commit `3d3c42e5aac5ba805825da76410c181273ba90b1`.

## Current priority

Finish Chassis into a broadly usable professional audio framework. Work in the ordered completion slices in `docs/next-runtime-slice.md`; the high-level completion bar lives in `docs/roadmap.md`.

Immediate priorities are:

1. close the remaining core authoring/API gaps that affect multiple classes of audio component;
2. add reusable bounded realtime communication primitives for control snapshots and DSP -> UI telemetry;
3. implement the already-designed event/note/MIDI/modulation surface and prove it with small conformance fixtures;
4. establish a production editor contract with gestures, observation, telemetry, attach/detach/recreation, sizing/scaling, and clear toolkit boundaries;
5. qualify VST3 and Audio Unit against the same semantic contract as native CLAP;
6. add standalone/device deployment using the same component/runtime implementation;
7. add graph/scheduling infrastructure only at the application layer, without moving application semantics into core;
8. finish packaging, signing, notarization, validation, examples, and release-oriented tooling so new audio projects can use Chassis without bespoke infrastructure.

Tonal EQ, the delayed probe, conformance plugins, and future limiter/restoration/instrument clients are evidence sources. They do not get to block framework work solely for parity with an old implementation. Preserve useful differential tests where they prove Chassis semantics; discard migration constraints that do not.

Do not duplicate or silently migrate Truce `audio-plugins` products into Chassis. Product work remains explicit and separate.
