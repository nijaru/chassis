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

## Automated headless gates

### Rust CI

Linux:

- `cargo fmt --all -- --check`;
- `cargo test --workspace --locked`;
- `cargo clippy --workspace --all-features --all-targets --locked -- -D warnings`;
- Loom publication model;
- `cargo deny check`;
- `cargo machete`.

macOS and Windows:

- `cargo test --workspace --locked`;
- `cargo check --workspace --all-features --all-targets --locked`.

Dependency-policy tooling is pinned in CI to `cargo-deny` 0.20.2 and `cargo-machete` 0.9.2.

### CLAP Conformance

On Linux, macOS, and Windows:

1. build the release conformance artifact;
2. package the platform-native `.clap` form;
3. build `clap-validator` from pinned revision `b2f1d9b79b1d264a5747f46707d72b1aa40a02ef`;
4. run the validator;
5. run a bounded five-second two-worker fuzz pass.

The current validator suite reports **35 passed, 0 failed, 9 intentional skips** on the supported headless matrix.

These gates do not establish production DAW behavior or representative performance.

## Current in-process CLAP evidence

Tests under `crates/chassis-clap/tests/` exercise the actual exported Chassis CLAP entry through pinned Clack host APIs rather than a synthetic adapter-only seam.

Current cases include:

- `host_conformance.rs`: f32/f64 callback dispatch, stereo main + stereo sidechain, configured 64-event bound, full callback allocation/deallocation counting, reactivation, latency callbacks, audio-thread transfer, repeated inactive instances;
- `host_frame_bounds.rs`: activation frame extremes and over-bound rejection;
- `host_activation_failure.rs`: product activation failure and same-instance retry;
- `host_latency_audio.rs`: nonzero reported latency matching actual delayed audio;
- `host_active_state_save.rs`: coherent CLAP state save while processing is active;
- `host_reentrancy.rs`: plugin -> host -> plugin main-thread re-entry through parameter-rescan and latency-change callbacks;
- `host_panic_containment.rs`: activation/process panic containment at the Clack FFI boundary.

The full adapter allocation test covers 1,000 f32 and 1,000 f64 callbacks with stereo main + stereo sidechain and exactly the configured 64 parameter events, with zero measured allocation/deallocation on the callback thread.

## Core conformance

The format-independent proof remains split by concern while exercising the same public runtime model:

- `crates/chassis-core/tests/conformance.rs`: schema rejection, lifecycle, buffer relationships, process bounds/context, base parameters, sample-accurate automation, invalid events, generic non-flat buffer-source traversal;
- `crates/chassis-core/tests/lifecycle_negative.rs`: product activation failure, state preservation, inactive post-failure state, successful reuse;
- `crates/chassis-core/tests/realtime_alloc.rs`: repeated post-activation process calls with callback-thread allocation/deallocation counting;
- `crates/chassis-core/tests/latency.rs`: activation-scoped latency snapshot/stability/removal/recomputation.

Grow these public-contract tests as semantics are added; do not create a second conceptual runtime for adapters.

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
3. in-process CLAP host test that pauses an active callback before endpoint publication, saves the old complete generation, releases processing, then saves both new endpoints together.

A production-DAW active-save scenario remains host qualification, not a semantic implementation gap.

## Realtime allocation and performance

Current mechanical evidence:

- core `InstanceRuntime::process`: zero measured post-activation callback-thread allocation/deallocation;
- full CLAP adapter: zero measured allocation/deallocation under the representative mapped topology/event-bound test;
- frame-count minimum/maximum callbacks succeed and over-bound processing is rejected.

A manual adapter-overhead harness exists at `crates/chassis-clap/benches/adapter_overhead.rs` and runs with:

```text
cargo bench -p chassis-clap --bench adapter_overhead
```

It exercises 32/64/128/512-frame blocks, f32/f64, zero/max event load, and stereo main + sidechain. Run it on stable representative hardware and record CPU/architecture, OS, Rust version, release profile, sample rate, topology, and event load. Shared CI VM timing is not production performance evidence.

Representative measurement (first recording, 2026-09-03, Nick's Mac15,9 / Apple M3 Max / arm64, macOS 26.6.2, rustc 1.98.0 (88d9e12ae 2026-08-18), cargo bench release profile, 48 kHz, stereo main + stereo sidechain, trivial no-op DSP):

- no-event load: median 49–53 ns/callback across every block size and precision, ~19–20 M callbacks/s;
- configured 64-event load: median 477–499 ns/callback, ~2.0 M callbacks/s;
- overhead is flat in frame count and f32/f64; event normalization dominates at ~7 ns/event;
- two consecutive runs agreed within ~4% on medians; p99 stayed within ~6% of median except single-cell OS noise.

Interpretation: adapter-only overhead is well under a single sample period at 48 kHz (20.8 µs); there is no material adapter cost to optimize before real-client work. This is a distribution measurement on one machine, not a universal constant.

## Lifecycle / negative space

Executable current coverage includes:

- create/init/destroy without activation;
- result-based activation failure + same-instance retry;
- activation panic + same-instance retry;
- activate/start/process/stop/deactivate;
- process panic mapping to host processing failure followed by clean stop/deactivation;
- repeated activation with changed sample rate/block bounds;
- processor transfer between host audio threads without simultaneous mutation;
- repeated instance creation/destruction;
- callback dimension/event rejection before product DSP;
- main-thread synchronous host re-entry during state-rescan and latency-change calls.

Pinned validator/fuzz additionally exercises lifecycle sequencing.

## Panic / FFI boundary

`chassis-core` forbids unsafe code. Production Chassis currently relies on Clack for raw CLAP pointer/alias validation, safe channel views, and panic containment at FFI callbacks.

Pinned Clack wraps plugin FFI handlers in `std::panic::catch_unwind`. Chassis executable evidence establishes:

- an activation panic is caught, reported as activation failure, and the plugin remains inactive/reusable;
- a process panic is caught and returned to the host as processing failure;
- the host can then stop and deactivate cleanly.

Do not infer that arbitrary DSP state is valid for continued processing after a panic. Process failure should terminate that processing run.

Test-only allocator instrumentation uses `GlobalAlloc` forwarding with explicit safety comments and does not alter the production unsafe boundary.

If Chassis later owns unsafe adapter code, isolate it, document discharged invariants, keep `unsafe_op_in_unsafe_fn` denied, and add matching Miri/sanitizer/native evidence where modelable.

## Re-entrancy evidence

The exact Clack pin is required because published 0.1.1 predates the main-thread re-entrancy safety fix. Chassis now exercises the relevant behavior directly:

- state load calls the host parameter-rescan extension;
- the host immediately re-enters plugin extension discovery;
- activation calls the host latency-change extension;
- the host immediately re-enters plugin latency extension discovery.

Bitwig remains useful real-world confirmation when available, but main-thread re-entrancy is no longer an untested semantic assumption.

## Latency / PDC evidence

Implemented semantics:

- one processor establishes `LatencySamples` during activation;
- `InstanceRuntime` snapshots it for the active lifetime;
- deactivation removes it and reactivation may compute another value;
- CLAP exposes the snapshot through `PluginLatency` and calls the host latency-change hook when activation changes it.

The deliberate delayed probe reports 64 samples at 48 kHz and its stereo impulse emerges exactly at sample 64.

Still required for production PDC claims:

- a real DAW observing/compensating that latency;
- alignment/differential evidence with compensation enabled;
- production product restart/reactivation behavior when configuration changes required latency;
- cross-format latency parity once VST3/AU exist.

## State/adversarial tests

Treat host state as untrusted bytes. Keep coverage for malformed/truncated/oversized input, invalid types/numerics, wrong product/schema, unknown parameters, migration chains, resource exhaustion, and failed loads leaving live state unchanged.

Retain golden fixtures for every released schema. The current CHSS envelope remains pre-v1 until production-host and cross-format state gates are complete.

## Host matrix

| Host / harness | Platform | Status | Evidence |
| --- | --- | --- | --- |
| Clack in-process host harness | Linux/macOS/Windows CI | current | adapter process/allocation, f32/f64, lifecycle, active state save, latency, re-entrancy, panic containment |
| `clap-validator` 0.4.1 | Linux/macOS/Windows CI | current | 35 pass / 0 fail / 9 skip + bounded fuzz |
| REAPER 7.79 | macOS arm64 (M3 Max, 48 kHz) | current head | 2026-09-06 headless `-renderproject` qualification: scan + instantiate (FX browser, 5 params enumerated), square `trim` automation render sample-exact against the deterministic reference (value changes land at exact sample offsets 24000/36000), injected-CHSS state load render sample-exact (trim 0.25/0.75; blobs byte-identical to a REAPER-saved blob at 0.5 and validated by the real `StateDocument` decoder), project save while the transport rolled through automation with coherent base state (trim=0.5, not torn) and sample-exact automation replay after reload, no-FX baseline render bit-identical to the source fixture. Host-quirk recorded: `TrackFX_SetParamNormalized` on an inactive CLAP reports success but delivers no change; the differential shows FabFilter Pro-Q 4 (CLAP) behaving identically while ReaEQ (VST) applies immediately, so this is REAPER's inactive-CLAP delivery behavior, not an adapter defect — the automation path (process events) is sample-exact. |
| Bitwig | — | not run | production-host re-entrancy confirmation remains useful when available |

When a production DAW is available, refresh current-head scan/save-reopen/automation/PDC evidence rather than treating the headless harnesses as equivalent.

## Cross-format differential gate

Once VST3/AU projections exist, render the same deterministic product state + input + automation/events through each format and compare audio, state, identities, port configuration, automation timing, latency/tail, and editor-visible parameter semantics. Use bit-exact comparison for framework-only deterministic behavior when possible and explicit tolerances only where semantics require them.

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
