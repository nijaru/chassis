# Chassis

Chassis is a convention-first Rust framework for professional realtime audio software.

The first production surface is audio plugins, beginning with effects. The core component model is intentionally usable for instruments, standalone applications, and embedded processors without rewriting product DSP/state. Optional hosting/device/graph layers may later support larger audio applications without making DAW/project semantics part of Chassis core.

Status: **private pre-alpha design and implementation**. Nothing in `chassis-core` or `chassis-clap` is a stable public API yet.

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

The first adapter uses the published Clack 0.1.1 release at the low-level CLAP boundary. Clack's repository is already developing 0.2.0, but that version is not currently published on crates.io, so Chassis deliberately stays on the reproducible registry release rather than introducing a git dependency solely for unreleased API changes. [clap-wrapper](https://github.com/free-audio/clap-wrapper) remains the preferred initial route to VST3/AUv2/AUv3 once the native CLAP semantic path is qualified.

Clack/CLAP types remain outside `chassis-core`. The initial `chassis-clap` code uses Clack's safe audio API and owns no raw CLAP pointer dereference itself. Backend qualification still requires local build/test, CLAP validation, lifecycle stress, and real-host testing.

See [the Clack 0.1.1 adapter audit](docs/research/clack-0.1.1-adapter-audit.md).

## Default effect convention

An ordinary effect that does not specify custom I/O conceptually gets:

```text
audio.main.in    stereo required
audio.main.out   stereo required
audio.sidechain  stereo optional/inactive
```

Unused optional facilities should not impose process-time work.

The first CLAP proof intentionally exports only the required stereo main pair. Sidechain/multibus/configurable-layout export is a later adapter gate rather than a hidden limitation in `chassis-core`.

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

  chassis-clap/
    src/lib.rs      first Clack lifecycle + stereo/f32 buffer translation proof

examples/
  clap-conformance/
    src/lib.rs      exported deterministic gain component for CLAP qualification
```

The first CLAP slice maps factory/main-thread construction, activation, f32 stereo processing, reset, and deactivation onto the existing Chassis runtime. It deliberately stops before parameters/state, transport/events, render mode, f64, sidechains/multibus, GUI, or packaging.

`chassis-core` remains std-only. `chassis-clap` is the first crate with third-party dependencies and pins published Clack 0.1.1 exactly while the adapter contract is being qualified.

## Validation

There is currently **no authoritative hosted CI**. Validation is local/tool-driven until hosted automation is intentionally restored.

Baseline commands on a supported development machine:

```sh
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-features --all-targets -- -D warnings
cargo deny check
cargo machete
```

The current CLAP slice was authored in an environment without a Rust toolchain, so it must not be described as green until those commands regenerate/review `Cargo.lock` and pass locally. After that, build/package the conformance export and run CLAP-native validation before treating the adapter as qualified.

Use Miri/model tests/sanitizers/native validators as the relevant unsafe/adapters are implemented. Do not describe an unexecuted check as passing.

## Licensing

Chassis is **AGPL-3.0-or-later**. The intended long-term model also offers a separate commercial license for proprietary software that incorporates Chassis without accepting AGPL copyleft obligations.

AGPL users may experiment, modify, distribute, and sell AGPL-compliant open-source products. A vendor wanting to distribute a proprietary/closed-source product incorporating Chassis would use the separate commercial license.

Commercial terms are intentionally undefined during private pre-alpha. Before accepting substantive external code contributions, Chassis will establish contributor terms that preserve commercial relicensing rights.

Third-party dependencies keep their own licenses. `deny.toml` and [docs/dependencies.md](docs/dependencies.md) define the conservative dependency policy; checks are currently run locally rather than by GitHub Actions.
