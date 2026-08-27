# Chassis

Chassis is a convention-first Rust framework for professional audio software.

The initial product surface is audio plugins: effects first, instruments next. The core model is intentionally broader than a plugin ABI so the same audio component can later run as a plugin, standalone application, or embedded processor without rewriting product DSP and state.

Status: **pre-alpha / private design and implementation**.

## Goals

- Make the common case for professional plugins require very little framework plumbing.
- Encode realtime and thread-ownership constraints structurally where Rust can enforce them.
- Keep product DSP, visual design, and application semantics owned by the product.
- Provide conventions with escape hatches rather than forcing one DSP or UI architecture.
- Support macOS, Windows, and Linux.
- Target CLAP, VST3, and Audio Unit; CLAP is the native plugin boundary initially.
- Design ports and channel layouts so stereo is easy now without blocking surround, ambisonics, or immersive layouts later.
- Keep effects and instruments compatible with the same core model.

The default effect convention is stereo main input/output with an optional stereo sidechain. That is a convenience, not a core architectural limitation.

## Architecture direction

A Chassis product is fundamentally an audio component with processing, parameters, state, events, ports, and optional editor/controller capabilities. Deployment adapters expose that component as a plugin, standalone application, or embedded processor.

The initial backend direction is [Clack](https://github.com/prokopyl/clack) for the safe low-level CLAP boundary and [clap-wrapper](https://github.com/free-audio/clap-wrapper) for VST3/AU projection while those wrappers meet Chassis's validation requirements. Backend types must not leak into the product-facing core API.

See [docs/architecture.md](docs/architecture.md) and [docs/roadmap.md](docs/roadmap.md).

## Workspace

```text
crates/
  chassis-core/        format-independent component/runtime contracts

docs/
  architecture.md
  roadmap.md
  licensing.md
```

Additional crates such as `chassis-clap`, `chassis-gui`, `chassis-iced`, `chassis-test`, `chassis-standalone`, and `cargo-chassis` are planned when their boundaries are proven.

## Licensing

Chassis is currently licensed under **AGPL-3.0-or-later**. The intended long-term model is dual licensing: AGPL for open-source use, with a separate commercial license for proprietary software that incorporates Chassis without accepting the AGPL obligations.

Commercial terms are not defined yet. Before accepting substantive outside contributions, Chassis will establish contributor terms that preserve the ability to offer commercial licenses.

See [docs/licensing.md](docs/licensing.md).
