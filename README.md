# Chassis

Chassis is a convention-first Rust framework for professional realtime audio components.

The first production clients are audio effects. The format-independent core is deliberately compatible with instruments, standalone deployment, and embedded processing without making any one plugin format, GUI toolkit, or host part of the product-facing model.

Status: **private pre-alpha**. Public APIs and the CHSS persistence envelope are not frozen.

## Current checkpoint

Chassis is in **Phase 2: native CLAP qualification**. The format-independent lifecycle/state model and the current CLAP adapter semantics are executable and qualified on the portable Rust + Linux headless matrix. Production DAW/platform qualification remains separate.

Implemented today:

- durable `InstanceRuntime<P>` ownership of canonical parameter and custom semantic state;
- typed float/integer/boolean/choice parameters with stable persistent keys and dense realtime indices;
- deterministic bounded state documents, adjacent migrations, transactional replacement, and adversarial decode coverage;
- borrowed sample-accurate parameter events plus block-start transport context;
- generic allocation-free `ProcessBufferSource<S>` traversal;
- activation-failure recovery and reusable-runtime negative-space coverage;
- activation-scoped `LatencySamples` captured from the active `Processor`;
- mapped CLAP f32/f64 audio ports, including auxiliary/input-only/output-only/asymmetric cases supported by the current core layout model;
- explicit stable CLAP audio/parameter IDs;
- exported `trim` automation that audibly drives deterministic f32/f64 DSP sample by sample;
- CLAP render/offline and latency/PDC extension projection;
- generation-checked scalar publication with direct concurrency tests and Loom evidence;
- coherent active state save through the real CLAP extension while processing is in progress;
- full in-process CLAP adapter allocation/lifecycle tests through pinned Clack host APIs.

Current portable/headless evidence includes:

- Rust 1.98.0: workspace fmt, tests, strict Clippy, and Loom green at `c6aa54f2022d456e89329fd969ba993eb8d76e52`;
- current Linux conformance artifact: `clap-validator` 0.4.1 at pinned revision `b2f1d9b79b1d264a5747f46707d72b1aa40a02ef`, **35 passed, 0 failed, 9 intentional skips**;
- five-second two-worker validator fuzz: clean;
- full adapter callback path: zero measured allocation/deallocation across 1,000 f32 and 1,000 f64 callbacks at the configured 64-event bound with stereo main + stereo sidechain;
- CLAP frame-count minimum/maximum accepted and over-bound blocks rejected;
- product activation failure leaves the same CLAP instance reusable;
- repeated inactive instance construction/destruction, sample-rate/block-size reactivation, and audio-thread processor transfer are exercised;
- activation latency changes are visible through `PluginLatency` and the host latency-change callback;
- a deliberate delayed probe reports 64 samples and produces its stereo impulse exactly 64 samples later;
- active state save while an audio callback is paused returns the coherent pre-publication generation, then the completed automation endpoints after the callback publishes.

The previously recorded REAPER 7.79/macOS-arm64 result remains useful historical host evidence, but it predates the newest automation/latency work and is **not** current-head production qualification. When a real DAW is available again, refresh scan/instantiate/save-reopen, automated `trim` render, active-save, PDC alignment, and native host precision behavior.

The remaining code-side Phase-2 evidence gap is representative adapter-only performance measurement on stable hardware. CI runner timing is not treated as production performance data.

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

`chassis-core` forbids unsafe code. Production `chassis-clap` code currently owns no Chassis unsafe block; Clack provides the ABI and safe channel views. Test-only allocation instrumentation uses unsafe allocator forwarding solely to measure callback behavior.

## Backend strategy

The CLAP adapter uses Clack pinned to exact revision `c5975f9f89f0953b00768680357985d46178078a`. The pin is a deliberate safety exception while the published Clack release does not contain the required reentrancy fix. A future safety-fixed crates.io release can replace the git pin only after audit and conformance requalification.

VST3/AUv2/AUv3 should initially project through `clap-wrapper`, but wrapper output earns support only through Chassis differential tests, native validators, and real-host qualification. Native format adapters are justified only by concrete semantic or maintenance limitations.

## Validation

GitHub Actions provides two distinct portable signals:

- **Rust CI**: fmt, workspace tests, strict Clippy, and Loom;
- **CLAP Conformance**: build/package the Linux conformance plugin, run the pinned `clap-validator`, and run bounded fuzzing.

Neither substitutes for production DAW/platform qualification or representative performance measurement.

Baseline local checks additionally include dependency/license/dead-dependency review when those tools are configured and available. Never describe an unexecuted check as passing.

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
