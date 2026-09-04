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

Current Rust code checkpoint `081caccadaca8c8038efa53f1fbbb41695143321` is green for GitHub Actions fmt, workspace tests, strict Clippy, and Loom.

Since the last native CLAP qualification checkpoint, current head has changed observable behavior and lifecycle evidence:

- exported `trim` now drives deterministic f32/f64 DSP sample by sample from base state plus automation events;
- the temporary activation-local runtime path was removed in favor of `InstanceRuntime`;
- scalar publication tests now cover coherent completed snapshots, in-progress multi-value writes, stale realtime publication, bounded realtime attempts, and generation exhaustion;
- core activation failure is tested to leave the runtime inactive, preserve state, and remain reusable;
- a callback-thread allocation detector covers repeated post-activation core processing;
- core latency is activation-scoped and CLAP now projects the latency extension.

The last recorded **native** baseline predates those changes:

- `clap-validator` 0.4.1 (source commit `b2f1d9b79b1d264a5747f46707d72b1aa40a02ef`) reported **35 passed, 0 failed, 9 intentional skips**;
- validator coverage included f64 process-audio cases, parameter set/flush/sample-accurate/fuzz/conversion cases, and state reproducibility cases;
- five-second two-worker validator fuzz completed without errors;
- REAPER 7.79/macOS-arm64 scanned, instantiated, round-tripped the choice + float state chunk through a saved project, and rendered the component deterministically;
- REAPER selected f32 processing for the advertised capability set, so native host `data64` dispatch remained unexercised.

Treat those results as regression history, not qualification of current head. Current-head native qualification requires rebuilding the artifact and rerunning validator/fuzz/REAPER after the automation and latency changes.

## Core conformance

The format-independent proof is split by concern while exercising the same public runtime model:

- `crates/chassis-core/tests/conformance.rs` covers schema rejection, lifecycle, safe buffer relationships, process bounds/context, base parameters, sample-accurate automation, invalid events, and generic non-flat buffer-source traversal through `InstanceRuntime`;
- `crates/chassis-core/tests/lifecycle_negative.rs` covers product activation failure, state preservation, inactive post-failure state, and successful reuse;
- `crates/chassis-core/tests/realtime_alloc.rs` checks repeated post-activation process calls for allocation/deallocation on the callback test thread;
- `crates/chassis-core/tests/latency.rs` proves latency is absent while inactive, snapshotted on successful activation, stable during that active lifetime, removed on deactivation, and recomputed on reactivation.

Grow these public-contract tests as semantics are added; do not create a second conceptual runtime for adapters.

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
- `trim` base state + sample-accurate automation driving exported f32/f64 samples;
- generation-checked scalar publication;
- bounded CHSS state save/load;
- CLAP latency extension backed by activation-scoped runtime latency;
- transport play/record/tempo snapshot.

The conformance processor currently reports zero latency. This can qualify extension presence and zero-latency behavior, but it cannot establish nonzero PDC correctness. A nonzero latency test must delay audio by exactly the reported amount, either in a deliberate delayed conformance product or the first real lookahead client.

Current native export evidence must be refreshed by rendering actual `trim` automation and comparing samples against the deterministic reference trajectory.

## Active state-save contract

CLAP scalar state save is a non-realtime operation over the publication bridge.

Implemented contract:

- audio processing never waits for state save;
- save waits only as needed to obtain one coherent **completed** scalar generation;
- a save racing a process block may linearize immediately before or after that block's final automation-endpoint publication;
- save never emits a mixed cross-parameter generation;
- newer control/state publication prevents stale realtime completion from overwriting it;
- state load is transactional and requests host value rescan after publication.

Direct executable publication tests cover the coherence and ordering rules, including an in-progress multi-value writer. Loom covers the atomic publication protocol. The remaining production gate is a reproducible real-host active-save-during-automation scenario, followed by equivalent cross-format evidence once VST3/AU exist.

## Realtime allocation/work checks

A core integration test now mechanically checks repeated post-activation `InstanceRuntime::process` calls for allocation/deallocation on the callback test thread. The thread-local gate avoids counting unrelated Rust test-harness activity while still intercepting allocations made by that callback path.

That result is deliberately narrow. It does **not** prove the complete CLAP adapter path allocation-free or quantify its cost.

Next stress/evidence should cover:

- configured maximum parameter-event count;
- representative mapped main + auxiliary/sidechain topology;
- minimum/maximum/legal zero frame counts;
- f32 and f64 adapter paths where executable;
- allocation/deallocation across the adapter process path;
- adapter-only overhead separate from product DSP.

Record CPU/architecture, build mode, sample rate, block size, channel/layout, event count, and toolchain for performance results.

## Lifecycle / negative space

Core activation failure cleanup is executable and covered. Keep/extend deployment coverage for:

- create/init/destroy without activation;
- activate/start/process/stop/deactivate;
- repeated activation/deactivation;
- reset/reactivate;
- sample-rate/block-size reactivation;
- malformed host data at the lowest safe synthetic boundary available;
- processor transfer between host audio threads without simultaneous mutation;
- repeated instance creation/destruction;
- reentrant host callbacks allowed by the format/dependency model;
- eventual module unload once callback/task services exist.

Every introduced resource needs a named final owner and testable cleanup path.

## Latency / PDC evidence

Core latency semantics are now explicit:

- one processor establishes a `LatencySamples` value during activation;
- `InstanceRuntime` snapshots that value for the active lifetime;
- deactivation removes it and reactivation may compute another value;
- CLAP exposes that snapshot through `PluginLatency` and calls the host latency-change hook when a new activation changes it.

Current Rust tests establish those ownership/lifecycle facts. Production PDC evidence additionally requires:

- a processor whose output is actually delayed by the declared count;
- host observation of that latency;
- alignment/differential evidence with compensation enabled;
- restart/reactivation behavior if product configuration changes the required latency;
- cross-format latency parity once VST3/AU exist.

Do not use metadata-only nonzero latency as a conformance shortcut.

## State/adversarial tests

Treat host state as untrusted bytes. Keep coverage for malformed/truncated/oversized input, invalid types/numerics, wrong product/schema, unknown parameters, migration chains, resource exhaustion, and failed loads leaving live state unchanged.

Retain golden fixtures for every actually released schema. The current CHSS envelope remains pre-v1 until active-host-save and cross-format gates are complete.

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
| REAPER 7.79 | macOS arm64 | previous native baseline passed; current head requires refresh | scan, instantiate, parameter state round trip, deterministic f32 render; host chose f32 |
| Bitwig | — | not run | not installed |

The current-head REAPER refresh should add an actual automated `trim` render and active-save-during-automation scenario. Future VST3/AU coverage should include hosts that exercise save/reopen/automation and editor lifecycle, including Ableton Live and Logic where applicable.

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
