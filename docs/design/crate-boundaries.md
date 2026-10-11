# Package Boundaries

## Current packages

| Package | Responsibility | Boundary reason |
| --- | --- | --- |
| `chassis-runtime` | Format-independent component schema, instance lifecycle, processing, parameters, state, audio/events, and bounded telemetry | Usable without a plugin ABI or platform dependency; safe Rust, currently std-only |
| `chassis-clap` | CLAP metadata, host lifecycle, buffer/event translation, and cross-thread state projection | Clack/CLAP dependency and deployment-specific lifetimes, failure handling, and qualification |
| `chassis-clap-conformance` | Exported automation/state qualification fixture | Test artifact, not an authoring library |
| `chassis-clap-delayed-probe` | Exported latency/PDC qualification fixture | Test artifact, not an authoring library |

`chassis-runtime` replaces the unpublished `chassis-core` package. The old name described its place in a dependency graph; the new name describes what callers use. The rename adds no layer, facade, or compatibility alias.

The runtime owns both immutable semantic definitions and the durable instance that executes them. Keeping these together avoids splitting tightly coupled schema, state, and lifecycle guarantees across packages. Domain modules remain explicit; root exports should be curated only when an actual authoring workflow benefits.

## Rules for new boundaries

Create a package when it isolates an actual concern:

- unsafe/FFI code or deployment-specific lifetimes;
- a substantial optional dependency or platform backend;
- independently usable functionality with a coherent release contract;
- tooling or test machinery that should not enter production runtime dependencies.

A domain noun, long source file, roadmap item, or possible future feature is not by itself a package boundary. Refactor cohesive modules before adding crates. Do not make normal authors understand the implementation graph, but do not hide useful boundaries behind an empty umbrella package either.

The only current production edge is:

```text
export/application -> chassis-runtime
CLAP export        -> chassis-clap -> chassis-runtime
                                  -> Clack
```

## Future integrations

Editor toolkit, device, hosted-plugin, codec, and heavy DSP dependencies must remain optional and must not flow into the format-independent runtime. Graph/scheduling belongs above ordinary component/runtime semantics; it must not introduce a second processor model or DAW document concepts.

VST3/AU projection may use a reviewed wrapper. A native adapter package is justified only when Chassis owns its implementation. A wrapper build target is not a Rust package simply because it exports another format.

Common DSP utilities may start as modules. Split them when independent use or optional FFT/resampling/convolution dependencies provide a real boundary. Use mature dependencies rather than duplicating their algorithms merely to fill a package.

Standalone bootstrap composes runtime, device, and optional editor/graph capabilities; it does not justify a parallel DSP runtime. Media/offline processing similarly composes existing semantics until a concrete random-access or multi-pass requirement demands another contract.

Test helpers and build/validation/packaging tools should be shared only once repeated clients demonstrate a maintained interface. No test-support crate or Cargo subcommand is required by the current workspace.

## Author-facing packaging

There is no umbrella package today. Its need, exports, package name, and feature model are unresolved, not an accepted future architecture. Choose them from executable authoring examples before publication, checking registry availability at that time. Do not publish placeholders or lock in dependency aliases to work around a hypothetical name conflict.

Features should select optional integrations, not change persistence identity or silently alter semantic guarantees. Validate supported combinations explicitly. Closely coupled packages may release together without requiring every future tool or integration to share a version.
