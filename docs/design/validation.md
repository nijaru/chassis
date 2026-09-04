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

## Portable Rust regression gate

GitHub Actions is a portable regression signal for the Rust workspace. It is **not** the authority for native plugin support because it does not cover native packaging, DAW behavior, realtime measurements, or the full platform matrix.

Baseline local commands:

```text
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-features --all-targets --locked -- -D warnings
cargo deny check
cargo machete
cargo build --release -p chassis-clap-conformance --locked
```

Use the repository's named stable toolchain. Review lockfile/dependency-source changes rather than accepting generated output blindly.

## Current evidence boundary

The current parametered f64-capable CLAP artifact has recorded evidence on Apple Silicon macOS / Rust 1.98.0:

- workspace fmt/test/strict Clippy/deny/machete/release build green at the qualified checkpoint;
- every Clack package resolved to reviewed revision `c5975f9f89f0953b00768680357985d46178078a`;
- `clap-validator` 0.4.1 (source commit `b2f1d9b79b1d264a5747f46707d72b1aa40a02ef`) reported **35 passed, 0 failed, 9 intentional skips**;
- validator coverage includes f64 process-audio cases, parameter set/flush/sample-accurate/fuzz/conversion cases, and all state reproducibility cases;
- five-second two-worker validator fuzz completed without errors;
- REAPER 7.79/macOS-arm64 scanned, instantiated, round-tripped the choice + float state chunk through a saved project, and rendered the component deterministically;
- REAPER selected f32 processing for the current capability flags, so native host `data64` dispatch remains unexercised despite validator-qualified f64 processing.

The conformance artifact at that checkpoint used a process-inert parameter set with fixed 0.5 gain. The next qualification makes `trim` audibly control DSP so host automation can be checked against expected sample output.

Bitwig is not installed in the current environment and is not part of current evidence.

## Core conformance

`crates/chassis-core/tests/conformance.rs` is the external public-API proof for format-independent semantics.

It should exercise through `InstanceRuntime`, not the temporary activation-local compatibility path:

- construction/schema rejection;
- activation/reset/deactivation;
- separate and exact in-place buffers;
- malformed I/O rejected before product activation;
- frame-count bounds and legal zero-frame callbacks;
- realtime/buffered/offline process context;
- base parameter edits;
- sample-accurate automation trajectories;
- invalid events rejected before product DSP;
- generic non-flat `ProcessBufferSource` traversal.

Grow this same product contract as semantics are added; do not create a separate conceptual runtime for each adapter.

## CLAP conformance export

`examples/clap-conformance` is the deterministic native export probe.

Current adapter surface includes:

- setup-mapped f32/f64 audio ports through safe Clack channel relationships;
- arbitrary mapped current-core mono/stereo ports and auxiliary/asymmetric relationships;
- stable audio/parameter IDs;
- lifecycle mapping into `InstanceRuntime`;
- render realtime/offline mode;
- float/integer/boolean/choice parameter metadata and values;
- bounded parameter-event normalization;
- generation-checked scalar publication;
- bounded CHSS state save/load;
- transport play/record/tempo snapshot.

Next export evidence must make `trim` affect DSP and compare an automated host render against a deterministic reference trajectory.

## Active state-save contract

CLAP scalar state save is a non-realtime operation over the publication bridge.

Required/target contract:

- audio processing never waits for state save;
- save waits only as needed to obtain one coherent **completed** scalar generation;
- a save racing a process block may linearize immediately before or after that block's final automation-endpoint publication;
- save never emits a mixed cross-parameter generation;
- newer control/state publication prevents stale realtime completion from overwriting it;
- state load is transactional and requests host value rescan after publication.

Add direct concurrency/unit evidence for this contract and then a reproducible active-save host scenario. Do not call active-save production-qualified until both exist.

## Realtime allocation/work checks

The code is designed to avoid explicit process-time allocation: audio mapping/process slots, normalized-event storage, and control scratch are built before processing.

Design inspection is not allocation proof. Add instrumentation that fails when allocation/deallocation occurs after activation during representative process callbacks.

Stress at least:

- minimum/maximum/legal zero frame counts;
- configured maximum parameter-event count;
- main + auxiliary/sidechain mapped topology;
- f32 and f64 adapter paths where executable;
- separate and in-place relationships.

Measure adapter-only overhead separately from product DSP and record CPU/architecture, build mode, sample rate, block size, channel/layout, event count, and toolchain.

## Lifecycle / negative space

Keep executable coverage for:

- create/init/destroy without activation;
- activate/start/process/stop/deactivate;
- repeated activation/deactivation;
- reset/reactivate;
- activation failure cleanup;
- sample-rate/block-size reactivation;
- malformed host data at the lowest safe synthetic boundary available;
- processor transfer between host audio threads without simultaneous mutation;
- repeated instance creation/destruction;
- reentrant host callbacks allowed by the format/dependency model;
- eventual module unload once callback/task services exist.

Every introduced resource needs a named final owner and testable cleanup path.

## State/adversarial tests

Treat host state as untrusted bytes. Keep coverage for malformed/truncated/oversized input, invalid types/numerics, wrong product/schema, unknown parameters, migration chains, resource exhaustion, and failed loads leaving live state unchanged.

Retain golden fixtures for every actually released schema. The current CHSS envelope remains pre-v1 until active-save and cross-format gates are complete.

## Unsafe/FFI evidence

`chassis-core` forbids unsafe code. Chassis currently relies on Clack to validate raw CLAP pointer/alias relationships and expose safe channel pairs.

If Chassis later owns unsafe adapter code:

- isolate the surface;
- document every unsafe block with the discharged invariants;
- keep `unsafe_op_in_unsafe_fn` denied;
- run Miri on modelable Rust logic;
- use sanitizers/native stress for foreign boundaries;
- model subtle atomics with Loom before framework-wide promotion.

Safe wrapper consumption does not remove the need to audit the dependency's unsafe/lifetime behavior.

## Real-host matrix

Native validators do not replace DAWs. Maintain reproducible scenarios per supported format/OS/architecture.

Current recorded matrix:

| Host | Platform | Status | Evidence |
| --- | --- | --- | --- |
| REAPER 7.79 | macOS arm64 | pass for current checkpoint | scan, instantiate, parameter state round trip, deterministic f32 render; host chose f32 |
| Bitwig | — | not run | not installed |

Future VST3/AU coverage should include hosts that exercise save/reopen/automation and editor lifecycle, including Ableton Live and Logic where applicable.

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
