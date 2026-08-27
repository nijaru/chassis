# Validation and Conformance Strategy

Status: first format-independent conformance slice implemented; tooling expands with semantic layers/adapters.

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

There is currently **no authoritative hosted CI** for Chassis. Validation is local/tool-driven and must record exactly what was run.

Baseline Rust commands on a supported development machine:

```text
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-features --all-targets -- -D warnings
cargo deny check
```

Once third-party dependencies exist, also use `cargo machete`. Before promoting Rust unsafe code where Miri can model it, run `cargo miri test` for the relevant crates/tests.

Hosted automation may later run these commands, but the command/evidence contract is authoritative, not a CI provider.

## Conformance component

`crates/chassis-core/tests/conformance.rs` is the first deliberately boring external-API component. Its purpose is framework semantics, not DSP quality.

The current processor applies a deterministic fixed gain and exercises:

- default effect activation through the public `Component` contract;
- separate input/output buffers using explicit bounded copy-to-output processing;
- exact in-place buffers represented with one mutable Rust reference;
- `Processor::reset` lifecycle ownership;
- consuming deactivation / processor destruction;
- malformed structural audio configuration rejected before product activation;
- callback frame count above activated maximum rejected before product DSP;
- callback below a guaranteed positive minimum rejected before product DSP;
- zero-frame callback accepted when no positive minimum was promised;
- realtime, buffered-realtime, and offline per-call mode values.

This is only the first slice. It does **not** yet prove typed parameters, automation, persistent state, events/transport, allocation guards, adapters, FFI safety, host behavior, or production readiness.

Grow the same deterministic component as those features land instead of creating unrelated test products for every layer.

## Core unit tests

Format-independent unit/integration tests cover semantic authority and negative space as implemented:

- stable-key/schema uniqueness;
- whole I/O structural validation;
- process activation bounds;
- safe buffer construction/length rejection;
- exact-alias versus disjoint copy semantics;
- lifecycle/process callback containment.

As the corresponding features land, extend coverage to:

- parameter domain/mapping properties;
- process trajectory/event ordering;
- state encoding/migrations;
- state load failure leaving live authority unchanged;
- bounded queue/scratch behavior;
- generation/replacement semantics;
- transport calculations.

Property testing is appropriate after a dependency is reviewed under the license policy.

## Realtime allocation/work checks

The current code is designed so the first process path uses borrowed slices and no explicit allocation, but design inspection is **not** an allocation-proof claim.

Before promoting the runtime path, add a test-only allocation guard and verify normal/adversarial-valid cases.

Also prove work bounds. In particular, stable endpoint/schema resolution must not become a per-block allocating or quadratic scan merely because the API uses human-readable persistent keys. Resolve dense host/runtime mappings outside the callback where possible.

Do not optimize away the controlled `make_in_place()` copy merely to satisfy a zero-copy slogan; benchmark it under representative channel/block sizes before adding complexity.

## Lifecycle and ownership torture

The current conformance slice proves basic activation/reset/process/deactivation sequencing. Expand the same framework tests as lifecycle features appear:

- create/destroy without activation;
- repeated activate/reset/process/deactivate;
- invalid/failed activation cleanup;
- sample-rate/block-size reactivation;
- inactive I/O reconfiguration;
- state load before/after activation where legal;
- editor open/close/destroy loops;
- legal callback races during editor/instance teardown;
- stale worker/snapshot completion after replacement/destruction;
- repeated instance creation/destruction and module unload once task/runtime code exists.

Every resource introduced by Chassis needs a testable final owner/cleanup path.

## Adapter tests

Each format adapter independently verifies translation of:

- product/port/parameter identity;
- I/O layout/configuration;
- raw process buffer pointer/alias/length validation before Rust references exist;
- endpoint-to-dense-host-buffer mapping;
- parameter automation/modulation timing;
- state streams;
- lifecycle sequences;
- latency/tail/bypass capabilities when implemented;
- editor lifetime/resizing;
- note/MIDI semantics when implemented.

Invalid host data must be contained before construction of safe `ChannelBuffer` references.

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

Once multiple adapters exist, render the same deterministic conformance cases through each format and compare semantic results:

```text
same product state + input + automation/events
CLAP -> reference
VST3 -> compare
AU   -> compare
```

Compare audio output where meaningful plus state, parameters, port configuration, identity fixtures, latency/tail, and event timing. Use bit-exact comparison for framework-only behavior when possible and explicit tolerances only where semantics require them.

## State corruption/fuzz testing

Treat host state as untrusted bytes. When the state codec exists, test/fuzz arbitrary/truncated/oversized input, duplicate keys, invalid UTF-8/types/numerics, wrong product IDs, unknown versions, migration chains, and resource exhaustion.

Invalid state must not panic, allocate without configured bounds, or partially mutate live authority.

## Unsafe/FFI evidence

Format-independent crates deny unsafe code. The current `ChannelBuffer` layer relies only on safe references; unsafe alias proof belongs to adapters.

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
- controlled separate->in-place copies;
- parameter/event iteration;
- adapter translation;
- endpoint lookup/mapping;
- telemetry/control primitives;
- state encode/decode off-thread;
- any conversion path introduced by an adapter.

Record workload, sample rate, block sizes, channels/layout, CPU/architecture, build mode, and relevant toolchain. Do not promote lock-free/SIMD/cache-layout optimization without an apples-to-apples baseline that shows it matters.

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
