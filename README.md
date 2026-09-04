# Chassis

Chassis is a convention-first Rust framework for professional realtime audio components.

The first production clients are audio effects. The format-independent core is deliberately compatible with instruments, standalone deployment, and embedded processing without making any one plugin format, GUI toolkit, or host part of the product-facing model.

Status: **private pre-alpha**. Public APIs and the CHSS persistence envelope are not frozen.

## Current checkpoint

Chassis is now in **Phase 2: native CLAP qualification**, with a small number of Phase-1 semantic consistency gates still open.

Implemented today:

- durable `InstanceRuntime<P>` ownership of canonical parameter and custom semantic state;
- typed float/integer/boolean/choice parameters with stable persistent keys and dense realtime indices;
- deterministic bounded state documents, adjacent migrations, transactional replacement, and adversarial decode coverage;
- borrowed sample-accurate parameter events plus block-start transport context;
- generic allocation-free `ProcessBufferSource<S>` traversal;
- mapped CLAP f32/f64 audio ports, including auxiliary/input-only/output-only/asymmetric cases supported by the current core layout model;
- explicit stable CLAP audio/parameter IDs;
- CLAP render/offline projection;
- generation-checked scalar publication qualified under Loom;
- choice plain-value/name round trips and host value rescan after state load.

The current exported conformance artifact passed `clap-validator` 0.4.1 with **35 passes, 0 failures, 9 intentional skips**, plus a five-second two-worker fuzz run. REAPER 7.79/macOS-arm64 scanned, instantiated, state-round-tripped both parameters, and rendered the artifact through a saved project; the fixed-gain f32 render was bit-identical to the expected differential. Native host `data64` dispatch remains unexercised because REAPER selected the advertised f32 path.

The next checkpoint makes the exported `trim` parameter audibly drive DSP, qualifies active save during automation, instruments realtime allocation/work bounds, then starts the first real FX client.

## Architecture

The ownership model is:

```text
Component
  immutable product schema/capabilities/factory

InstanceRuntime<P>
  durable framework-owned parameter + semantic state
  active lifecycle coordination
  optional active Processor

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
