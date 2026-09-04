# AGENTS.md

## Purpose

Chassis is a convention-first Rust framework for professional realtime audio components. Effects are the first production clients. Instruments, standalone deployment, and optional host/device/graph layers remain compatible future directions without making plugin-host or application semantics part of core.

Read `docs/architecture.md`, `docs/roadmap.md`, `docs/next-runtime-slice.md`, `docs/licensing.md`, and `docs/dependencies.md` before architectural/dependency changes. Design contracts live under `docs/design/`; current source/tests outrank stale prose, and the owning document must be updated when a decision changes.

## Rust and API baseline

- Follow `rust-toolchain.toml`, Edition 2024, workspace lints, and current validation commands.
- Do not pin/raise an MSRV or dependency requirement without an explicit compatibility or reproducibility reason.
- Prefer domain newtypes/enums once values cross into semantic core logic.
- Treat every `pub` item and re-export as deliberate API design. These crates are unpublished pre-alpha: replace weak abstractions rather than carrying compatibility shims.
- Prefer typed library errors; avoid recoverable `.unwrap()`.
- Format-independent crates remain safe Rust. Adapter unsafe code is allowed only where an actual FFI boundary requires it, with explicit invariants and matching evidence.

## Architecture and ownership

- `Component` is the immutable schema/capability/factory definition.
- `InstanceRuntime<P>` is the durable format-independent authority for canonical parameter/custom semantic state and active lifecycle coordination.
- `Processor` exclusively owns mutable realtime DSP history while active and reports activation-established processing latency.
- Active latency is snapshotted by `InstanceRuntime` for one activation; changed latency requires reactivation/restart at the deployment boundary.
- `MainThread`, `Shared`, editor, and host bridges expose deliberate orchestration/projections; they are not second authorities.
- Deployment adapters add `Send`/thread-transfer constraints only where the host requires them.
- Keep CLAP/VST3/AU/Clack/clap-wrapper/toolkit/platform types out of product-facing core APIs.
- Keep stable product/parameter/port identities independent from Rust names, labels, declaration order, runtime dense indices, and backend IDs.
- Map adapter semantics faithfully or reject unsupported input; never silently change meaning to satisfy a host.

## Realtime path

For audio callbacks and other deterministic hot paths:

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

The process view is derived. Realtime automation endpoint publication must lose to newer control/state generations rather than overwrite them. State save while active must serialize one coherent completed semantic generation, never a mixed snapshot. State load is decode -> migrate -> validate -> transactional publish; failure leaves live state unchanged.

Do not automatically smooth explicit host ramps. Product smoothing policy stays explicit.

## Validation and claims

Separate language correctness, format conformance, realtime guarantees, lifecycle correctness, state/identity compatibility, performance, and production host support.

Current portable evidence is split into:

- **Rust CI**: fmt, workspace tests, strict Clippy, Loom;
- **CLAP Conformance**: current Linux artifact, pinned `clap-validator` 0.4.1, and bounded fuzz;
- **in-process Clack host tests**: actual Chassis CLAP entry for adapter allocation/work bounds, f32/f64, lifecycle, active state save, and latency metadata/DSP consistency.

Current Linux validator evidence is 35 passed / 0 failed / 9 intentional skips plus clean five-second two-worker fuzzing.

The REAPER 7.79/macOS-arm64 evidence is a historical baseline that predates current head. Do not present the synthetic host or Linux validator as production DAW/platform qualification.

Baseline local checks may additionally include dependency/license/dead-dependency tools when configured and available. Record exact evidence; never report an unrun gate as passing.

## Dependencies and licensing

Chassis is AGPL-3.0-or-later with an intended commercial dual-license path. Dependencies must be compatible with that distribution model and meet stricter realtime/safety review when they touch critical paths.

Clack is the current explicit git-source exception at revision `c5975f9f89f0953b00768680357985d46178078a`. Do not downgrade to the affected published 0.1.1 release or advance the revision automatically. Any Clack change is an audit + conformance event.

## Current priority

1. Keep repository authority docs synchronized with current executable evidence.
2. When a production DAW is available, refresh current-head scan/instantiate/save-reopen, automated `trim` render, active-save-during-automation, PDC alignment, and native precision behavior.
3. Measure adapter-only overhead on representative stable hardware; do not use shared CI timing as production performance evidence.
4. Let the first product that explicitly opts into Chassis drive the next framework surface. A mastering-limiter class of client is the intended pressure case for lookahead resources, offline parity, smoothing, telemetry, state, and deterministic rendering.
5. Do not duplicate or silently migrate the existing Truce `audio-plugins` implementation to Chassis.
6. Add product-originated gesture/edit and meter/telemetry infrastructure when a real Chassis client needs them; defer note/MIDI until an instrument/event client exists.
7. Then qualify VST3/AU projection and editor lifecycle through the same product semantics and differential tests.

Do not create empty crates or speculative framework abstractions merely to make the roadmap look complete.
