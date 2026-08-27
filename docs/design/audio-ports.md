# Audio Ports and Layout Negotiation

Status: design direction; public API not frozen.

## Goal

Make stereo effects nearly configuration-free while preserving a coherent path to instruments, auxiliary buses, surround, ambisonics, immersive channel layouts, and hosts that reconfigure layouts while inactive.

The core models Chassis semantics. CLAP/VST3/AU identifiers and call shapes belong in adapters.

## Stable identity versus runtime indexing

A port has a stable human-readable product key independent of its active layout and backend representation, for example:

```text
audio.main.in
audio.main.out
audio.sidechain
```

Conceptually:

```text
AudioPortDescriptor
├── stable key
├── display name
├── direction
├── role
└── configuration capabilities
```

Adapters/runtime setup may derive compact numeric IDs or dense indices after schema validation. Those are **derived projections**, not author-facing compatibility identity and not serialized product state.

Changing stereo to 5.1 does not create a different conceptual main output.

## Whole-component configuration is authoritative

Layout negotiation operates on one coherent proposed configuration rather than independently mutating ports:

```text
AudioIoConfiguration
├── active inputs[]  -> stable port key + layout
└── active outputs[] -> stable port key + layout
```

The product policy validates the proposal as a whole while processing is inactive, then Chassis commits it atomically for the next activation.

This supports invariants such as:

- main input/output must match;
- an instrument has no audio input;
- sidechain exists only for selected layouts;
- multiple instrument output buses have different layouts;
- immersive ports must change coherently.

The active processor receives an immutable activation-time configuration. No layout metadata races against the callback.

## Configuration policy

Port descriptors answer **what ports exist**. A separate policy answers **which whole configurations are valid**.

Useful policy forms:

- fixed configuration;
- one of a small enumerated set;
- matching main input/output layout;
- matching channel count subject to layout-family constraints;
- generator/instrument outputs with no audio input;
- explicit product validation for a supported family.

A rule-based policy may accept a family such as “any supported speaker layout where main input/output match” without enumerating every future surround layout.

Policy evaluation is non-realtime/inactive and cannot depend on mutable Processor state.

The initial default effect policy accepts stereo main input/output, optionally with the conventional stereo sidechain. It does not automatically accept arbitrary multichannel layouts merely because the data model can represent them.

## Layout families

The semantic model should support these families without an ever-growing single enum of named layouts:

- mono;
- stereo;
- discrete N-channel audio where speaker meaning is unknown;
- channel/speaker layouts with ordered semantic speaker positions;
- ambisonics with explicit ordering/normalization;
- future distinct semantic families only when requirements justify them.

Named conveniences such as 5.1, 7.1, 7.1.4, or 9.1.6 construct speaker layouts; they are not the underlying extensibility mechanism.

Channel counts should use a domain type wide enough to map target APIs without an arbitrary small framework cap. Current target APIs commonly use 32-bit counts, so the initial Rust spike should prefer a non-zero `u32`-sized representation rather than `u16`. Actual allocations/work remain bounded by the accepted activation configuration and practical host/product limits.

## Speaker positions

The speaker-position representation must tolerate standardized positions beyond one backend's current list. Do not expose CLAP numeric constants or VST3 speaker masks as the core identity.

Adapters map known Chassis positions exactly or reject a configuration they cannot represent. Never silently turn labeled surround into unlabeled discrete audio merely to make a host accept it.

## Ambisonics

Ambisonics is not speaker audio. Preserve ordering and normalization metadata (for example ACN/FuMa and SN3D/N3D where relevant) as semantic configuration.

An adapter supporting only a subset rejects or explicitly converts under a declared policy; it does not relabel unsupported ambisonics silently.

## Immersive / Atmos

A channel-based 7.1.4 or 9.1.6 bed can be represented as a speaker layout when the host format supports it.

That is not equivalent to Dolby Atmos object metadata, renderer integration, ADM/BWF workflows, or proprietary host extensions. Those are separate capabilities to consider only if a real product requires them.

## Default effect convention

```text
main input:   stereo, required
main output:  stereo, required
sidechain:    stereo, optional/inactive
```

One convenience constructor/derive-level convention should cover this. Authors should not manually build per-format bus tables.

## Adapter rule

For each proposed host layout, an adapter must either:

1. map the semantic Chassis configuration without changing meaning; or
2. report unsupported/invalid configuration through the target format's contract.

If a backend changes one bus at a time, the adapter still evaluates the resulting **whole** Chassis proposal before committing it. Backend API shape does not weaken cross-bus invariants.

## Initial implementation boundary

The first processing proof only needs:

- stable port keys;
- mono/stereo/discrete layouts;
- default stereo effect policy with optional sidechain;
- whole-configuration validation and activation-time dense indexing.

Surround and ambisonic semantic types should be designed before public API stabilization, but full host support does not block the first CLAP processing path.
