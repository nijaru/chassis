# Crate and Package Boundaries

Status: design direction; only `chassis-core` exists today.

## Goal

Keep Chassis modular where module boundaries improve safety, dependency isolation, build times, or optional capabilities without turning the framework into a large collection of tiny crates that users must understand individually.

A normal plugin author should interact with a small public surface.

## Public authoring surface

The intended primary crates/packages are approximately:

```text
chassis-audio          umbrella author-facing package/crate
chassis-iced           optional first-class Iced GUI integration
chassis-test           author/conformance test support
cargo-chassis          build/validate/package CLI
```

The bare crates.io package name `chassis` is already occupied. The project can publish the umbrella package as `chassis-audio` while documenting a dependency rename so product code can still use the natural Rust path `chassis::...`:

```toml
[dependencies]
chassis = { package = "chassis-audio", version = "..." }
```

Whether the library target itself is named `chassis` or the rename is purely a consumer convention should be tested before publication. Do not publish a placeholder package merely to reserve names.

## Internal/framework crates

Likely durable implementation boundaries:

```text
chassis-core
  format-independent semantic/runtime contracts
  no format SDK or GUI toolkit dependencies
  unsafe denied

chassis-clap
  Clack/CLAP adapter
  format translation and necessary FFI/unsafe isolation

chassis-derive
  optional proc macros once manual APIs are proven

chassis-gui
  toolkit-independent editor/host-window and parameter-binding contract

chassis-iced
  Iced/wgpu implementation of the GUI contract

chassis-test
  conformance host/component, allocation checks, fixtures, property helpers

chassis-standalone
  product-grade standalone component host/device runtime

cargo-chassis
  workspace discovery, format builds, identity manifests, validation, packaging
```

This is an upper-bound direction, not a requirement to create every crate immediately.

## Why some boundaries deserve crates

### `chassis-core`

Keeps the product-facing semantic model compilable/testable without CLAP/VST3/AU, GUI, device, or packaging dependencies. It should remain useful for embedded/standalone/application components as well as plugins.

### Format adapter

Format adapter code has fundamentally different constraints:

- external SDK/binding dependencies;
- FFI/unsafe code;
- host-specific lifecycle quirks;
- platform-specific builds;
- native validators.

Keeping it outside `chassis-core` lets the core preserve `unsafe_code = deny` and prevents backend types from leaking into product APIs.

CLAP is initially the native plugin boundary, so one `chassis-clap` crate is enough. VST3/AU projections through `clap-wrapper` belong primarily to build/tooling integration until Chassis has evidence that native Rust adapters are worth owning.

Do not create empty `chassis-vst3` or `chassis-au` crates simply because those formats exist.

### Proc macros

A proc-macro crate is mechanically required if Chassis introduces derives/attributes. Keep macro expansion thin: it should generate calls/implementations against documented semantic APIs rather than contain hidden framework behavior.

### GUI toolkit

Iced brings a substantial dependency/rendering/windowing tree that headless products and tests should not pay for. The generic editor contract and a toolkit implementation therefore deserve separation.

### Test support

Allocation guards, synthetic host machinery, golden fixtures, and property/fuzz helpers should not ship in normal release plugin binaries merely because they are framework-owned.

## What should remain modules

Do not automatically split common semantic areas into crates.

Unless concrete dependency/build constraints prove otherwise, these belong as modules inside `chassis-core`:

- audio ports/layouts;
- process configuration/buffers;
- events/transport;
- parameters/automation;
- state/migrations;
- component/runtime ownership;
- latency/tail/status/bypass semantics;
- identity primitives needed at runtime.

Likewise, common format translation helpers can remain modules inside their adapter instead of separate crates.

## Optional common DSP/utilities

If Chassis later adds broadly useful helpers such as smoothing, meters, analyzer transport, FFT transport, or voice-management primitives, prefer placing lightweight utilities in the most relevant existing crate first.

A separate `chassis-dsp`-style crate is justified only if a coherent optional library emerges with dependencies/release cadence clearly distinct from the core framework.

Chassis is not trying to own all DSP algorithms.

## Hosting/application roadmap

Potential future application-side crates remain conditional:

```text
chassis-host
chassis-device
chassis-graph
```

These should be added only when an actual host/application client exists. They are not prerequisites for plugin v0.1.

`chassis-host` may reuse Clack host support but must not force plugin-authoring code to depend on hosting functionality.

## Dependency direction

Keep dependencies acyclic and oriented from high-level optional surfaces toward the semantic core:

```text
product
   ↓
chassis-audio ───────────────┐
   ↓                         │
chassis-core                 │
                             │
chassis-clap ────────────────┘
   ↓
Clack

chassis-iced -> chassis-gui -> chassis-core
chassis-test ----------------> chassis-core (+ adapters as test features)
chassis-standalone ----------> chassis-core (+ GUI/device deps)
```

The exact umbrella-to-adapter dependency arrangement should avoid circularity and should allow a headless `chassis-core` consumer without plugin-format code.

## Release/versioning

Early pre-alpha crates can version together to simplify development. Once external users exist, keep closely coupled framework crates on synchronized compatible versions unless there is a compelling reason to version independently.

The CLI should verify that a workspace is not accidentally mixing incompatible Chassis crate versions.

## User-experience rule

If ordinary documentation needs to teach authors the internal crate graph before they can make a gain plugin, the boundaries are wrong.

Internal modularity exists to improve implementation and optionality. The public authoring story should still read as one framework.
