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
- Active latency is snapshotted by `InstanceRuntime` for the lifetime of one activation; a changed latency requires reactivation/restart at the deployment boundary.
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

GitHub Actions is a portable Rust regression signal only. It does not establish native plugin support. Current native validator/REAPER evidence predates the latest audible-automation and CLAP-latency changes, so current head requires native requalification before those results can be promoted again.

Baseline checks:

```text
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-features --all-targets --locked -- -D warnings
cargo deny check
cargo machete
```

Use Loom for subtle atomics, Miri for modelable unsafe Rust, sanitizers/native stress for FFI, native format validators, deterministic host renders, fuzzing, and representative benchmarks where those claims exist. Record exact evidence; never report an unrun gate as passing.

## Dependencies and licensing

Chassis is AGPL-3.0-or-later with an intended commercial dual-license path. Dependencies must be compatible with that distribution model and meet stricter realtime/safety review when they touch critical paths.

Clack is the current explicit git-source exception at revision `c5975f9f89f0953b00768680357985d46178078a`. Do not downgrade to the affected published 0.1.1 release or advance the revision automatically. Any Clack change is an audit + conformance event.

## Current priority

1. Keep repository docs and agent instructions synchronized with the current Phase-2 checkpoint.
2. Rebuild and natively requalify current CLAP head: validator, bounded fuzz, REAPER scan/instantiate/state, an automated `trim` render against deterministic expected samples, and a reproducible active-save-during-automation scenario.
3. Preserve the distinction between current zero-latency conformance coverage and actual nonzero PDC qualification; exercise nonzero latency with the first real delayed client or a deliberate delayed conformance case.
4. Extend realtime evidence beyond the existing post-activation core allocation test: representative mapped adapter topology, maximum configured event load, frame-count extremes, and adapter-only overhead measurements.
5. Close remaining deployment negative space that real hosts can expose, especially reentrant callbacks and repeated host lifecycle transitions; core activation-failure recovery is already covered.
6. Start the first real FX client (mastering limiter) and let it pressure lookahead latency, activation-time resources, offline parity, smoothing, telemetry, state, and deterministic rendering.
7. Add product-originated gesture/edit and other editor-facing CLAP capabilities when that client needs them; defer note/MIDI until an instrument/event client exists.
8. Then qualify VST3/AU projection and editor lifecycle through the same product semantics and differential tests.

Do not create empty crates or speculative framework abstractions merely to make the roadmap look complete.
