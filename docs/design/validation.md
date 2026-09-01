# Validation and Conformance Strategy

Status: format-independent conformance and the CLAP adapter are locally validated. The current f64-capable artifact has passed the CLAP validator and a bit-exact headless REAPER render requalification; production host qualification remains open.

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
cargo machete
```

`cargo machete` now applies because the CLAP adapter has third-party dependencies. Before promoting Rust unsafe code where Miri can model it, run `cargo miri test` for the relevant crates/tests.

Hosted automation may later run these commands, but the command/evidence contract is authoritative, not a CI provider.

### Current evidence boundary

The first adapter gate is locally green on Apple Silicon macOS with Rust 1.98.0:

- the Cargo-generated lockfile resolves every Clack package to the reviewed revision `c5975f9f89f0953b00768680357985d46178078a`;
- workspace tests, fmt, Clippy, cargo-deny, and cargo-machete pass;
- the current release `cdylib` is packaged as a macOS `.clap` bundle with a valid `Info.plist`;
- `clap-validator` 0.4.1 (source commit `b2f1d9b79b1d264a5747f46707d72b1aa40a02ef`) reports 35 passed, 0 failed, and 9 intentional skips against the current parametered f64 artifact, including both double-precision process-audio cases, all parameter set/flush/fuzz/conversion cases, and all state-reproducibility cases;
- its five-second, two-worker fuzz run completed without errors, including malformed optional transport values;
- REAPER 7.79/macOS-arm64 scanned, instantiated, state-round-tripped both parameters (choice `parameter/mode`, float `parameter/trim`), and rendered the current parametered f64-capable artifact through a real project; the f32 differential render was bit-identical to 0.5× the no-FX render.

The conformance component's parameters are process-inert (fixed 0.5 gain regardless of value), so parameter-event paths are exercised for transport/state/round-trip correctness rather than audible automation response. The native result proves the tested CLAP lifecycle/buffer/transport/parameter/state path, not production host support or the complete CLAP contract. REAPER dispatched the current artifact on its f32 path (the artifact advertises `SUPPORTS_64BITS` without `PREFERS_64BITS`), so native `data64` dispatch remains unexercised; Bitwig is not installed in the current validation environment. REAPER in this environment runs unlicensed, which shows an evaluation nag window on interactive launches; all recorded evidence was gathered through batch `-renderproject` runs, which complete and exit without interaction.

## Core conformance component

`crates/chassis-core/tests/conformance.rs` is the deliberately boring external-API component for format-independent semantics.

The current processor applies deterministic fixed gain and exercises:

- default effect activation through the public `Component` contract;
- separate input/output buffers using explicit bounded copy-to-output processing;
- exact in-place buffers represented with one mutable Rust reference;
- `Processor::reset` lifecycle ownership;
- consuming deactivation / processor destruction;
- malformed structural audio configuration rejected before product activation;
- callback frame count above activated maximum rejected before product DSP;
- callback below a guaranteed positive minimum rejected before product DSP;
- zero-frame callback accepted when no positive minimum was promised;
- realtime, buffered-realtime, and offline per-call mode values;
- CLAP render-mode projection into realtime/offline process context.

Grow the same semantics as parameters/state/events land instead of creating unrelated product contracts for every layer.

## CLAP conformance export

`examples/clap-conformance` is the first actual format export probe. It deliberately implements a deterministic 0.5 gain and uses the explicit Chassis `Component`/`Processor`/`Process<S>` API for both `f32` and `f64`.

The current `chassis-clap` adapter supports the following semantic slice:

- Clack's safe plugin boundary from the pinned post-fix revision `c5975f9f89f0953b00768680357985d46178078a` (development workspace version 0.2.0);
- one required stereo main input/output pair;
- f32 processing, with optional f64 processing through an explicit capability marker;
- CLAP render-mode projection into realtime/offline process context;
- exact in-place or separate paired channels;
- Chassis activation/process/reset/deactivation;
- explicit scalar float, integer, and boolean parameter metadata/value mapping;
- bounded CLAP parameter-value event normalization into borrowed core set events;
- bounded CHSS parameter state save/load with product/schema checks;
- block-start CLAP transport play/record flags and tempo;
- conservative `ProcessStatus::Continue`;
- `ProcessMode::Realtime` and host-selected offline render mode.

It does **not** yet claim choice/index parameters, modulation or gesture output, note/MIDI events, sidechain/multibus, latency/tail, GUI, packaging, or production host support. F64 support is qualified at the adapter and validator level; expanded topology and real-host requalification remain open.

### First local adapter gate

On a supported development machine, regenerate and review the committed lockfile, then run:

```text
cargo fmt --all
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-features --all-targets -- -D warnings
cargo deny check
cargo machete
cargo build --release -p chassis-clap-conformance
```

Review the resulting `Cargo.lock` diff rather than merely accepting that Cargo generated it. Confirm every Clack package resolves to the full pinned revision, and confirm `clap-sys`, `bitflags`, and their registry checksums have expected provenance.

Any compile/lint failure is adapter feedback. Fix the API/translation rather than weakening the workspace lints or adding broad `allow` attributes.

## Realtime allocation/work checks

The core process path and CLAP translation are designed to avoid explicit process-time allocation: the adapter traverses Clack safe channel pairs directly and preallocates normalized parameter-event/control scratch during activation.

Design inspection is **not** an allocation-proof claim. Before promotion, add test/measurement evidence that both Chassis and the exact Clack path used do not allocate/block unexpectedly during process.

For general multibus support, do not replace the fixed proof with callback-time `Vec<ChannelBuffer>` construction. Resolve the borrowing/endpoint model from real adapter constraints first.

Also prove work bounds. Stable endpoint/schema resolution must not become per-block allocating or quadratic merely because persistent identities are human-readable.

Do not optimize away the controlled `make_in_place()` copy merely to satisfy a zero-copy slogan; benchmark it under representative channel/block sizes before adding complexity.

## Lifecycle and ownership torture

The format-independent conformance slice proves basic activation/reset/process/deactivation sequencing. The CLAP adapter now needs host-level sequences for:

- create/init/destroy without activation;
- activate/start/process/stop/deactivate;
- repeated activate/deactivate;
- reset while active under legal CLAP sequencing;
- activation failure cleanup;
- sample-rate/block-size reactivation;
- invalid/malformed audio port data where Clack exposes a safe synthetic boundary;
- processor movement between host audio threads without simultaneous processing;
- repeated instance creation/destruction;
- eventual module unload behavior once callbacks/tasks exist.

Every resource introduced by Chassis needs a testable final owner/cleanup path.

## Buffer adapter tests

The current Chassis CLAP translation receives Clack's safe `ChannelPair` relationships rather than raw CLAP pointers. That means Clack currently owns the raw-pointer validation/alias construction.

Chassis-level tests should verify translation/containment of:

- exact alias -> one Chassis `InPlace` mutable view;
- disjoint pair -> Chassis `Separate`;
- required input-only/output-only main channel rejection;
- wrong port count;
- wrong main channel count;
- unavailable or mixed sample representations, including a host-supplied `Both` representation;
- activation/callback frame-bound failures;
- mapped sidechain/auxiliary and asymmetric layouts where the activated adapter mapping declares them;

Separately audit Clack's internal unsafe implementation before production qualification; safe API consumption does not eliminate dependency trust review.

## Native CLAP qualification

The first narrow adapter qualification has been exercised locally:

1. the conformance `cdylib` was built in release mode;
2. it was packaged as a macOS `.clap` bundle with a platform-correct `Contents/MacOS` executable and `Info.plist`;
3. `clap-validator` 0.4.1 was built from source commit `b2f1d9b79b1d264a5747f46707d72b1aa40a02ef` and run with `validate --json` against the current parametered f64 artifact; it reported 35 passed, 0 failed, and 9 intentional skips, including both double-precision process-audio cases, the full parameter set/flush/sample-accurate/modulation-fuzz/conversion matrix, and the state-reproducibility matrix (basic, binary, buffered); a five-second, two-worker fuzz run also completed without errors;
4. validator lifecycle, buffer, block-size, sample-rate, reset/reactivate, transport-null, and transport-fuzz cases completed without failures; both double-precision in-place and out-of-place process cases succeeded; a five-second, two-worker fuzz run also completed without errors, including malformed optional transport values;
5. the earlier narrow bundle was loaded by REAPER 7.78/macOS-arm64 and rendered through a real project; a no-FX differential render confirmed the fixed 0.5 gain within 6e-8 absolute error;
6. the current parametered f64-capable artifact was loaded by REAPER 7.79/macOS-arm64 through a headless `-renderproject` run: the host scanned the bundle, instantiated the FX, saved and reloaded a state chunk carrying both parameters (`parameter/mode` = `Choice("clean")`, `parameter/trim` = `Float(0.5)`), and rendered 1-second 32-bit and 64-bit float fixtures at 44.1 kHz; the f32 FX render was bit-identical to 0.5× the bypassed render (44100 frames, zero violations), and the bypassed 64-bit float fixture passed through bit-identically; a 2e-38 subnormal probe and a bit-identical `f32(0.5×source)` comparison confirmed REAPER dispatched the plugin on its f32 path — the artifact advertises `SUPPORTS_64BITS` without `PREFERS_64BITS`, so this is the expected host choice, and native `data64` dispatch remains unexercised by the available host matrix;
7. the environment was Apple Silicon macOS with Rust 1.98.0 and the stable toolchain named by `rust-toolchain.toml`.

Bitwig validation remains pending because it is not installed in this environment. A validator pass and one REAPER smoke test are additional evidence, not proof of production support.

## Core unit tests

Format-independent unit/integration tests cover semantic authority and negative space as implemented:

- stable-key/schema uniqueness;
- whole I/O structural validation;
- process activation bounds;
- safe buffer construction/length rejection;
- exact-alias versus disjoint copy semantics;
- lifecycle/process callback containment;
- typed parameter schema/default/range/choice validation;
- owned base-value edits and transactional parameter state application;
- bounded deterministic state encoding/decoding and malformed-input rejection;
- bounded sample-sorted parameter events and lazy set/linear trajectories;
- invalid automation rejection before product DSP;
- optional block-start transport context and frame-bound matching.

As the corresponding features land, extend coverage to:

- parameter mapping properties;
- host-specific process trajectory/event ordering;
- scalar CLAP metadata, event, transport, and state projection tests;
- state encoding/migrations;
- state load failure leaving live authority unchanged;
- bounded queue/scratch behavior and allocation instrumentation;
- generation/replacement semantics;
- richer transport calculations and availability mappings.

Property testing is appropriate after a dependency is reviewed under the license policy.

## Other format adapters

Each later format/projection independently verifies translation of:

- product/port/parameter identity;
- I/O layout/configuration;
- process buffer aliasing/length semantics;
- endpoint-to-dense-host-buffer mapping;
- parameter automation/modulation timing;
- state streams;
- lifecycle sequences;
- latency/tail/bypass capabilities when implemented;
- editor lifetime/resizing;
- note/MIDI semantics when implemented.

Do not infer VST3/AU correctness from native CLAP success or from `clap-wrapper` merely building.

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

Current evidence:

| Host | Platform | Result | Scenario |
| --- | --- | --- | --- |
| REAPER 7.78 | macOS arm64 | pass | loaded the earlier narrow packaged CLAP, activated it, and rendered a stereo f32 fixture with differential 0.5-gain verification within 6e-8 |
| REAPER 7.79 | macOS arm64 | pass | headless `-renderproject` render of the current parametered f64-capable artifact: scan, instantiate, two-parameter state round trip, bit-exact f32 0.5-gain differential, bit-identical bypassed 64-bit float pass-through; f32 dispatch confirmed via subnormal probe (`data64` remains unexercised). Interactive launches show an unlicensed evaluation nag in this environment; evidence used batch rendering only |
| Bitwig | — | not run | not installed in the validation environment |

Likely early coverage also includes Ableton Live, Logic Pro for AU, and a CLAP-heavy host such as Bitwig. Exact versions are release evidence, not permanent architecture assumptions.

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

Format-independent crates deny unsafe code. The first `chassis-clap` slice also owns no unsafe block; Clack supplies the CLAP ABI and safe audio views.

If Chassis later owns adapter unsafe code:

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
