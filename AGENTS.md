# AGENTS.md

## Purpose

Chassis is a convention-first Rust framework for professional realtime audio components. Effects are the first production clients. Instruments, standalone deployment, and optional host/device/graph layers remain compatible future directions without making plugin-host or application semantics part of core.

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

Do not add lock-free structures, cache padding, custom allocators, `no_std`, SIMD, zero-copy machinery, or unsafe code without a concrete requirement and evidence.

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

The current validator suite is 35 passed / 0 failed / 9 intentional skips. Do not equate the synthetic host or validator with production DAW qualification. Current head carries REAPER 7.79/macOS-arm64 evidence (2026-09-06) for scan/instantiate, sample-exact automation render, state round-trip, active-save, and PDC alignment with the exported delayed probe. Native f64 wire dispatch is not a gap — f64 semantics are qualified headlessly; wire-precision preference moves to first-client pressure.

`cargo-deny` and `cargo-machete` are automated policy gates. Record exact evidence; never report an unrun gate as passing.

## Panic boundary

Pinned Clack catches Rust panics at its plugin FFI wrappers. Chassis tests establish that an activation panic becomes activation failure and leaves the instance reusable, while a process panic becomes host-visible processing failure and allows clean stop/deactivation.

Do not promise continued audio processing after an arbitrary product DSP panic. Treat process failure as terminal for that processing run and let the host stop/deactivate/restart according to format/host policy.

## Dependencies and licensing

Chassis is AGPL-3.0-or-later with an intended commercial dual-license path. Dependencies must fit that distribution model and meet stricter realtime/safety review when they touch critical paths.

Clack is pinned to exact revision `c5975f9f89f0953b00768680357985d46178078a`, also the `v0.2` prerelease commit containing the required re-entrancy fixes. As of 2026-09-04, crates.io still exposes 0.1.1 as the latest registry release. Any Clack source/revision change is an audit + conformance event.

CI actions should be exact-version/commit pinned when practical. `actions/checkout` is currently v7.0.1 commit `3d3c42e5aac5ba805825da76410c181273ba90b1`.

## Current priority

1. Keep repository authority docs synchronized with executable evidence.
2. Run the adapter benchmark on stable representative hardware; do not use shared CI timing as production performance evidence.
3. When a production DAW is available, refresh current-head scan/instantiate/save-reopen, automated `trim` render, active-save-during-automation, PDC alignment, and native precision behavior.
4. Let the first product that explicitly opts into Chassis drive the next framework surface. The Tonal EQ port opted in 2026-09-06 (`audio-plugins` commit `92c2b52`): DSP parity against the frozen JUCE oracle first, five-slot product changes after parity. A mastering-limiter class of client (Invisibull) follows at its own opt-in and is the intended pressure case for lookahead resources, offline parity, smoothing, telemetry, and deterministic rendering.
5. Do not duplicate or silently migrate the existing Truce `audio-plugins` implementation to Chassis.
6. Add product-originated gesture/edit and meter/telemetry infrastructure when a real Chassis client needs them; defer note/MIDI until an instrument/event client exists.
7. Then qualify VST3/AU projection and editor lifecycle through the same product semantics and differential tests.

Do not create speculative framework abstractions merely to fill the roadmap.
