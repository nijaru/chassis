# Chassis

Chassis is a Rust framework for building audio software.

Its format-independent component/runtime model is intended to support DSP components, effects, instruments, event processors, plugins, standalone applications, plugin hosts, audio engines, offline processing, and embedded use. Plugins are the first and likely most common authoring/deployment path, but plugin-host assumptions do not define the core model.

Chassis owns broadly reusable audio infrastructure. Applications own their product, document, workflow, and visual semantics. A DAW should be able to use Chassis for its audio engine, devices, processing graph, plugin hosting, rendering, and processor infrastructure without Chassis defining tracks, clips, arrangements, session views, project UX, or editing workflows.

**Pre-alpha / work in progress.** The crates are unpublished, and the public APIs and CHSS state format are not stable. Pre-alpha APIs may be replaced aggressively when doing so produces a simpler, safer, more general design.

## Build and test

Install [Rust through rustup](https://rustup.rs/), then run from the repository root:

```sh
cargo test --workspace --locked
cargo build --release --workspace --locked
```

The repository selects the stable Rust toolchain and Edition 2024. CLAP dependencies use an [audited, pinned Clack revision](docs/research/clack-pinned-adapter-audit.md).

## Workspace

| Crate | Purpose |
| --- | --- |
| [chassis-core](crates/chassis-core) | Format-independent lifecycle, audio/event ports, process buffers, parameters, automation, state, and realtime communication contracts |
| [chassis-clap](crates/chassis-clap) | CLAP deployment adapter through Clack |
| [clap-conformance](examples/clap-conformance) | Focused parameter/automation/state conformance component |
| [clap-delayed-probe](examples/clap-delayed-probe) | Fixed-delay component for latency/PDC qualification |

Planned framework layers include common DSP utilities, editor integration, VST3/Audio Unit deployment, audio/MIDI devices, standalone deployment, graph/scheduling, plugin hosting, media/offline processing integration, and release/conformance tooling. Package boundaries are created only where dependency, safety, optionality, or release boundaries justify them.

The framework roadmap uses focused conformance fixtures rather than migration of an existing plugin as the completion gate. Existing product ports remain useful regression evidence where they exercise framework semantics.

## Design

`Component` defines immutable schema, capabilities, and factories. `InstanceRuntime<P>` owns durable parameter and semantic state and coordinates activation. `Processor` owns active DSP history. The same processor model should remain usable through plugin adapters, directly inside another Rust application, as a graph node, in a standalone application, and during offline rendering.

Product-facing core APIs contain no plugin-format, GUI-toolkit, device-backend, or operating-system types. Realtime callbacks have activation-defined resource and event bounds. The test suite checks callback allocations, lifecycle failures, concurrent state publication, f32/f64 processing, malformed state, and other runtime invariants. `chassis-core::telemetry` provides the first reusable bounded DSP-to-control snapshot primitive without adding locks or callback allocation.

Chassis may provide common DSP building blocks where doing so creates a coherent reusable audio foundation, but it should use strong existing Rust libraries where appropriate instead of reimplementing codecs, FFTs, resamplers, device backends, or other mature primitives merely for ownership.

Recorded host and performance results, their scope, and remaining qualification work are in the [validation guide](docs/design/validation.md).

- [Architecture](docs/architecture.md)
- [Roadmap / completion bar](docs/roadmap.md)
- [Current execution plan](docs/next-runtime-slice.md)
- [Parameters, automation, and state](docs/design/parameters-state.md)
- [Events, notes, MIDI, and transport](docs/design/events-transport.md)
- [Process buffers](docs/design/process-buffers.md)
- [Open questions / freeze gates](docs/design/open-questions.md)
- [Dependency policy](docs/dependencies.md)

## License

[AGPL-3.0-or-later](LICENSE), with an intended separate commercial license. Commercial terms are not yet available. Substantive outside code contributions are not accepted until contribution and relicensing terms are established; see [licensing](docs/licensing.md).
