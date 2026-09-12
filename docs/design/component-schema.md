# Component Schema and Runtime Identity

Status: active pre-alpha architecture. Direct component-schema authority, runtime ownership, frozen activation schema, dense audio endpoints, and schema-owned CLAP state identity are implemented. Remaining work centers on owned dynamic metadata and explicit whole-I/O policy.

## Goal

One Chassis component has one coherent immutable semantic schema that can describe an effect, instrument, event processor, graph node, embedded processor, offline processor, or deployment target without assuming plugin or stereo-effect semantics.

The schema is control/setup-domain data. Realtime paths use validated dense indices derived from it.

## Implemented model

```text
Component
  `-> schema() -> ComponentSchema
        |- semantic component/state identity + state-schema version when present
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

CLAP deployment
  |- one shared coherent ComponentSchema snapshot
  |- CLAP_ID remains deployment/export identity
  `- state save/load uses ComponentSchema::state_identity()
```

Implemented details include:

- `Component::schema()` as the sole component metadata authority;
- no separate `Component::audio_ports`, `event_ports`, or `parameter_descriptors` authorities;
- neutral core semantics with no implicit stereo-effect `Component` default;
- explicit `ComponentSchema::stereo_effect(...)` authoring convenience for identity-less/transient common effects;
- explicit `ComponentSchema::stereo_effect_with_state(...)` for conventional effects with semantic persistent identity;
- `ComponentSchema`, `ComponentId`, `StateIdentity`, and `StateSchemaVersion`;
- one immutable schema snapshot owned by `InstanceRuntime`;
- `InstanceRuntime::from_schema` / `for_component` as the coherent construction paths; proof-order `new` / `new_with_event_ports` constructors are removed;
- activation rejection for audio/event/parameter/state-identity drift;
- runtime-owned semantic state identity/version for complete save/load/migration APIs;
- the exact runtime-frozen schema exposed through `ActivationConfig::schema()` so processor setup never regenerates component metadata;
- stable audio keys resolved to `AudioPortIndex` before processing;
- `ResolvedAudioIoConfiguration` for activation-local port/layout/direction legality;
- `InputEndpoint` / `OutputEndpoint` containing only dense `AudioPortIndex` plus channel;
- process-boundary rejection of inactive, unknown, wrong-direction, and out-of-range endpoints before DSP;
- CLAP setup projecting audio and parameters from the same coherent `ComponentSchema`;
- CLAP shared/main-thread state retaining and comparing the coherent schema snapshot;
- CLAP state save/load consuming semantic `StateIdentity` from the component schema instead of `CLAP_ID` / a duplicate deployment schema version;
- CLAP exports requiring semantic state identity while `CLAP_ID` remains descriptor/export identity only;
- processor/conformance fixtures using dense process identity rather than stable-key comparisons.

## Stable identities versus dense indices

Stable identities exist for persistence, authoring, inspection, and setup. Dense indices exist for active processing.

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

Audio endpoints are dense-only process values:

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

`InstanceRuntime` validates each process endpoint against the activation-owned resolved configuration before product DSP runs. This gives plugin adapters, devices, graphs, embedded callers, and offline runtimes the same core legality contract rather than relying on one deployment adapter to be trusted implicitly.

The current runtime may traverse a buffer source once for endpoint legality and again for block dimensions/processing. Keep that simple bounded behavior until representative measurement justifies a combined process plan or stronger source-qualification API.

## `ComponentSchema`

The authoring contract is one coherent schema factory:

```rust,ignore
pub trait Component {
    type Processor: Processor;
    type ActivationError;

    fn schema(&self) -> Result<ComponentSchema, ComponentSchemaError>;
    fn activate_with_state(...) -> Result<Self::Processor, Self::ActivationError>;
}
```

Returning a validated schema by value is acceptable because schema construction is a setup/control-domain operation. `InstanceRuntime` snapshots that result immutably, and `ActivationConfig::schema()` gives activation code the exact frozen generation rather than asking product code to reconstruct it again.

Common effect authoring remains concise through `ComponentSchema::stereo_effect(...)` or `stereo_effect_with_state(...)`. Zero-audio/event processors and custom I/O components construct their actual schema explicitly rather than inheriting a hidden effect default.

## Owned schema metadata

The remaining metadata limitation is that several stable audio/event identities and display fields are still source-static. That is acceptable for current plugin fixtures but not sufficient for dynamically discovered/hosted components.

Schema construction is non-realtime. Prefer owned validated identifiers/metadata where dynamic construction is useful. `Arc<str>` or another validated owned string representation is preferable to a hidden global interner merely to avoid small setup-domain allocations; dense indices already remove strings from processing.

Do not change every metadata type merely for aesthetic consistency. Migrate the fields that materially block dynamic/hosted components, then let real host/graph/device clients determine whether further ownership abstraction is warranted.

## State identity

Persistent semantic state has one component-owned identity/version authority in `ComponentSchema` and `InstanceRuntime`.

Distinguish:

- **semantic state identity/version**, owned by the component/schema/runtime contract;
- **deployment/export identity**, such as CLAP IDs, VST3 class IDs, AU subtype/manufacturer, and bundle IDs, owned by product/export metadata/tooling and deliberately mapped to the component.

A product manifest may connect the two, but a plugin ABI identifier must not become core state identity accidentally.

Core complete-state APIs already use schema-owned identity. CLAP now does the same: shared state retains the validated schema snapshot, state save/load uses its `StateIdentity`, and `CLAP_ID` is used only for CLAP deployment identity. The duplicated `CLAP_STATE_SCHEMA` authority is removed.

Do not make state identity mandatory for every core component merely because plugin deployment needs it. Identity-less schemas remain useful for transient or embedded components until a real persistence requirement proves otherwise; deployments that persist state may require identity at their own boundary.

## Conventional helpers

Neutral core semantics do not imply verbose common cases.

`ComponentSchema::stereo_effect(...)` and `stereo_effect_with_state(...)` are explicit convention helpers. Additional helpers should only be added for repeated real patterns such as:

```text
mono effect
stereo effect + optional sidechain variants
stereo instrument output
zero-audio event processor
```

A helper creates an ordinary schema/configuration. It does not create a separate lifecycle model or processor trait.

## Runtime construction

`InstanceRuntime::from_schema` and `InstanceRuntime::for_component` consume one complete validated schema snapshot and derive mutable base state from it.

Do not reintroduce constructor families whose differences only reflect the historical order features were implemented.

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

1. make stable audio/event metadata dynamically ownable where real hosted/dynamic clients require it;
2. implement explicit whole-I/O configuration policy;
3. exercise instruments, multi-output processors, and dynamic/hosted metadata through the same schema/runtime model;
4. only then move outward into broader background lifecycle, UI, device, graph, and hosting layers.

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
