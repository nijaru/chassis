# Audio Ports and Layout Negotiation

Status: executable pre-alpha foundation; public API not frozen. Owned-capable port metadata, structural configuration validation, dense activation-local indexing, and schema-owned whole-component I/O policy are implemented for mono/stereo/discrete layouts.

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
└── optionality
```

`PortKey` and port display names can remain borrowed for static product definitions or own runtime-discovered text for dynamic/hosted components. This ownership lives entirely in setup/control-domain metadata.

Adapters/runtime setup resolve stable keys to compact `AudioPortIndex` values after schema validation. Dense indices are **derived projections**, not author-facing compatibility identity and not serialized product state.

Changing stereo to another allowed layout does not create a different conceptual main output.

## Whole-component configuration is authoritative

Layout negotiation operates on one coherent proposed configuration rather than independently mutating ports:

```text
AudioIoConfiguration
`- active ports[] -> stable port key + layout
```

Optional inactive ports are omitted. The runtime first validates the proposal structurally against the declared ports, then evaluates the component's `AudioIoPolicy`, then resolves accepted stable identities to dense activation-owned metadata.

This supports invariants such as:

- main input/output must match when a policy says so;
- an instrument has no audio input;
- sidechain exists only for selected layouts;
- multiple instrument output buses have different layouts;
- immersive ports must change coherently.

The active processor receives an immutable activation-time configuration. No layout metadata races against the callback.

## Configuration policy

Port descriptors answer **what ports exist**. `AudioIoPolicy` answers **which whole configurations are valid**.

The first executable policy forms are:

```text
AnyStructurallyValid
Enumerated([configuration...])
```

`AnyStructurallyValid` is an explicit choice for dynamic/embedded components whose semantics genuinely impose no stronger relationship than descriptor validity.

`Enumerated` owns a finite set of allowed whole configurations. Schema construction rejects an empty enumerated policy, structurally invalid allowed configurations, and duplicate configurations. Matching is semantic and order-independent rather than depending on declaration order.

Policy evaluation is non-realtime/inactive and cannot depend on mutable `Processor` state. Policy is part of immutable component schema identity for one runtime generation; activation rejects policy drift even if port descriptors are otherwise unchanged.

The conventional stereo-effect helpers use an enumerated policy containing exactly:

```text
stereo main input + stereo main output
or
stereo main input + stereo main output + stereo sidechain
```

They do not automatically accept mono, arbitrary discrete, or future surround layouts merely because the port data model can represent them.

Rule-family policies remain a deliberate future extension. A real component may eventually justify forms such as “main input/output must use the same supported speaker layout” without enumerating every layout, but the policy language should not grow ahead of concrete requirements.

## Activation and adapter rule

For one proposed activation:

```text
stable port schema
       +
AudioIoPolicy
       +
requested AudioIoConfiguration
       |
       v
structural validation
       |
       v
whole-policy acceptance
       |
       v
stable key -> AudioPortIndex resolution
       |
       v
ResolvedAudioIoConfiguration
```

A failed structural or policy check occurs before product activation resources are published.

For each host proposal, an adapter must either:

1. map the semantic Chassis configuration without changing meaning; or
2. report unsupported/invalid configuration through the target format's contract.

If a backend changes one bus at a time, the adapter still evaluates the resulting **whole** Chassis proposal before committing it. Backend API shape does not weaken cross-bus invariants.

The current CLAP adapter derives its setup mapping from the coherent component schema and rejects a mapping outside `ComponentSchema::audio_io_policy()`. The callback process plan carries dense `AudioPortIndex` values and layouts, not stable `PortKey` strings.

## Layout families

The executable core currently provides:

- mono;
- stereo;
- discrete N-channel audio where speaker meaning is unknown.

The semantic model should later support speaker/channel layouts with ordered semantic positions and ambisonics with explicit ordering/normalization. Add those representations before claiming support for labeled surround/immersive layouts; do not overload `Discrete` with speaker meaning.

Channel counts use a non-zero `u32` domain so core does not impose an arbitrary smaller cap than target APIs. Actual allocations/work remain bounded by the accepted activation configuration and practical host/product limits.

## Speaker positions

The future speaker-position representation must tolerate standardized positions beyond one backend's current list. Do not expose CLAP numeric constants or VST3 speaker masks as core identity.

Adapters map known Chassis positions exactly or reject a configuration they cannot represent. Never silently turn labeled surround into unlabeled discrete audio merely to make a host accept it.

## Ambisonics

Ambisonics is not speaker audio. Preserve ordering and normalization metadata (for example ACN/FuMa and SN3D/N3D where relevant) as semantic configuration.

An adapter supporting only a subset rejects or explicitly converts under a declared policy; it does not relabel unsupported ambisonics silently.

## Immersive / Atmos

A channel-based 7.1.4 or 9.1.6 bed can eventually be represented as a speaker layout when the host format supports it.

That is not equivalent to Dolby Atmos object metadata, renderer integration, ADM/BWF workflows, or proprietary host extensions. Those are separate capabilities to consider only if a real product requires them.

## Default effect convention

```text
main input:   stereo, required
main output:  stereo, required
sidechain:    stereo, optional/inactive
```

`ComponentSchema::stereo_effect(...)` and `stereo_effect_with_state(...)` construct this ordinary schema/policy combination. Authors do not build per-format bus tables.

## Next proof targets

The current audio-port abstraction should now be exercised rather than generalized speculatively:

- instrument: no audio input, stereo output, note input;
- multi-output component with materially different output buses;
- sidechain/multi-layout component using more than the conventional fixed stereo policy;
- dynamic/hosted component whose port keys and names are constructed at runtime.

Those proofs should determine whether Chassis next needs a rule-family policy, speaker layouts, additional convenience constructors, or only documentation/ergonomic improvements.
