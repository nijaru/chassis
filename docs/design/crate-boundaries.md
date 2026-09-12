# Crate and Package Boundaries

Status: design direction; `chassis-core` and the first `chassis-clap` adapter exist.

## Goal

Use crate boundaries where they isolate unsafe/FFI code, large optional dependencies, platform integrations, tooling, or test-only machinery. Do not turn every domain noun into a crate or force normal authors to understand the implementation graph.

Chassis is one audio framework, not a collection of unrelated microcrates.

## Public authoring surface

The intended ordinary surface should remain small even as the internal framework grows:

```text
chassis-audio     curated author-facing package/facade
chassis-test      author/conformance testing helpers
cargo-chassis     build/validate/package tooling
```

Optional GUI, hosting, device, media, and other integrations may be exposed through features or companion packages where dependency boundaries justify that split.

The bare crates.io package `chassis` is already occupied. `chassis-audio` can be documented with a dependency rename so source code still uses the natural path:

```toml
[dependencies]
chassis = { package = "chassis-audio", version = "..." }
```

Do not publish placeholders merely to reserve names. Publish only when a useful package exists.

## Facade versus internal graph

`chassis-core` should keep deliberate domain modules and avoid glob-exporting every implementation detail at its root.

The umbrella package should intentionally curate the concepts ordinary authors need. A plugin tutorial, standalone processor, or embedded DSP component should not begin with an explanation of the internal crate graph.

Likewise, an application using graph/device/host layers should opt into those facilities without forcing plugin-only users to build unrelated dependencies.

## Durable boundary principles

### `chassis-core`

Format-independent semantic/runtime contracts:

- component/processor lifecycle;
- audio/event ports and layouts;
- process buffers/context;
- parameters/automation/modulation;
- semantic events/transport views;
- state/migrations;
- latency/tail/bypass semantics;
- narrow realtime communication primitives;
- stable identities required by the runtime.

Properties:

- std-first;
- no plugin SDK, GUI toolkit, device backend, codec, or OS API dependency;
- unsafe denied;
- usable directly by embedded/application processors without plugin deployment.

### Deployment adapters

Examples:

```text
chassis-clap       [exists]
chassis-vst3       [only if Chassis owns a native adapter]
chassis-au         [only if Chassis owns a native adapter]
```

FFI/format glue has a different risk profile: external bindings, host lifecycle quirks, platform builds, native validators, and potential unsafe boundaries. Isolate it from core.

CLAP is the first native adapter. VST3/AU through a reviewed wrapper initially belong primarily to deployment/build integration; create native adapter crates only when Chassis actually owns those adapters.

### Editor/UI integration

Potential boundaries:

```text
chassis-editor     toolkit-independent editor/host-window/binding contract if large enough
chassis-iced       optional Iced integration
<other adapters>   only with real supported clients
```

A GUI toolkit brings substantial rendering/window dependencies. Headless users must not inherit them.

Chassis owns editor lifecycle/integration, not a widget library or product visual design.

### DSP utilities

Common DSP utilities are planned framework scope, but **do not create `chassis-dsp` merely because the domain exists**.

Start with modules/integrations where practical. Split a DSP package only when it provides a coherent optional dependency/release boundary—for example if FFT/resampling/convolution integrations would otherwise pull significant dependencies into all core users.

Broad candidates include units, ramps/smoothing, delays, metering, filters, oscillators/envelopes, FFT/STFT/window integration, oversampling/resampling, convolution helpers, and buffer/channel operations.

Strong existing FFT/resampling/etc. crates should normally remain dependencies behind Chassis semantics rather than being copied into the repository.

### Device/runtime layer

Likely boundary:

```text
chassis-device
```

Own Chassis semantics for audio/MIDI enumeration, open/close/reconfiguration, sample-rate/block-size negotiation, xruns/errors, timestamps, and device/application integration. Platform/device backend crates remain implementation dependencies.

This layer is optional for plugin-only/embedded users.

### Graph/scheduling layer

Likely boundary:

```text
chassis-graph
```

Own generic audio/event routing, validated execution plans, latency propagation/compensation, and realtime scheduling over ordinary Chassis processors.

The graph must not own DAW concepts such as tracks, clips, arrangements, project workflows, or mixer UX.

### Plugin hosting layer

Likely boundary:

```text
chassis-host
```

Own discovery/scanning, loading, lifecycle, hosted state/parameters/editors, failure policy, and graph integration for third-party plugin formats.

Hosting dependencies and FFI must not flow into `chassis-core` or ordinary plugin-authoring builds.

### Media/offline layer

A package split such as `chassis-media` or `chassis-offline` is possible only if concrete dependency/ownership boundaries justify it.

This logical area covers audio source/sink metadata, stream/read/write/seek integration, codec adapters, resampling/channel adaptation, deterministic rendering, and future random-access/multi-pass processing.

Chassis should not implement or own project media libraries, clip editing, or codec algorithms merely to create a package.

### Standalone layer

Potential boundary:

```text
chassis-standalone
```

Application bootstrap over core + device + optional editor/graph facilities. It must run the same component/runtime implementation rather than introduce a second DSP architecture.

### Test support

```text
chassis-test
```

Synthetic hosts/devices, allocation guards, property/fuzz helpers, lifecycle torture machinery, graph fixtures, and conformance utilities should not ship through every production dependency merely because products need them in tests.

### Tooling

```text
cargo-chassis
```

Workspace discovery, identity manifests, export builds, validators, packaging, signing/notarization hooks, compatibility checks, and templates. Tooling is not part of the runtime DSP dependency graph.

## What stays as modules initially

Inside `chassis-core`, prefer modules for tightly coupled semantic contracts:

- audio ports/layouts/I/O policy;
- activation/process context/buffer interfaces;
- parameters/automation/modulation;
- events/transport;
- persistent state/migrations;
- component/runtime ownership traits;
- latency/tail/bypass metadata;
- realtime communication primitives;
- stable runtime identity primitives.

Do not split state/parameters/events into separate crates merely because they are substantial concepts.

Likewise, do not create crates for `meter`, `filter`, `transport`, `offline`, etc. until there is a meaningful dependency/release boundary.

## Current / likely dependency direction

A plausible end-state graph is:

```text
product/application
        |
        v
chassis-audio              curated facade
   |        |       |       |       |
   |        |       |       |       `-> optional tooling metadata helpers
   |        |       |       `----------> optional host/media integrations
   |        |       `------------------> optional graph/device/standalone
   |        `--------------------------> optional deployment/editor integration
   `-----------------------------------> chassis-core + lightweight utilities

chassis-clap       -> chassis-core + Clack
chassis-device     -> chassis-core + reviewed device/MIDI backends
chassis-graph      -> chassis-core (+ utility modules)
chassis-host       -> chassis-core/graph + plugin-host adapters
chassis-standalone -> core + device (+ graph/editor as selected)
chassis-test       -> core (+ adapters/layers behind test features)
cargo-chassis      -> build/tooling dependencies only
```

Exact package names and edges are not frozen. Keep the graph acyclic and keep large optional/platform dependencies from flowing downward.

## Dependency integration rule

A dependency should be wrapped behind Chassis semantics when:

- its API exposes backend/platform concepts that should not become framework compatibility identity;
- Chassis needs stronger realtime/lifecycle guarantees than the dependency alone expresses;
- multiple backends should present one semantic capability;
- Chassis needs to constrain configuration, ownership, or failure behavior.

Do not wrap dependencies merely to hide their names when direct use is already the best stable abstraction.

## Features

Use Cargo features for genuinely optional integration, not to create a combinatorial semantic matrix.

Rules before publication:

- default features should give the most common author experience without pulling unrelated GUI/device/host/media stacks;
- disabling defaults must not silently change compatibility identity/state semantics;
- release feature combinations are validated explicitly;
- avoid target-dependent public APIs where a runtime/adapter capability query is more honest;
- graph/host/device/media functionality should normally be opt-in for plugin-only users.

## Versioning

Pre-alpha internal crates can version together. Once external users exist, closely coupled framework crates should normally keep synchronized compatible releases so authors are not solving an internal dependency puzzle.

`cargo-chassis` can eventually detect incompatible mixed framework versions in a product workspace.

## User-experience rule

Internal modularity serves safety, optionality, build times, and maintenance. It is not the user model.

If an ordinary plugin author must understand graph/device/media crates to write an effect, the facade is wrong. If a DAW/audio-application author must bypass Chassis core and invent another processor model to use graph/devices/hosting, the architecture is wrong.
