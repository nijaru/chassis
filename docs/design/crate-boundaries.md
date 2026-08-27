# Crate and Package Boundaries

Status: design direction; only `chassis-core` exists today.

## Goal

Use crate boundaries where they isolate unsafe/FFI code, large optional dependencies, tooling, or test-only machinery. Do not turn every domain noun into a crate or force normal authors to understand the implementation graph.

## Public authoring surface

The intended ordinary surface is small:

```text
chassis-audio     main author-facing package/facade
chassis-iced      optional first-class Iced integration
chassis-test      author/conformance testing helpers
cargo-chassis     build/validate/package tooling
```

The bare crates.io package `chassis` is already occupied. `chassis-audio` can be documented with a dependency rename so source code still uses the natural path:

```toml
[dependencies]
chassis = { package = "chassis-audio", version = "..." }
```

Do not publish placeholders merely to reserve names. Publish only when a useful initial package exists.

## Facade versus accidental re-export

`chassis-core` should keep deliberate domain modules and avoid glob-exporting every implementation detail at its root.

That does **not** mean the umbrella package must expose the internal crate graph. `chassis-audio` may deliberately re-export the small author-facing semantic traits/types and enable selected plugin/export integration behind documented features when that produces the best user experience.

The distinction is:

- internal crate roots avoid accidental API expansion;
- the public facade intentionally curates the API most plugin authors should use.

A gain/EQ/compressor tutorial should not begin with an explanation of `chassis-core` versus `chassis-clap`.

## Likely durable internal boundaries

```text
chassis-core
  format-independent semantic/runtime contracts
  std-first, no format SDK or GUI toolkit
  unsafe denied

chassis-clap
  Clack/CLAP adapter
  host translation + isolated necessary FFI/unsafe

chassis-derive
  proc macros only after explicit APIs prove repetitive declarations

chassis-gui
  toolkit-independent editor/host-window/parameter-binding contract
  only if this contract proves substantial enough to deserve a crate

chassis-iced
  Iced/wgpu integration

chassis-test
  conformance runtime/component, allocation/work guards, fixtures, property helpers

chassis-standalone
  standalone component/device host once that product surface is implemented

cargo-chassis
  workspace discovery, export builds, identity manifests, validators, packaging
```

This is an upper bound, not a checklist. A proposed crate must have a dependency/safety/release/test boundary that a module cannot express as well.

## Why some boundaries deserve crates

### Core

Core remains testable/usable without plugin SDKs, GUI, devices, or packaging. Embedded/application components can depend on it without inheriting export dependencies.

### CLAP adapter

Format glue has a different risk profile: FFI/unsafe, external bindings, host lifecycle quirks, platform builds, native validators. Isolate it so core can forbid unsafe code and product semantics cannot accidentally depend on backend types.

CLAP is the first native adapter. VST3/AU through `clap-wrapper` initially belong primarily to export/build integration; create native `chassis-vst3`/`chassis-au` crates only if Chassis actually owns those adapters later.

### Proc macros

Rust requires a proc-macro crate mechanically. Macro expansions stay thin and target documented explicit Chassis APIs; the macro crate does not become a hidden second runtime.

### GUI

A production GUI toolkit brings a large dependency/rendering/window stack. Headless products/core users should not compile it. Whether a separate `chassis-gui` crate is needed versus keeping a small editor contract in the facade/core should be decided by implementation size and dependency pressure, not aesthetics.

### Test support

Synthetic hosts, global allocation guards, fuzz/property fixtures, and lifecycle torture machinery should not ship merely because product code uses Chassis.

## What stays as modules initially

Inside `chassis-core`, prefer modules for tightly coupled semantic contracts:

- audio ports/layouts/I/O policy;
- activation/process context/buffer interfaces;
- parameters/automation;
- events/transport;
- persistent state/migrations;
- component/runtime ownership traits;
- latency/tail/bypass metadata when implemented;
- stable identity primitives needed at runtime.

Do not split state/parameters/events into separate crates simply because they are sizable concepts.

## Common utilities

Smoothing, metering, analyzer transport, snapshot publication, voice helpers, FFT/resampling helpers, etc. can begin in the product or appropriate existing framework module.

Extract a new optional crate only when repeated clients reveal one coherent capability with meaningfully different dependencies/release cadence. Chassis should not become a generic DSP algorithm collection by default.

## Hosting/application layers

Conditional future layers may include:

```text
chassis-host
chassis-device
chassis-graph
```

They appear only with a real host/application client. Plugin authors do not inherit hosting/device/graph dependencies through the normal facade unless explicitly requested.

## Dependency direction

Conceptually:

```text
product
   ↓
chassis-audio (curated facade)
   ├── chassis-core
   ├── optional chassis-clap/export support
   └── optional derive support

chassis-clap       -> chassis-core + Clack
chassis-iced       -> editor contract/core + Iced
chassis-test       -> core (+ adapters behind test features)
chassis-standalone -> core + device/window deps
cargo-chassis      -> build/tooling metadata; not runtime DSP
```

Keep the graph acyclic. Large GUI/device/tooling dependencies must not flow downward into `chassis-core`.

## Features

Use Cargo features for genuinely optional integration, not to create a combinatorial semantic matrix.

Rules before publication:

- default features should give the most common author experience without pulling obviously unrelated GUI/device/tooling stacks;
- disabling defaults must not change compatibility identity/state semantics silently;
- feature combinations used for releases are validated explicitly;
- avoid target-dependent public APIs where a runtime/adapter capability query is more honest.

## Versioning

Pre-alpha internal crates can version together. Once external users exist, closely coupled framework crates should normally keep synchronized compatible releases so authors are not solving an internal dependency puzzle.

`cargo-chassis` can detect incompatible mixed framework versions in a product workspace.

## User-experience rule

Internal modularity serves safety, optionality, build times, and maintenance. If ordinary plugin documentation has to teach the crate graph before an author can write product DSP, the public boundary is wrong.
