# Chassis

Chassis is a convention-first Rust framework for realtime audio components, starting with effects and native CLAP plugins.

**Pre-alpha / work in progress.** The crates are unpublished, and the public APIs and CHSS state format are not stable. VST3, Audio Unit, and editor integration are planned.

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
| [chassis-core](crates/chassis-core) | Format-independent lifecycle, audio buffers, parameters, automation, and state |
| [chassis-clap](crates/chassis-clap) | CLAP adapter through Clack |
| [clap-conformance](examples/clap-conformance) | Example plugin with sample-accurate gain automation and typed parameters |
| [clap-delayed-probe](examples/clap-delayed-probe) | Example plugin with a fixed 64-sample delay for latency qualification |

Start with the conformance example for component definition and plugin export. Platform-specific CLAP packaging and validator commands live in the [conformance workflow](.github/workflows/clap-conformance.yml).

## Design

`Component` defines immutable identity, schema, capabilities, and factories. `InstanceRuntime<P>` owns durable parameter and semantic state and coordinates activation. `Processor` owns active DSP history. Product-facing core APIs contain no plugin-format or GUI-toolkit types.

Realtime callbacks have activation-defined resource and event bounds. The test suite checks callback allocations, lifecycle failures, concurrent state publication, f32/f64 processing, and malformed state. Recorded host and performance results, their scope, and remaining qualification work are in the [validation guide](docs/design/validation.md).

- [Architecture](docs/architecture.md)
- [Parameters, automation, and state](docs/design/parameters-state.md)
- [Process buffers](docs/design/process-buffers.md)
- [Roadmap](docs/roadmap.md)
- [Dependency policy](docs/dependencies.md)

## License

[AGPL-3.0-or-later](LICENSE), with an intended separate commercial license. Commercial terms are not yet available. Substantive outside code contributions are not accepted until contribution and relicensing terms are established; see [licensing](docs/licensing.md).
