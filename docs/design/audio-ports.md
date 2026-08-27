# Audio Ports and Layout Negotiation

Status: design direction; public API not frozen.

## Goal

Make stereo effects nearly configuration-free while preserving a coherent path to instruments, auxiliary buses, surround, ambisonics, immersive channel layouts, and hosts that can change layouts while a component is inactive.

The core model must express Chassis semantics. CLAP, VST3, Audio Unit, and platform-specific layout identifiers belong in adapters.

## Evidence from target formats

The target APIs do not share one channel-layout representation:

- CLAP describes ports separately from optional surround and ambisonic semantics. Its configurable-audio-ports extension lets the host push a set of port configuration requests which are accepted or rejected atomically while the plugin is inactive.
- VST3 represents speaker arrangements and lets the host propose input/output bus arrangements to the processor as a configuration.
- Core Audio can identify predefined layouts by tag or describe channels explicitly. Apple defines channel-based Atmos layouts such as 5.1.2, 7.1.4, and 9.1.6.

Therefore Chassis should model the semantic configuration and let each adapter translate or reject it rather than copying one backend's representation.

## Separate identity from configuration

A port has stable product identity independent of its current channel layout.

Conceptually:

```text
AudioPortDescriptor
├── stable id
├── display name
├── direction
├── role
└── activation/configuration capabilities

AudioPortConfiguration
├── port id
└── channel layout
```

Changing stereo to 5.1 must not create a new parameter/state identity for the same conceptual main output.

## Whole-component I/O configuration

Layout negotiation should operate on a coherent component configuration rather than mutating independent ports one at a time.

Conceptually:

```text
AudioIoConfiguration
├── inputs[]
│   └── port id + active layout
└── outputs[]
    └── port id + active layout
```

A proposed configuration is validated as a whole and committed atomically while processing is inactive. This allows products to enforce relationships such as:

- main input and output must use matching layouts;
- an instrument may have no audio input;
- a sidechain may be available only for particular layouts;
- an instrument may expose multiple differently-sized output buses;
- an immersive configuration may require a coherent group of ports.

The active processor receives an immutable/activation-time view of the accepted configuration. Layout changes require deactivation/reconfiguration rather than racing mutable metadata against the realtime callback.

## Layout families

The intended semantic families are:

- mono;
- stereo;
- discrete N-channel audio when speaker meaning is unspecified;
- speaker/channel-based layouts with ordered semantic speaker positions;
- ambisonic layouts with explicit ordering and normalization;
- future extension points where a genuinely different semantic family is required.

Do not model every immersive configuration as an ever-growing top-level enum. Named conveniences such as 5.1, 7.1, 7.1.4, or 9.1.6 can construct a speaker layout without becoming the underlying representation.

Speaker positions also need an extension strategy. The common CLAP surround positions are a useful baseline, but VST3 and Core Audio expose positions not present in that exact set. The public representation must tolerate adding standardized positions without breaking downstream code.

## Atmos terminology

Chassis should distinguish channel-based immersive layouts from Dolby Atmos-specific object/metadata workflows.

A 7.1.4 or 9.1.6 speaker bed can be represented as a semantic speaker layout and mapped to a host format that supports it. That is not equivalent to implementing Dolby Atmos object metadata, renderer integration, ADM/BWF workflows, or proprietary host extensions.

The roadmap may add those capabilities if real products require them; they should not be implied by a `ChannelLayout::Atmos` enum variant.

## Ambisonics

Ambisonics is not a speaker layout. Its metadata needs to preserve at least:

- channel count/order relationship;
- channel ordering, such as ACN or FuMa where applicable;
- normalization, such as SN3D, N3D, MaxN, SN2D, or N2D where applicable.

Adapters may support only a subset. For example, a backend that only exposes ACN/SN3D must reject or explicitly convert unsupported configurations rather than silently relabeling them.

## Effect convention

The conventional effect path remains:

```text
main input:   stereo, required
main output:  stereo, required
sidechain:    stereo, optional
```

This should be expressible with one convenience constructor or derive/macro-level convention once the underlying API is settled.

Products can override the convention declaratively. The common path should not require manually constructing port arrays.

## Adapter rule

Format adapters must either:

1. map a Chassis layout/configuration without changing its semantics; or
2. report that the configuration is unsupported.

Do not silently collapse labeled surround to discrete channels, reinterpret ambisonics as speaker audio, or remap speaker identities merely to satisfy a host request.

## Initial implementation boundary

The first production proof only needs mono/stereo/discrete layout mechanics plus the stereo-effect convention. Surround and ambisonic data structures should be designed before the public API freezes, but full host validation can remain on the roadmap.

The current `chassis-core` channel enum is a spike, not the final layout API. It should be replaced before publishing the crate.
