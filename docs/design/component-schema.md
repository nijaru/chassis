# Component Schema and Runtime Identity

Status: active pre-alpha architecture. The complete schema/runtime authority and dense audio endpoint model are implemented; the remaining work is removal of proof-era authoring bridges, owned dynamic metadata, and explicit whole-I/O policy.

## Goal

One Chassis component should have one coherent immutable semantic schema that can describe an effect, instrument, event processor, graph node, embedded processor, offline processor, or deployment target without assuming plugin or stereo-effect semantics.

The schema is control/setup-domain data. Realtime paths use validated dense indices derived from it.

## Implemented foundation

The current core now has one runtime-owned component schema model:

```text
Component
  |
  `-> ComponentSchema
        |- semantic component/state identity + state-schema version
        |- audio-port schema
        |- event-port schema
        |- parameter schema
        `- dense key lookup derived at validation/setup

InstanceRuntime
  |- owned validated immutable ComponentSchema
  |- canonical parameter/custom state
  |- active resolved audio configuration
  |- active processor + latency/configuration
  `- schema drift rejection before activation

ActivationConfig
  |- process resource bounds
  |- accepted audio configuration
  `- exact runtime-frozen ComponentSchema generation

Processor / ProcessBlock
  |- AudioPortIndex + channel endpoints
  |- EventPortIndex note/event addressing
  |- ParameterIndex automation addressing
  `- no stable strings or backend IDs in process endpoint values
```

Implemented details include:

- `ComponentSchema`, `ComponentId`, `StateIdentity`, and `StateSchemaVersion`;
- one immutable schema snapshot owned by `InstanceRuntime`;
- activation rejection for audio/event/parameter/state-identity drift;
- runtime-owned semantic state identity/version for complete save/load/migration APIs;
- the exact runtime-frozen schema exposed through `ActivationConfig::schema()` so processor setup never needs to regenerate component metadata;
- stable audio keys resolved to `AudioPortIndex` before processing;
- `ResolvedAudioIoConfiguration` for activation-local port/layout/direction legality;
- `InputEndpoint` / `OutputEndpoint` containing only dense `AudioPortIndex` plus channel;
- process-boundary rejection of inactive, unknown, wrong-direction, and out-of-range endpoints before DSP;
- CLAP setup mapping that resolves Chassis audio indices before the callback;
- processor/conformance fixtures using dense process identity rather than stable-key comparisons.

## Remaining proof-era API

The current `Component` trait still exposes separate incremental methods for audio ports, event ports, and parameters. Its default `schema()` folds those methods into one `ComponentSchema` as a migration bridge.

Likewise, conventional stereo-effect defaults still live on the core trait, and transitional runtime/state helpers remain for older tests/adapters.

These are compatibility bridges inside an unpublished pre-alpha repository, not intended stable API.

Before freezing the authoring surface:

- make `Component::schema()` the direct component authority;
- remove separate schema methods as independent authorities;
- remove the core semantic default that every component is a conventional stereo effect;
- keep conventional effect helpers/facades above neutral core semantics;
- remove `InstanceRuntime::new` / `new_with_event_ports` and other proof-order constructors;
- remove caller-supplied complete-state identity paths after all adapters use schema identity;
- make stable audio/event metadata ownable for dynamically constructed/hosted components;
- add explicit whole-I/O policy for components with multiple legal configurations.

## Stable identities versus dense indices

Stable identities exist for persistence, authoring, inspection, and setup.

Dense indices exist for active processing.

Use the same pattern consistently:

```text
AudioPortKey  -> AudioPortIndex
EventPortKey  -> EventPortIndex
ParameterKey  -> ParameterIndex
```

Rules:

- keys are semantic compatibility identity;
- indices are meaningful only relative to one validated schema generation;
- declaration order alone is not persistent identity;
- backend IDs are adapter projections and never core identity;
- process-time code does not perform string lookup;
- changing a display label does not change identity;
- dynamic/hosted components must eventually be able to own keys instead of requiring source-static literals.

## Audio process endpoints

Audio endpoints are now dense-only process values:

```text
stable audio schema
       +
requested whole-I/O configuration
       |
       v
validate + resolve while inactive
       |
       v
ResolvedAudioIoConfiguration
       |
       v
InputEndpoint / OutputEndpoint
  AudioPortIndex + channel
```

Stable keys remain on schema/configuration/setup APIs. They are not stored in callback endpoint values.

`InstanceRuntime` validates each process endpoint against the activation-owned resolved configuration before product DSP runs. This makes the same core contract usable by plugin adapters, devices, graphs, embedded callers, and offline runtimes rather than relying on any one adapter to be trusted implicitly.

The current runtime may traverse a buffer source once for endpoint legality and again for block dimensions/processing. Keep that simple bounded behavior until representative measurement justifies a combined process plan or stronger source qualification API.

## `ComponentSchema`

A component should expose one coherent schema rather than several independently queried authorities.

Target authoring shape:

```rust,ignore
pub trait Component {
    type Processor: Processor;
    type ActivationError;

    fn schema(&self) -> Result<ComponentSchema, ComponentSchemaError>;
    fn activate_with_state(...) -> Result<Self::Processor, Self::ActivationError>;
}
```

Returning a validated schema by value is acceptable because schema construction is a setup/control-domain operation. `InstanceRuntime` snapshots that result immutably, and `ActivationConfig::schema()` gives activation code the exact frozen generation rather than asking product code to reconstruct it again.

A dynamic component assembled at runtime must be representable. Core schema identity therefore must not require every name/key to be an `&'static str` merely because plugin definitions are commonly static.

## Owned schema metadata

Schema construction is a non-realtime operation. Prefer owned validated identifiers where dynamic construction is useful.

Potential implementation choices include `Arc<str>` or validated owned strings. Do not introduce a global interner merely to avoid small setup-domain allocations; dense indices already remove strings from processing.

Display names and other metadata can likewise be owned non-realtime data when that improves generality.

## State identity

Persistent semantic state has one component-owned identity/version authority through `ComponentSchema` and `InstanceRuntime`.

Distinguish:

- **semantic state identity/version**, owned by the component/schema/runtime contract;
- **deployment/export identity**, such as CLAP IDs, VST3 class IDs, AU subtype/manufacturer, and bundle IDs, owned by product/export metadata/tooling and deliberately mapped to the component.

A product manifest may connect the two, but a plugin ABI identifier must not become core state identity accidentally.

Explicit product-ID runtime helpers currently remain only as migration paths and should disappear after deployment adapters use schema-owned identity directly.

## Conventional helpers

Neutral core semantics do not imply verbose common cases.

Provide helpers/facade constructors for patterns such as:

```text
stereo effect
mono effect
stereo effect + optional sidechain
stereo instrument output
zero-audio event processor
```

A helper creates an ordinary schema/configuration. It does not create a separate lifecycle model or special processor trait.

Plugin-focused documentation can make conventional helpers the easy path while core remains accurate for other audio software.

## Runtime construction

`InstanceRuntime::from_schema` and `InstanceRuntime::for_component` are the intended construction direction: consume/clone/validate one complete schema snapshot, then derive mutable base state from it.

Avoid retaining a constructor family whose differences only reflect the historical order features were implemented.

## Schema drift

Once an instance is created, compatibility-sensitive schema is fixed for that instance generation.

Activation with a component whose schema no longer matches fails before product resources become active.

Explicit dynamic reconfiguration may later exist for capabilities designed to change, such as accepted audio layouts. Immutable identity/schema changes are not silently accepted as ordinary activation changes.

## I/O policy

Structural port validation and dense endpoint legality are implemented, but port existence alone cannot express every legal whole-component configuration.

The next semantic layer should be able to express policies such as:

```text
mono in -> mono out
or
stereo in -> stereo out
with optional stereo sidechain
```

Do not ask CLAP/VST3/AU/device/graph adapters to infer these combinations independently.

The policy remains setup/control-domain data and produces one accepted activation configuration/process plan.

## Realtime implications

The architecture should continue to preserve:

- all stable-key validation and key -> index resolution outside processing;
- no callback string allocation or lookup;
- no hidden global interner;
- small value-type process endpoints;
- no schema/configuration mutation racing an active processor;
- bounded validation/work before product DSP;
- format-specific setup mappings derived from the same semantic schema.

## Next migration order

1. finish direct `Component::schema()` authority;
2. delete proof-era constructors and default-effect assumptions from neutral core;
3. migrate remaining deployment state identity to the schema-owned path;
4. make stable port/event metadata dynamically ownable where required;
5. implement explicit whole-I/O configuration policy;
6. exercise the resulting API with materially different component classes before freezing it.

Do not preserve source compatibility with unpublished proof APIs at the expense of the target model.

## Freeze criteria

Do not freeze this surface until it has represented and been exercised by materially different component classes:

- conventional effect;
- sidechain/multi-layout effect;
- zero-audio event processor;
- instrument with event input/audio output;
- multi-output component;
- embedded/graph-node use without plugin deployment;
- plugin adapter projection;
- dynamic/hosted component metadata where applicable.
