# Chassis

Chassis is a convention-first Rust framework for professional realtime audio components.

The first production clients are audio effects. The format-independent core is deliberately compatible with instruments, standalone deployment, and embedded processing without making any plugin format, GUI toolkit, or host part of the product-facing model.

Status: **private pre-alpha**. Public APIs and the CHSS persistence envelope are not frozen.

## Current checkpoint

Chassis is in **Phase 2: native CLAP qualification**. The core lifecycle/state model and current CLAP semantics have strong executable headless coverage. Production DAW behavior and representative hardware performance remain separate gates.

Implemented today:

- durable `InstanceRuntime<P>` ownership of canonical parameter and custom semantic state;
- typed float/integer/boolean/choice parameters with stable persistent keys and dense realtime indices;
- deterministic bounded state documents, adjacent migrations, transactional replacement, and adversarial decode coverage;
- borrowed sample-accurate parameter events plus block-start transport context;
- generic allocation-free `ProcessBufferSource<S>` traversal;
- activation-failure recovery and activation-scoped `LatencySamples`;
- mapped CLAP f32/f64 audio ports, including auxiliary/input-only/output-only/asymmetric cases supported by the current core layout model;
- explicit stable CLAP audio/parameter IDs;
- exported `trim` automation that audibly drives deterministic f32/f64 DSP sample by sample;
- CLAP render/offline and latency extension projection;
- generation-checked scalar publication with direct concurrency tests and Loom evidence;
- coherent active state save through the real CLAP state extension while processing is active;
- full in-process CLAP adapter allocation/lifecycle/re-entrancy tests through pinned Clack host APIs;
- FFI panic containment tests for activation and process callbacks.

Current automated evidence includes:

- Rust 1.98.0 workspace fmt, tests, strict Clippy, and Loom;
- full workspace tests and all-target/all-feature checks on Linux, macOS, and Windows CI;
- `cargo-deny` advisory/license/ban/source policy and `cargo-machete` unused-dependency checks;
- packaged CLAP artifacts validated and bounded-fuzzed on Linux, macOS, and Windows with pinned `clap-validator` 0.4.1;
- current validator suite: **35 passed, 0 failed, 9 intentional skips**;
- full adapter callback path: zero measured allocation/deallocation across 1,000 f32 and 1,000 f64 callbacks at the configured 64-event bound with stereo main + stereo sidechain;
- CLAP frame-count minimum/maximum accepted and over-bound blocks rejected;
- product activation failure and product activation panic both leave the CLAP instance inactive and reusable;
- product process panic is contained at the Clack FFI boundary and reported to the host as processing failure;
- repeated inactive instance construction/destruction, sample-rate/block-size reactivation, and audio-thread processor transfer;
- plugin -> host -> plugin main-thread re-entry during state rescan and latency-change callbacks;
- a deliberate delayed probe whose reported 64-sample latency matches its actual stereo impulse delay;
- active state save during processing linearizing to one coherent completed parameter generation.

Current head carries REAPER 7.79/macOS-arm64 (M3 Max, 48 kHz) real-DAW evidence (2026-09-06) for scan/instantiate, sample-exact automated `trim` rendering, injected-state save/reload rendering, active-save during automated playback, and PDC alignment with the exported delayed probe; see `docs/design/validation.md` for the host matrix. Native f64 dispatch remains open (needs a host that selects `data64`).

The remaining code-side Phase-2 measurement gap is adapter-only overhead on stable representative hardware. CI runner timing is not treated as production performance data. A dependency-free `cargo bench -p chassis-clap --bench adapter_overhead` harness is ready for that measurement.

## Architecture

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

Realtime rules apply to deterministic callbacks: no allocation/deallocation after activation, no blocking I/O, no contended/unbounded locks, and explicit work/resource bounds. Controlled copies are allowed when they simplify ownership and measurement does not justify more complexity.

## Workspace

```text
crates/
  chassis-core/     format-independent lifecycle, buffers, params, automation, state
  chassis-clap/     CLAP projection through the reviewed Clack revision
examples/
  clap-conformance/ deterministic exported qualification component
  clap-delayed-probe/ exported nonzero-latency probe for host PDC qualification
```

`chassis-core` forbids unsafe code. Production `chassis-clap` code currently owns no Chassis unsafe block; Clack provides the ABI and safe channel views. Test-only allocation instrumentation uses unsafe allocator forwarding solely to measure callback behavior.

## Backend strategy

The CLAP adapter uses Clack pinned to exact revision `c5975f9f89f0953b00768680357985d46178078a`, which is also Clack's `v0.2` prerelease commit containing the required re-entrancy fixes. As of 2026-09-04, crates.io still exposes 0.1.1 as the latest registry release, so the exact git revision remains the safer reproducible source. Any Clack source/revision change is an audit + conformance event.

VST3/AUv2/AUv3 should initially project through `clap-wrapper`, but wrapper output earns support only through Chassis differential tests, native validators, and real-host qualification. Native format adapters are justified only by concrete semantic or maintenance limitations.

## Validation

GitHub Actions provides distinct automated gates:

- **Rust CI**: fmt, Linux tests, strict Clippy, Loom, macOS/Windows portable tests/checks, dependency policy;
- **CLAP Conformance**: package and run pinned `clap-validator` + bounded fuzz on Linux, macOS, and Windows.

Neither substitutes for production DAW qualification or representative hardware performance measurement. Never describe an unexecuted check as passing.

## Read first

- [Architecture](docs/architecture.md)
- [Roadmap](docs/roadmap.md)
- [Current execution plan](docs/next-runtime-slice.md)
- [Open questions / freeze gates](docs/design/open-questions.md)
- [Validation strategy](docs/design/validation.md)
- [Pinned Clack audit](docs/research/clack-pinned-adapter-audit.md)
- [Parameters, automation, and state](docs/design/parameters-state.md)
- [Process buffers](docs/design/process-buffers.md)
- [Licensing](docs/licensing.md)
- [Dependency policy](docs/dependencies.md)

## Licensing

Chassis is AGPL-3.0-or-later with an intended separate commercial license for proprietary products. Dependency policy therefore considers both realtime/safety quality and commercial relicensing compatibility. Contributor/relicensing terms must be established before substantive outside contributions are accepted.
