# Parameters, Automation, and State

Status: design direction; public API and serialization format not frozen.

## Goal

Make the parameter/state path conventional enough that most plugins declare their controls and persistent state rather than implementing host plumbing, while preserving sample-accurate automation and allowing unusual products to replace smoothing or interpretation semantics.

## Cross-format observations

The target APIs share several durable concepts:

- parameters have stable identities;
- hosts need metadata, ranges/defaults, display formatting, and text parsing;
- VST3 and CLAP expose normalized/numeric automation points with sample offsets during processing;
- CLAP distinguishes base parameter value changes from modulation and supports per-note/key/channel/port modulation capabilities;
- plugin-originated edits need host notification and gesture boundaries;
- Audio Unit parameters have stable identifiers/addresses, ranges, units, value strings, formatting/parsing, and automation APIs.

Chassis should model these semantics directly and translate them in adapters.

## Canonical parameter identity

The preferred Chassis identity is a stable human-readable string key, for example:

```text
input.gain
compressor.threshold
band.1.frequency
output.ceiling
```

Reasons:

- readable in state, tests, diagnostics, and migration code;
- naturally namespaced for nested/repeated parameter groups;
- independent of one plugin format's integer width;
- renaming display labels does not affect compatibility.

Once released, the canonical key is a compatibility contract and must not change merely because a field or UI label is renamed.

Format adapters that require numeric IDs should derive a deterministic numeric representation from the canonical key and detect collisions when the parameter schema is built. A collision must fail loudly rather than silently remapping an existing parameter. The API should provide an explicit numeric-ID override as an escape hatch if a real collision or legacy compatibility requirement occurs.

The exact hash/mapping algorithm must be specified and frozen before publishing a stable adapter.

## Parameter types

The conventional set should include:

- float;
- integer;
- boolean;
- enum/choice with stable variant identities.

Enum state must not depend only on declaration order. Reordering choices should not silently reinterpret existing presets.

Likely later/common extensions include read-only/output parameters, bypass, trigger/momentary semantics, and host-visible grouping. These should be added from real requirements rather than encoded as product-specific hacks.

## Plain values versus normalized host values

Product code should primarily work in meaningful plain units. Normalized 0..1 representations are adapter/host concerns.

A parameter definition therefore needs a mapping between plain and normalized domains plus formatting/parsing behavior.

Common mappings should be built in:

- linear;
- logarithmic/exponential frequency-style ranges;
- skew/power curves;
- stepped/discrete mappings;
- custom monotonic mapping as an escape hatch.

Round-trip and monotonicity properties should be testable by `chassis-test`.

## Formatting and parsing

A parameter can provide conventional unit metadata plus value formatting/parsing hooks.

Examples:

```text
0.707 -> "-3.01 dB"
1000.0 -> "1.00 kHz"
0 -> "Off"
1 -> "On"
```

Adapters should use product formatting where the target API supports it rather than independently inventing display strings.

## Automation and modulation

Base automation and modulation are distinct semantic streams.

Base automation changes the parameter's underlying automated value. Modulation temporarily offsets/transforms that value according to host capabilities and must not overwrite the canonical base value.

Processing should receive timestamped changes at sample offsets. Adapters must preserve those offsets when the source format provides them.

The initial event model should remain monophonic/global while leaving room for CLAP-style per-note/key/channel/port modulation without redesigning parameter identity.

## Parameter state views

Chassis should distinguish at least three notions that frameworks often accidentally collapse:

1. **canonical/base state** — the persistent/control-thread value;
2. **process-time automated value** — value after timestamped host automation at a particular sample;
3. **effective/modulated value** — process-time value after applicable modulation.

A GUI reading the current base value is not the same operation as DSP consuming a sample-accurate effective value.

The ergonomic API may hide boilerplate, but it must not erase these semantics.

## Smoothing

Smoothing is common enough to provide built-in helpers but not universal enough to mandate one behavior.

Parameter definitions may eventually specify conventional smoothing such as none, linear time, or exponential time. Products can opt out and consume raw timestamped values/events.

Before freezing smoothing semantics, test how smoothing should interact with already-sample-accurate automation ramps. Chassis must avoid unintentionally double-smoothing a trajectory the host has already specified precisely.

## Plugin-originated edits

UI or product-originated parameter edits should use a standard handle with gesture semantics conceptually equivalent to:

```text
begin_edit(param)
set_value(param, value)
end_edit(param)
```

The framework is responsible for updating canonical state, notifying observers/UI, and informing the host without feedback loops.

Realtime-originated changes require a bounded event path and format capability checks. A product must not call arbitrary main-thread host APIs from `process`.

## Parameter schema lifetime

The parameter schema is fixed for an instantiated component unless a future capability explicitly models dynamic parameters. Most plugin hosts assume stable parameter identity/count strongly enough that dynamic mutation should not be the default abstraction.

Nested/repeated groups should be expressible declaratively while still generating stable canonical keys.

## Persistent state

Host state should be a Chassis/product semantic format shared across CLAP, VST3, AU, standalone, and embedded deployment rather than one format per adapter.

The logical state contains:

```text
State
├── Chassis state-envelope version
├── product schema version
├── typed parameter base values keyed by canonical parameter ID
└── typed/custom product persistent fields
```

DSP runtime history is not persistent by convention. Filter delay lines, compressor envelopes, oscillator phases, lookahead buffers, and other transient processor state reset/reconstruct rather than appearing in presets/state blobs.

## Serialization properties

The eventual encoding must be:

- deterministic for identical semantic state;
- bounded and defensive when decoding untrusted/corrupt host blobs;
- portable across supported operating systems and plugin formats;
- explicitly versioned;
- capable of skipping/handling unknown fields where useful;
- independent of Rust memory layout and compiler version.

Do not use an encoding merely because `serde` can serialize it. The on-disk/host blob becomes a long-term compatibility contract.

A human-readable representation may be useful for debugging, but compact binary state is acceptable if the schema and deterministic encoding are well specified.

The actual serialization dependency/format remains open until evaluated for stability, security, and license compatibility.

## Typed state representation

Parameter values should serialize in their meaningful typed/plain form rather than as opaque host-normalized floats where possible:

- float -> finite numeric plain value;
- integer -> integer;
- boolean -> boolean;
- enum -> stable variant identity.

This makes state less sensitive to later changes in normalization curves. Intentional semantic range changes still require a product migration.

## Migrations

Loading proceeds conceptually as:

```text
bytes
  -> validate/decode envelope
  -> identify product schema version
  -> migrate semantic state forward
  -> validate current parameter/custom state
  -> publish canonical non-RT state
  -> transfer required DSP changes through a safe activation/block boundary
```

Migration code never runs on the audio thread.

The framework should make sequential migrations conventional and testable rather than encouraging one giant version switch in product code.

## Presets

A preset is fundamentally named product state plus metadata. Chassis should eventually provide common serialization/storage primitives, but preset browsing, tags, search UX, cloud synchronization, and product-specific content remain outside core.

Factory presets can initially be embedded state blobs generated/tested by the same state codec.

## Testing requirements

`chassis-test` should eventually cover:

- unique canonical IDs and unique derived host IDs;
- stable host-ID mapping fixtures;
- plain <-> normalized round trips;
- formatting/parsing round trips where applicable;
- automation at block boundaries and multiple points per block;
- modulation not mutating base state;
- gesture ordering and host echo suppression;
- deterministic state bytes;
- corrupt/truncated/oversized state rejection;
- migration fixtures from every released schema version;
- state round-trip across all format adapters.

## Ergonomics direction

The likely end-user API is declarative, potentially derive/macro-based, but procedural macros should be added only after the semantic model is proven.

The desired experience is approximately:

```text
define parameter fields + stable IDs + ranges/defaults
        ↓
Chassis provides host metadata, state, automation, GUI handles, and tests
        ↓
processor consumes explicit realtime parameter views/events
```

Convention should remove plumbing, not hide timing or compatibility semantics.
