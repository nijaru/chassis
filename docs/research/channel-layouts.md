# Channel layout research

Status: active design research. This document constrains the eventual `chassis-core` layout API; the current `0.0.0` types are a spike, not a frozen public contract.

## Why this needs an explicit model

Stereo must be effortless, but a core API that assumes two channels or encodes one host's speaker representation will become a migration problem for surround, immersive audio, ambisonics, instruments with multiple outputs, and applications embedding Chassis components.

The common abstraction appears to be:

```text
port
├── stable identity + role
├── direction
├── channel count
└── layout semantics
    ├── simple mono/stereo
    ├── speaker/surround map
    ├── ambisonic representation
    ├── discrete/arbitrary channels
    └── extensible/custom type
```

The layout semantics should be distinct from the number of buses/ports.

## CLAP

CLAP's core `audio-ports` extension describes stable input/output ports, channel count, and a `port_type`. Mono and stereo are built-in conventional port types. Surround and ambisonic layouts are represented through dedicated extensions rather than by pretending every channel is an ordinary stereo-like speaker channel.

CLAP 1.2 stabilized the surround, ambisonic, configurable-audio-ports, audio-ports-activation, and related extensions. Configurable audio ports are particularly relevant because immersive-capable plugins may need the host to request a supported configuration while the plugin is inactive.

Sources:

- https://github.com/free-audio/clap/blob/main/include/clap/ext/audio-ports.h
- https://github.com/free-audio/clap/blob/main/include/clap/ext/surround.h
- https://github.com/free-audio/clap/blob/main/include/clap/ext/ambisonic.h
- https://github.com/free-audio/clap/blob/main/include/clap/ext/configurable-audio-ports.h
- https://github.com/free-audio/clap/blob/main/ChangeLog.md

## VST3

VST3 uses `SpeakerArrangement` values composed from semantic speaker positions and also defines many conventional arrangements. Current definitions include modern immersive layouts such as 5.x.4, 7.1.4, 7.1.6, 9.1.4, and 9.1.6, plus higher-order ambisonic arrangements using ACN ordering and SN3D normalization through seventh order.

This is evidence that Chassis should preserve semantic layout information rather than only a channel count.

Source:

- https://steinbergmedia.github.io/vst3_doc/vstinterfaces/group__speakerArrangements.html

## Apple Core Audio / Audio Unit

Core Audio exposes explicit channel-layout tags and channel labels. Current layout tags include Atmos 5.1.2, 5.1.4, 7.1.2, 7.1.4, and 9.1.6, as well as traditional surround and ambisonic layouts.

Apple's Atmos 7.1.4 and 9.1.6 definitions also demonstrate that naming conventions differ across ecosystems; adapter translation should therefore be centralized rather than leaking Apple labels into product code.

Sources:

- https://developer.apple.com/documentation/coreaudiotypes/audio-channel-layout-tags
- https://developer.apple.com/documentation/technotes/tn3190-usb-audio-device-design-considerations

## Chassis implications

1. **Stereo is a convention, not the representation.** `effect()` may default to stereo main I/O and an optional stereo sidechain, but the processing/descriptor model must accept arbitrary ports and layouts.
2. **Do not freeze a closed speaker enum prematurely.** Rust enums are not externally extensible unless the API explicitly plans for that. The final model should leave room for future speaker positions and third-party/custom layout kinds.
3. **Treat ambisonics distinctly.** Order, channel ordering, and normalization are meaningful metadata and should not be inferred from a generic array of channel names.
4. **Represent discrete channels.** Some processors care only about N independent channels and should not be forced to invent speaker semantics.
5. **Allow configuration negotiation.** Some hosts/formats support changing layouts only while inactive; this must fit the lifecycle model.
6. **Adapters own translation.** Chassis layout types should be format-neutral; `chassis-clap`, VST3/AU projection, and future native adapters translate to host-specific arrangements.
7. **Test mappings explicitly.** Atmos/surround support is not complete merely because the core can hold 12 or 16 channels. Each advertised layout needs adapter mapping and host validation.

## Current implementation position

The first `chassis-core` channel types exist only to establish that the API is not stereo-shaped. Before publication, replace or refine them based on a dedicated layout API design pass covering:

- extensible speaker positions;
- layout kind/type identity;
- speaker maps;
- ambisonic order/ordering/normalization;
- discrete layouts;
- configuration negotiation;
- in-place pairing and sample precision capabilities where those belong at the port level.
