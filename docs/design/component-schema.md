# Component Schema and Runtime Identity

Status: target design for the next pre-alpha core refactor; implementation may replace current public APIs freely while unpublished.

## Goal

One Chassis component should have one coherent immutable semantic schema that can describe an effect, instrument, event processor, graph node, embedded processor, offline processor, or deployment target without assuming plugin or stereo-effect semantics.

The schema is control/setup-domain data. Realtime paths use validated dense indices derived from it.

## Problems in the current incremental API

The current core grew capability-by-capability:

- `Component` exposes audio ports, event ports, and parameters through separate methods;
- parameters use an owned validated schema/store, while audio/event port metadata still depends heavily on static strings;
- event ports now have dense runtime indices, while audio process endpoints still carry stable `PortKey` identity directly;
- `InstanceRuntime::new`, `new_with_event_ports`, and `for_component` reflect the order features were added rather than one complete component schema;
- the runtime snapshots parameter/event schemas but does not yet own the complete immutable audio schema as one authority;
- state product identity/schema version are repeatedly supplied by callers/adapters instead of being part of one durable component/state definition;
- conventional stereo-effect defaults live in the core `Component` trait even though zero-audio/event-only/instrument/application components are equally valid.

These are reasonable proof-era choices but should not become the stable framework API.

## Target model

Conceptually:

```text
Component
  |
  `-> ComponentSchema
        |- stable component/state identity
        |- audio-port schema
        |- event-port schema
        |- parameter schema
        |- capabilities / I/O policy as they become specified
        `- immutable semantic metadata

InstanceRuntime
  |- owned/validated immutable schema snapshot
  |- canonical parameter/custom state
  |- stable-key -> dense-index setup maps
  `- active processor/configuration

Processor / ProcessBlock
  |- dense audio-port indices
  |- dense event-port indices
  |- dense parameter indices
  `- no string lookup or backend IDs on the realtime path
```

Exact Rust type names are not frozen by this document.

## `ComponentSchema`

A component should expose one coherent schema rather than several independently queried authorities.

Illustrative shape:

```rust,ignore
pub struct ComponentSchema {
    identity: ComponentIdentity,
    audio_ports: Vec<AudioPortDescriptor>,
    event_ports: Vec<EventPortDescriptor>,
    parameters: Vec<ParameterDescriptor>,
    // later: supported whole-I/O configurations/capabilities
}

pub trait Component {
    type Processor: Processor;
    type ActivationError;

    fn schema(&self) -> &ComponentSchema;
    fn activate_with_state(...) -> Result<Self::Processor, Self::ActivationError>;
}
```

The real API may borrow or own differently, but it should have one semantic authority.

A dynamic component assembled at runtime must be representable. Core schema identity therefore must not require every name/key to be an `&'static str` merely because plugin definitions are commonly static.

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
- dynamic/hosted components can own keys instead of requiring static source literals.

## Audio process endpoints

Audio buffer endpoints should eventually carry dense activation-local audio-port indices plus channel indices, not stable string keys.

Current stable-key endpoints were useful while proving buffer legality, but dense indices better match the established parameter/event model and allow dynamically discovered components without leaking owned strings onto the callback.

Setup performs:

```text
stable audio schema
       +
accepted whole-I/O configuration
       |
       v
validated dense mapping / process plan
       |
       v
ProcessBlock endpoints: AudioPortIndex + channel
```

Stable keys remain available on non-realtime schema/configuration APIs.

## Owned schema metadata

Schema construction is a non-realtime operation. Prefer owned validated identifiers where dynamic construction is useful.

Potential implementation choices include `Arc<str>` or validated owned strings. Do not choose an interning/global-registry design merely to avoid tiny setup-domain allocations; dense indices remove strings from the callback.

Display names and other metadata can likewise be owned non-realtime data when that improves generality.

## State identity

Persistent semantic state should have one component-owned identity/version authority.

Do not require every adapter/application caller to repeatedly pass matching `product_id` and `product_schema` values to runtime save/load APIs. That creates duplicated authority and makes cross-deployment compatibility easier to break.

Distinguish:

- **semantic state identity/version**, which belongs to the component/schema/runtime contract;
- **deployment/export identity**, such as CLAP IDs, VST3 class IDs, AU subtype/manufacturer, bundle IDs, which belongs to product/export metadata/tooling and maps to the component deliberately.

A product manifest may connect the two, but a plugin ABI identifier should not become the core state identifier by accident.

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

The helper creates an ordinary schema. It does not create a separate lifecycle model or special processor trait.

Plugin-focused documentation can make these helpers the default path while core remains accurate for other audio software.

## Runtime construction

The final runtime constructor should consume/clone/validate one complete schema snapshot.

Avoid retaining a family of constructors whose only distinction reflects historical feature additions (`new(parameters)`, `new_with_event_ports(...)`, etc.). If lower-level constructors remain useful for tests, make the schema object itself the unit of construction.

`InstanceRuntime::for_component` may remain a convenience over the same schema constructor.

## Schema drift

Once an instance is created, its compatibility-sensitive schema is fixed for that instance generation.

Activation with a component whose schema no longer matches must fail before product resources become active.

Eventually, explicit dynamic reconfiguration may exist for capabilities designed to change (for example accepted audio layouts), but immutable identity/schema changes are not silently accepted as ordinary activation changes.

## I/O policy

Audio-port existence is not enough to express all legal configurations.

The schema/refactor should leave room for an explicit whole-I/O policy such as:

```text
mono in -> mono out
or
stereo in -> stereo out
with optional stereo sidechain
```

Do not encode this by asking adapters to infer legal combinations independently.

The policy remains setup/control-domain data and produces one accepted activation configuration/process plan.

## Realtime implications

The refactor should improve, not weaken, realtime properties:

- all key validation and key -> index resolution before activation/process;
- no callback string allocation/lookup;
- no hidden global interner required;
- process endpoints remain small value types;
- schema/configuration mutation cannot race the active processor;
- adapters can precompute format-specific mappings from the same schema.

## Migration strategy

Because Chassis is unpublished pre-alpha:

1. introduce the coherent schema/identity types;
2. migrate runtime ownership and tests;
3. move audio process endpoints to dense audio-port indices;
4. migrate CLAP mapping to derive from the schema;
5. migrate examples/conformance fixtures;
6. delete obsolete compatibility constructors/default-effect assumptions rather than deprecating them indefinitely;
7. re-run allocation, state, adapter, conformance, and real-host regressions.

Do not preserve source compatibility with the current proof API at the expense of the target model.

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
