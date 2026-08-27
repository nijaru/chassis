# Validation and Conformance Strategy

Status: design direction; tooling evolves with implemented adapters.

## Goal

Framework correctness must be observable and reproducible. “It builds,” one validator pass, or one successful DAW load is not enough to promote a Chassis adapter or API.

Keep separate claims for:

- Rust/type-level correctness;
- format/API conformance;
- realtime invariants;
- lifecycle/ownership correctness;
- state/identity compatibility;
- cross-format semantic parity;
- performance;
- production host qualification.

A stronger claim requires matching evidence.

## Current validation authority

There is currently **no authoritative hosted CI** for Chassis. GitHub Actions was removed rather than leaving a permanently failing/non-running badge that could be mistaken for validation.

Until hosted automation is intentionally restored, validation is local/tool-driven and must record exactly what was run. A code change is not “green” because no remote check exists.

Baseline Rust commands on a supported development machine:

```text
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-features --all-targets -- -D warnings
cargo deny check
```

Once third-party dependencies exist, also use `cargo machete` to detect dead dependencies. Before promoting Rust unsafe code where Miri can model it, run `cargo miri test` for the relevant crates/tests.

Hosted automation may later run these commands, but the command/evidence contract is authoritative, not a particular CI provider.

## Conformance component

Build a deliberately boring component whose purpose is framework semantics, not DSP quality.

Grow it with the implementation rather than implementing every roadmap feature up front. The first useful slice needs:

- default stereo main input/output plus optional sidechain configuration;
- a few typed parameters;
- deterministic processing;
- timestamped automation;
- state round-trip;
- lifecycle activation/reset/deactivation;
- offline/realtime process mode coverage where available.

Add latency/tail, editor lifecycle, telemetry, background preparation, note/MIDI, multiple buses, etc. when those framework features actually land.

## Core tests

Format-independent tests cover semantic authority and negative space:

- stable-key/schema uniqueness;
- whole I/O configuration validation;
- parameter domain/mapping properties;
- process trajectory/event ordering;
- state encoding/migrations;
- state load failure leaving live authority unchanged;
- bounded queue/scratch behavior when those primitives exist;
- generation/replacement semantics;
- transport calculations.

Property testing is appropriate for mapping/state parsers after a dependency is reviewed under the license policy.

## Adapter tests

Each format adapter independently verifies translation of:

- product/port/parameter identity;
- I/O layout/configuration;
- process buffer aliasing and lengths;
- parameter automation/modulation timing;
- state streams;
- lifecycle sequences;
- latency/tail/bypass capabilities when implemented;
- editor lifetime/resizing;
- note/MIDI semantics when implemented.

Adapter tests specifically attack invalid host pointers/lengths/configuration where a safe synthetic boundary can model them. FFI validation precedes construction of safe Rust references.

## Native validators

Use target ecosystem validators as additional evidence:

- CLAP validator tooling;
- Steinberg VST3 validator;
- `auval` for Audio Unit;
- pluginval where useful;
- AAX/Avid/PACE validation only when that adapter exists.

Record validator versions and exact configurations for release qualification.

## Real-host matrix

Validators do not replace DAWs. Before claiming production support, maintain a tested host/OS/architecture matrix with reproducible scenarios.

Likely early coverage includes REAPER, Ableton Live, Logic Pro for AU, and a CLAP-heavy host such as Bitwig. Exact versions are release evidence, not permanent architecture assumptions.

## Differential cross-format testing

Render the same deterministic conformance cases through each exported format and compare semantic results:

```text
same product state + input + automation/events
CLAP -> reference
VST3 -> compare
AU   -> compare
```

Compare audio output where meaningful plus state, parameters, port configuration, identity fixtures, latency/tail, and event timing. Use bit-exact comparison for framework-only behavior when possible and explicit tolerances only where semantics require them.

## Realtime allocation/work checks

Framework-owned process paths need evidence that no heap allocation occurs after activation.

Test normal and adversarial-but-valid cases including automation-heavy blocks, optional buses, maximum configured dimensions, telemetry/control capacity exhaustion, and output-event rejection.

Also test that work remains bounded by validated dimensions. An allocation guard alone does not prove deterministic latency.

Do not optimize away controlled copies merely to satisfy a zero-copy slogan; benchmark them under representative channel/block sizes before adding ownership/unsafe complexity.

## Lifecycle and ownership torture

Exercise creation, failure, replacement, cancellation, and teardown—not only the happy path:

- create/destroy without activation;
- repeated activate/reset/process/deactivate;
- invalid/failed activation cleanup;
- state load before/after activation where legal;
- sample-rate/block-size reactivation;
- inactive I/O reconfiguration;
- editor open/close/destroy loops;
- legal callback races during editor/instance teardown;
- stale worker/snapshot completion after state replacement or destruction;
- repeated instance creation/destruction and module unload scenarios once task/runtime code exists.

Every resource introduced by Chassis needs a testable final owner/cleanup path.

## State corruption/fuzz testing

Treat host state as untrusted bytes. Test/fuzz arbitrary/truncated/oversized input, duplicate keys, invalid UTF-8/types/numerics, unknown versions, migration chains, and resource exhaustion.

Invalid state must not panic, allocate without configured bounds, or partially mutate live authority.

## Unsafe/FFI evidence

Format-independent crates deny unsafe code.

For adapter unsafe code:

- minimize and isolate the unsafe surface;
- every unsafe block has a `// SAFETY:` invariant explanation;
- use Edition 2024 `unsafe extern` forms;
- keep `unsafe_op_in_unsafe_fn` denied;
- run Miri on modelable Rust logic;
- use sanitizers/native stress for C/C++/Objective-C boundaries where practical;
- use Loom/model tests before framework-wide adoption of subtle atomic memory-ordering algorithms.

Never infer memory safety solely from a host spec; validate the facts needed to create Rust references.

## Performance evidence

Benchmark framework overhead separately from product DSP:

- pass-through/buffer access;
- parameter/event iteration;
- adapter translation;
- telemetry/control primitives;
- state encode/decode off-thread;
- any conversion/copy path introduced by an adapter.

Record workload, sample rate, block sizes, channels/layout, CPU/architecture, build mode, and relevant toolchain. Do not promote a lock-free/SIMD/cache-layout optimization without an apples-to-apples baseline that shows it matters.

## Release evidence

A release advertised for commercial plugin use should record:

- Rust/tooling versions used to build and validate;
- supported OS/architectures/formats;
- native validator versions/results;
- tested DAW matrix;
- identity/state compatibility fixtures;
- known limitations;
- relevant performance/realtime evidence.

Reproducible evidence, not development style or framework confidence, earns production status.
