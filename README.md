# Chassis

Chassis is a convention-first Rust framework for professional realtime audio components.

The same format-independent component/runtime model is intended to support effects, instruments, event processors, embedded processors, plugins, and standalone applications. Native CLAP is the first qualified plugin surface; VST3, Audio Unit, editor integration, event/instrument support, standalone/device deployment, and graph infrastructure are active completion work.

**Pre-alpha / work in progress.** The crates are unpublished, and the public APIs and CHSS state format are not stable.

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
| [chassis-core](crates/chassis-core) | Format-independent lifecycle, audio buffers, parameters, automation, state, and realtime communication contracts |
| [chassis-clap](crates/chassis-clap) | CLAP adapter through Clack |
| [clap-conformance](examples/clap-conformance) | Focused parameter/automation/state conformance component |
| [clap-delayed-probe](examples/clap-delayed-probe) | Fixed-delay component for latency/PDC qualification |

The framework roadmap now uses focused conformance fixtures rather than migration of an existing plugin as the completion gate. Existing product ports remain useful regression evidence where they exercise framework semantics.

## Design

`Component` defines immutable schema, capabilities, and factories. `InstanceRuntime<P>` owns durable parameter and semantic state and coordinates activation. `Processor` owns active DSP history. Product-facing core APIs contain no plugin-format or GUI-toolkit types.

Realtime callbacks have activation-defined resource and event bounds. The test suite checks callback allocations, lifecycle failures, concurrent state publication, f32/f64 processing, malformed state, and other runtime invariants. `chassis-core::telemetry` provides the first reusable bounded DSP-to-control snapshot primitive without adding locks or callback allocation.

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
