# Chassis

Chassis is a convention-first Rust framework for professional realtime audio software.

The first production surface is audio plugins, beginning with effects. The core component model is intentionally usable for instruments, standalone applications, and embedded processors without rewriting product DSP/state. Optional hosting/device/graph layers may later support larger audio applications without making DAW/project semantics part of Chassis core.

Status: **private pre-alpha design and implementation**. Nothing in `chassis-core` is a stable public API yet.

## Direction

Chassis aims to make the normal professional-plugin path mostly product code while preserving explicit ownership, realtime, and compatibility semantics.

Current principles:

- convention over configuration with lower-level escape hatches;
- one authority for each mutable state/lifecycle guarantee;
- exclusive mutable realtime `Processor` state while active;
- strict allocation/blocking/work rules on deterministic realtime paths, not the whole codebase;
- stable human-readable product/parameter/port identities separated from backend IDs/runtime indices;
- stereo main I/O + optional inactive stereo sidechain as the default effect convention, not a core limitation;
- whole-component I/O negotiation with room for instruments, multiple buses, surround, ambisonics, and immersive channel beds;
- format-independent persistent state and automation semantics;
- no hidden requirement for a particular GUI toolkit;
- evidence-driven adapters and performance work rather than assuming a framework/backend is correct because it builds.

Target desktop platforms are macOS, Windows, and Linux where each export format applies. Initial format work is CLAP first, then VST3 and Audio Unit. AAX is later/conditional because it also carries Avid/PACE licensing and distribution requirements.

## Backend strategy

The current preferred first implementation uses [Clack](https://github.com/prokopyl/clack) at the low-level CLAP boundary. [clap-wrapper](https://github.com/free-audio/clap-wrapper) is the preferred initial route to VST3/AUv2/AUv3 while each projection passes Chassis's own semantic/host validation.

These are implementation dependencies, not the product-facing semantic model. Chassis core does not expose CLAP/VST3/AU/Clack/clap-wrapper types, and a wrapper can be replaced if lifecycle/correctness/capability evidence requires it.

## Default effect convention

An ordinary effect that does not specify custom I/O conceptually gets:

```text
audio.main.in    stereo required
audio.main.out   stereo required
audio.sidechain  stereo optional/inactive
```

Unused optional facilities should not impose process-time work.

## Read first

- [Architecture](docs/architecture.md)
- [Roadmap](docs/roadmap.md)
- [Open questions / API freeze gates](docs/design/open-questions.md)
- [Licensing](docs/licensing.md)
- [Dependency/license policy](docs/dependencies.md)

Detailed contracts:

- [Runtime ownership](docs/design/runtime.md)
- [Audio ports/layout negotiation](docs/design/audio-ports.md)
- [Processing buffers](docs/design/process-buffers.md)
- [Parameters/automation/state](docs/design/parameters-state.md)
- [State format](docs/design/state-format.md)
- [Events/notes/MIDI/transport](docs/design/events-transport.md)
- [Common runtime services](docs/design/runtime-services.md)
- [Product/export identity](docs/design/identity-metadata.md)
- [Authoring/convention model](docs/design/authoring-api.md)
- [Crate/package boundaries](docs/design/crate-boundaries.md)
- [Validation/conformance](docs/design/validation.md)

## Current workspace

```text
crates/
  chassis-core/
    src/
      audio.rs      stable port keys, basic layouts/configuration validation
      buffer.rs     safe exact-alias/disjoint/input-only/output-only channel views
      process.rs    activation bounds + borrowed per-call ProcessBlock
      runtime.rs    explicit Component/Processor/Process lifecycle shell
    tests/
      conformance.rs deterministic external-API lifecycle/buffer tests
```

The first executable runtime slice deliberately stops before parameter/state authority, transport/events, background work, or format adapters. `Processor` owns reset/lifecycle semantics; sample processing is a separate `Process<S>` capability so an eventual f64 path does not require a second processor architecture.

`chassis-core` is currently unpublished `0.0.0`, std-only, and has no third-party Rust dependencies. Its current code is an implementation spike and can change freely before publication.

Future crates such as `chassis-clap`, `chassis-gui`, `chassis-iced`, `chassis-test`, `chassis-standalone`, and `cargo-chassis` are created only when an executable requirement proves the boundary. The public authoring experience should still feel like one framework rather than exposing an internal crate graph.

## Validation

There is currently **no authoritative hosted CI**. Validation is local/tool-driven until hosted automation is intentionally restored.

Baseline commands on a supported development machine:

```sh
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-features --all-targets -- -D warnings
cargo deny check
```

Add `cargo machete` once third-party dependencies exist. Use Miri/model tests/sanitizers/native validators as the relevant unsafe/adapters are implemented. Do not describe an unexecuted check as passing.

## Licensing

Chassis is **AGPL-3.0-or-later**. The intended long-term model also offers a separate commercial license for proprietary software that incorporates Chassis without accepting AGPL copyleft obligations.

AGPL users may experiment, modify, distribute, and sell AGPL-compliant open-source products. A vendor wanting to distribute a proprietary/closed-source product incorporating Chassis would use the separate commercial license.

Commercial terms are intentionally undefined during private pre-alpha. Before accepting substantive external code contributions, Chassis will establish contributor terms that preserve commercial relicensing rights.

Third-party dependencies keep their own licenses. `deny.toml` and [docs/dependencies.md](docs/dependencies.md) define the conservative dependency policy; checks are currently run locally rather than by GitHub Actions.
