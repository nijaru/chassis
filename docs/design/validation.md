# Validation and Conformance Strategy

## Goal

Framework correctness must be observable and reproducible. Keep separate evidence for:

- Rust/type correctness;
- format/API conformance;
- realtime allocation/work guarantees;
- lifecycle/ownership correctness;
- state/identity compatibility;
- cross-format semantic parity;
- performance;
- production host qualification.

A stronger claim requires matching evidence. One build, validator pass, or successful DAW load does not imply the others.

## Portable regression gates

GitHub Actions currently provides two distinct portable signals:

### Rust CI

- `cargo fmt --all -- --check`
- `cargo test --workspace --locked`
- `cargo clippy --workspace --all-features --all-targets --locked -- -D warnings`
- Loom publication model

### CLAP Conformance

- build the release conformance artifact on Linux;
- package the `.clap`;
- build `clap-validator` from pinned revision `b2f1d9b79b1d264a5747f46707d72b1aa40a02ef`;
- run the validator;
- run a bounded five-second two-worker fuzz pass.

These are not production DAW/platform qualification or representative performance measurements.

## Current evidence boundary

Current Rust checkpoint `c6aa54f2022d456e89329fd969ba993eb8d76e52` is green for fmt, workspace tests, strict Clippy, and Loom.

Current Linux CLAP artifact:

- `clap-validator` 0.4.1: **35 passed, 0 failed, 9 intentional skips**;
- five-second two-worker validator fuzz: clean;
- validator coverage includes f32/f64 process-audio paths, parameter set/flush/sample-accurate/conversion/fuzz cases, state reproducibility, transport, reactivation/reset, sample-rate changes, random block sizes, and denormal behavior.

Current in-process CLAP host qualification through pinned Clack APIs covers:

- construction/destruction without activation;
- product activation failure followed by successful retry on the same instance;
- activate/start/process/stop/deactivate;
- sample-rate/block-size reactivation;
- processor transfer onto an audio thread without simultaneous mutation;
- repeated instance creation/destruction;
- f32 and f64 callback dispatch;
- stereo main + stereo sidechain mapping;
- exactly the configured 64 parameter events;
- 1,000 measured f32 plus 1,000 measured f64 callbacks with zero callback-thread allocation/deallocation;
- activation minimum/maximum frame counts and rejection above the configured maximum;
- CLAP latency extension plus host latency-change callback;
- nonzero reported latency matching actual delayed stereo audio;
- CLAP state save while an audio callback is active, proving coherent before/after automation publication.

The recorded REAPER 7.79/macOS-arm64 result predates current observable behavior. It remains historical host evidence rather than current-head production qualification.

## Core conformance

The format-independent proof is split by concern while exercising the same public runtime model:

- `crates/chassis-core/tests/conformance.rs`: schema rejection, lifecycle, buffer relationships, process bounds/context, base parameters, sample-accurate automation, invalid events, generic non-flat buffer-source traversal;
- `crates/chassis-core/tests/lifecycle_negative.rs`: product activation failure, state preservation, inactive post-failure state, successful reuse;
- `crates/chassis-core/tests/realtime_alloc.rs`: repeated post-activation process calls with callback-thread allocation/deallocation counting;
- `crates/chassis-core/tests/latency.rs`: activation-scoped latency snapshot/stability/removal/recomputation.

Grow these public-contract tests as semantics are added; do not create a second conceptual runtime for adapters.

## CLAP adapter conformance

In-process host tests under `crates/chassis-clap/tests/` exercise the actual exported Chassis CLAP entry rather than a synthetic adapter seam.

Important current cases:

- `host_conformance.rs`: full adapter callback allocation/event/precision/topology evidence, latency callbacks, reactivation, audio-thread transfer, repeated inactive instances;
- `host_frame_bounds.rs`: activation frame extremes and over-bound rejection;
- `host_activation_failure.rs`: product activation failure and same-instance retry;
- `host_latency_audio.rs`: reported nonzero latency equals actual impulse delay;
- `host_active_state_save.rs`: CLAP state save while processing is active returns one coherent generation.

This in-process host is useful semantic evidence, but it is not a substitute for production DAWs.

## Active state-save contract

Implemented CLAP contract:

- audio processing never waits for state save;
- save obtains one coherent completed scalar generation;
- a save racing a process block may linearize immediately before or after final automation-endpoint publication;
- save never emits a mixed cross-parameter generation;
- newer control/state publication prevents stale realtime completion from overwriting it;
- state load is transactional and requests host value rescan after publication.

Evidence exists at three levels:

1. direct publication unit tests;
2. Loom atomic models;
3. in-process CLAP host test that pauses an active audio callback before endpoint publication, saves the old complete generation, releases processing, then saves both new endpoints together.

A production-DAW active-save scenario remains a host-qualification item, not a semantic implementation gap.

## Realtime allocation/work evidence

Current mechanical evidence:

- core `InstanceRuntime::process` repeated post-activation callback path: zero measured allocation/deallocation;
- full CLAP adapter path: zero measured allocation/deallocation across 1,000 f32 and 1,000 f64 callbacks;
- measured adapter test uses stereo main + stereo sidechain and exactly the configured 64 parameter events;
- CLAP frame-count minimum and maximum process successfully and over-bound processing is rejected.

The thread-local test allocator intentionally counts only the callback test thread so unrelated Rust test-harness activity does not contaminate the result.

Remaining performance evidence is quantitative adapter overhead on representative stable hardware. Record CPU/architecture, build mode, sample rate, block size, topology, event count, precision, and toolchain. Shared CI VM timings are not production performance data.

## Lifecycle / negative space

Executable current coverage includes:

- create/init/destroy without activation;
- activation failure + same-instance retry;
- activate/start/process/stop/deactivate;
- repeated activation with changed sample rate/block bounds;
- processor transfer between host audio threads without simultaneous mutation;
- repeated instance creation/destruction;
- callback dimension/event rejection before product DSP.

Pinned Clack validator/fuzz additionally exercises lifecycle sequencing. Keep further reentrancy/module-unload work tied to actual host callbacks/tasks when those facilities exist.

## Latency / PDC evidence

Implemented semantics:

- one processor establishes `LatencySamples` during activation;
- `InstanceRuntime` snapshots it for the active lifetime;
- deactivation removes it and reactivation may compute a new value;
- CLAP exposes the snapshot through `PluginLatency` and calls the host latency-change hook when activation changes it.

The deliberate delayed host probe establishes metadata/DSP agreement: at 48 kHz it reports 64 samples and a stereo impulse emerges exactly at sample 64.

Still required for production PDC claims:

- a real DAW observing/compensating that latency;
- alignment/differential evidence with compensation enabled;
- production product restart/reactivation behavior when configuration changes required latency;
- cross-format latency parity once VST3/AU exist.

## State/adversarial tests

Treat host state as untrusted bytes. Keep coverage for malformed/truncated/oversized input, invalid types/numerics, wrong product/schema, unknown parameters, migration chains, resource exhaustion, and failed loads leaving live state unchanged.

Retain golden fixtures for every actually released schema. The current CHSS envelope remains pre-v1 until production-host and cross-format state gates are complete.

## Unsafe/FFI evidence

`chassis-core` forbids unsafe code. Production Chassis currently relies on Clack to validate raw CLAP pointer/alias relationships and expose safe channel pairs.

Test-only allocator instrumentation uses `GlobalAlloc` forwarding with explicit safety comments; this does not alter the production unsafe boundary.

If Chassis later owns unsafe adapter code:

- isolate the surface;
- document every unsafe block with discharged invariants;
- keep `unsafe_op_in_unsafe_fn` denied;
- run Miri on modelable Rust logic;
- use sanitizers/native stress for foreign boundaries;
- model subtle atomics with Loom before framework-wide promotion.

## Host matrix

| Host / harness | Platform | Status | Evidence |
| --- | --- | --- | --- |
| Clack in-process host harness | Linux CI | current | adapter process/allocation, f32/f64, lifecycle, active state save, latency metadata + delayed audio |
| `clap-validator` 0.4.1 | Linux CI | current | 35 pass / 0 fail / 9 skip + bounded fuzz |
| REAPER 7.79 | macOS arm64 | historical baseline | prior scan, instantiate, parameter state round trip, deterministic f32 render; predates current head |
| Bitwig | — | not run | not installed |

When a production DAW is available, refresh current-head scan/save-reopen/automation/PDC evidence rather than treating the synthetic host as equivalent.

## Cross-format differential gate

Once VST3/AU projections exist, render the same deterministic product state + input + automation/events through each format and compare:

```text
CLAP reference
VST3 compare
AU compare
```

Compare audio where meaningful plus state, identities, port configuration, automation timing, latency/tail, and editor-visible parameter semantics. Use bit-exact comparison for framework-only deterministic behavior when possible; use explicit tolerances only when semantics require them.

## Release evidence

A release advertised for commercial plugin use must record:

- Rust/tool versions;
- supported OS/architectures/formats;
- native validator results;
- tested DAW matrix;
- identity/state fixtures;
- realtime/performance evidence;
- known limitations.

Evidence, not confidence or source availability, earns production status.
