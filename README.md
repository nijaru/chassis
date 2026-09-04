# Chassis

Chassis is a convention-first Rust framework for professional realtime audio components.

The first production clients are audio effects. The format-independent core is deliberately compatible with instruments, standalone deployment, and embedded processing without making any one plugin format, GUI toolkit, or host part of the product-facing model.

Status: **private pre-alpha**. Public APIs and the CHSS persistence envelope are not frozen.

## Current checkpoint

Chassis is in **Phase 2: native CLAP qualification**. The core lifecycle/state model is established and the current Rust regression gate is green through activation-scoped latency and its CLAP projection.

Implemented today:

- durable `InstanceRuntime<P>` ownership of canonical parameter and custom semantic state;
- typed float/integer/boolean/choice parameters with stable persistent keys and dense realtime indices;
- deterministic bounded state documents, adjacent migrations, transactional replacement, and adversarial decode coverage;
- borrowed sample-accurate parameter events plus block-start transport context;
- generic allocation-free `ProcessBufferSource<S>` traversal;
- an executable post-activation core process allocation detector;
- activation-failure recovery and reusable-runtime negative-space coverage;
- activation-scoped `LatencySamples` captured from the active `Processor`;
- mapped CLAP f32/f64 audio ports, including auxiliary/input-only/output-only/asymmetric cases supported by the current core layout model;
- explicit stable CLAP audio/parameter IDs;
- exported `trim` automation that audibly drives deterministic f32/f64 DSP sample by sample;
- CLAP render/offline and latency/PDC extension projection;
- generation-checked scalar publication with direct concurrency tests and Loom evidence;
- choice plain-value/name round trips and host value rescan after state load.

The last recorded native conformance baseline predates the current automation/latency changes. That earlier artifact passed `clap-validator` 0.4.1 with **35 passes, 0 failures, 9 intentional skips**, plus a five-second two-worker fuzz run. REAPER 7.79/macOS-arm64 scanned, instantiated, round-tripped both parameters, and produced the expected deterministic f32 render. REAPER selected f32, so native host `data64` dispatch remained unexercised.

Those native results remain useful regression history, but they do **not** qualify current head after the audible automation and CLAP latency changes. The immediate checkpoint is to rebuild the current artifact and repeat validator/fuzz/REAPER qualification with an actual automation render and active-save scenario. Nonzero host PDC behavior will be qualified when a real delayed client or purpose-built delayed conformance case exercises it.

After that, finish adapter work-bound/overhead evidence and start the first real FX client: a mastering limiter that pressures lookahead latency, offline behavior, smoothing, telemetry, state, and deterministic rendering.

## Architecture

The ownership model is:

```text
Component
  immutable product schema/capabilities/factory

InstanceRuntime<P>
  durable framework-owned parameter + semantic state
  active lifecycle coordination
  optional active Processor
  activation-scoped latency snapshot

Processor
  exclusive mutable realtime DSP history while active

MainThread / Shared / Editor
  optional non-realtime orchestration and synchronized projections
```

Stable product/parameter/port identities are independent from Rust names, display labels, declaration order, runtime dense indices, and backend IDs.

Realtime rules apply to deterministic callbacks, not the whole program: no allocation/deallocation after activation, no blocking I/O, no contended/unbounded locks, and explicit work/resource bounds. Controlled copies are allowed when they simplify ownership and measurement does not justify more complexity.

## Workspace

```text
crates/
  chassis-core/     format-independent lifecycle, buffers, params, automation, state
  chassis-clap/     CLAP projection through the reviewed Clack revision
examples/
  clap-conformance/ deterministic exported qualification component
```

`chassis-core` forbids unsafe code. `chassis-clap` currently owns no Chassis unsafe block; Clack provides the ABI and safe channel views.

## Backend strategy

The CLAP adapter uses Clack pinned to exact revision `c5975f9f89f0953b00768680357985d46178078a`. The pin is a deliberate safety exception while the published Clack release does not contain the required reentrancy fix. A future safety-fixed crates.io release can replace the git pin only after audit and conformance requalification.

VST3/AUv2/AUv3 should initially project through `clap-wrapper`, but wrapper output earns support only through Chassis differential tests, native validators, and real-host qualification. Native format adapters are justified only by concrete semantic or maintenance limitations.

## Validation

GitHub Actions is a **portable Rust regression signal**, not the authority for plugin production qualification. Native validators, host renders, fuzz/stress, packaging, realtime measurements, and platform-specific evidence remain local/native gates.

Baseline Rust checks:

```text
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-features --all-targets --locked -- -D warnings
cargo deny check
cargo machete
```

Loom, Miri, sanitizers, native format validators, host tests, and benchmarks are added where the relevant boundary exists. Never describe an unexecuted check as passing.

## Read first

- [Architecture](docs/architecture.md)
- [Roadmap](docs/roadmap.md)
- [Current execution plan](docs/next-runtime-slice.md)
- [Open questions / freeze gates](docs/design/open-questions.md)
- [Validation strategy](docs/design/validation.md)
- [Parameters, automation, and state](docs/design/parameters-state.md)
- [Process buffers](docs/design/process-buffers.md)
- [Licensing](docs/licensing.md)
- [Dependency policy](docs/dependencies.md)

## Licensing

Chassis is AGPL-3.0-or-later with an intended separate commercial license for proprietary products. Dependency policy therefore considers both realtime/safety quality and commercial relicensing compatibility. Contributor/relicensing terms must be established before substantive outside contributions are accepted.
