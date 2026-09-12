# Component Schema and Runtime Identity

Status: active pre-alpha architecture. Direct component-schema authority, runtime ownership, frozen activation schema, dense process identities, owned audio/event metadata, schema-owned whole-audio-I/O policy, and schema-owned runtime state identity are implemented and exercised across materially different component classes. The ownership shape is now a candidate-stable architecture; broader Chassis APIs remain pre-alpha.

## Goal

One Chassis component has one coherent immutable semantic schema that can describe an effect, instrument, event processor, graph node, embedded processor, offline processor, or deployment target without assuming plugin or stereo-effect semantics.

The schema is control/setup-domain data. Realtime paths use validated dense indices derived from it.

## Implemented model

```text
Component
  `-> schema() -> ComponentSchema
        |- semantic component/state identity + state-schema version when present
        |- owned-capable audio-port schema
        |- whole-component AudioIoPolicy
        |- owned-capable event-port schema
        |- parameter schema
        `- stable-key -> dense-index lookup derived at validation/setup

InstanceRuntime
  |- owned validated immutable ComponentSchema
  |- canonical parameter/custom state
  |- active resolved audio configuration
  |- active processor + latency/configuration
  `- schema + audio-policy drift rejection before activation

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
  |- setup audio mapping must satisfy ComponentSchema::audio_io_policy()
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
- activation rejection for audio/event/parameter/state-identity and audio-I/O-policy drift;
- runtime-owned semantic state identity/version for complete save/load/migration APIs, with runtime explicit-ID compatibility shims removed;
- the exact runtime-frozen schema exposed through `ActivationConfig::schema()` so processor setup never regenerates component metadata;
- audio and event stable keys/display metadata that may remain borrowed for static product declarations or own runtime-discovered metadata;
- stable audio keys resolved to `AudioPortIndex` before processing;
- stable event keys resolved to `EventPortIndex` before processing;
- `ResolvedAudioIoConfiguration` for activation-local port/layout/direction legality;
- `InputEndpoint` / `OutputEndpoint` containing only dense `AudioPortIndex` plus channel;
- note/event process addressing containing dense `EventPortIndex`, not stable event strings;
- process-boundary rejection of inactive, unknown, wrong-direction, and out-of-range audio endpoints before DSP;
- `AudioIoPolicy` as immutable schema authority for valid whole-component configurations;
- explicit `AnyStructurallyValid` policy for components that genuinely impose no stronger cross-port rule;
- explicit enumerated configuration policy, including duplicate/invalid-policy rejection at schema construction;
- the conventional effect policy accepting stereo main input/output with the stereo sidechain either inactive or active, rather than accepting arbitrary layouts merely because descriptors exist;
- runtime policy enforcement before product activation and failure-atomic rejection of structurally valid but unsupported layouts;
- CLAP setup projecting audio and parameters from the same coherent `ComponentSchema` and rejecting mappings outside its audio-I/O policy;
- CLAP shared/main-thread state retaining and comparing the coherent schema snapshot;
- CLAP state save/load consuming semantic `StateIdentity` from the component schema instead of `CLAP_ID` / a duplicate deployment schema version;
- CLAP exports requiring semantic state identity while `CLAP_ID` remains descriptor/export identity only;
- processor/conformance fixtures using dense process identity rather than stable-key comparisons.

## Stable identities versus dense indices

Stable identities exist for persistence, authoring, inspection, and setup. Dense indices exist for active processing.

```text
PortKey       -> AudioPortIndex
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
- static product declarations remain allocation-free at authoring time;
- dynamically discovered/hosted audio and event metadata can own its strings/capability lists without introducing callback string work.

## Audio process endpoints

Audio endpoints are dense-only process values:

```text
stable audio schema
       +
AudioIoPolicy
       +
requested whole-I/O configuration
       |
       v
structural validation + policy acceptance while inactive
       |
       v
resolve stable keys to dense indices
       |
       v
ResolvedAudioIoConfiguration
       |
       v
InputEndpoint / OutputEndpoint
  AudioPortIndex + channel
```

Stable keys remain on schema/configuration/setup APIs. They are not stored in callback endpoint values.

`InstanceRuntime` first validates structural configuration, then enforces the schema-owned whole-I/O policy, then resolves accepted ports to activation-owned dense metadata. It validates each process endpoint against that resolved configuration before product DSP runs. This gives plugin adapters, devices, graphs, embedded callers, and offline runtimes the same core legality contract rather than relying on one deployment adapter to be trusted implicitly.

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

Common effect authoring remains concise through `ComponentSchema::stereo_effect(...)` or `stereo_effect_with_state(...)`. Zero-audio/event processors and custom I/O components construct their actual schema and choose their I/O policy explicitly rather than inheriting a hidden effect default.

## Owned schema metadata

Audio/event schema metadata supports both static and runtime-owned forms. `PortKey` / `EventPortKey` and their display metadata use borrowed-or-owned setup-domain storage; event dialect capability lists likewise may be borrowed static slices or owned vectors.

This preserves zero-allocation static product declarations while allowing dynamically discovered or hosted components to build ordinary `ComponentSchema` values from runtime metadata. Dense indices still remove stable strings from processing, so ownership generality does not change the callback contract.

A hosted-style runtime fixture now constructs audio/event descriptors from runtime `String`s, activates them through the ordinary runtime, resolves them to dense indices, and processes without strings in callback identities.

Do not add a global string interner merely for metadata identity. If future graph/device/hosting clients expose another concrete ownership requirement, solve that requirement explicitly rather than abstracting all metadata preemptively.

## State identity

Persistent semantic state has one component-owned identity/version authority in `ComponentSchema` and `InstanceRuntime`.

Distinguish:

- **semantic state identity/version**, owned by the component/schema/runtime contract;
- **deployment/export identity**, such as CLAP IDs, VST3 class IDs, AU subtype/manufacturer, and bundle IDs, owned by product/export metadata/tooling and deliberately mapped to the component.

A product manifest may connect the two, but a plugin ABI identifier must not become core state identity accidentally.

Runtime complete-state APIs and CLAP save/load use schema-owned identity. The duplicated `CLAP_STATE_SCHEMA`, runtime explicit-product-ID state APIs, and parameter-only runtime state compatibility paths are removed. `CLAP_ID` is deployment identity only.

Do not make state identity mandatory for every core component merely because plugin deployment needs it. Identity-less schemas remain useful for transient or embedded components until a real persistence requirement proves otherwise; deployments that persist state may require identity at their own boundary.

## Conventional helpers

Neutral core semantics do not imply verbose common cases.

`ComponentSchema::stereo_effect(...)` and `stereo_effect_with_state(...)` are explicit convention helpers. They create an ordinary schema whose audio policy permits exactly stereo main input/output with an optional stereo sidechain.

Additional helpers should only be added after repeated real patterns prove them useful, for example mono effects, stereo instruments, or common multi-output instruments. A helper must not introduce a separate lifecycle or processor trait.

The current instrument, multi-output, layout-switching, and dynamic-metadata proofs did not require new helper families. Keep the explicit lower-level schema as the default for uncommon shapes until repetition justifies another convenience.

## Runtime construction

`InstanceRuntime::from_schema` and `InstanceRuntime::for_component` consume one complete validated schema snapshot and derive mutable base state from it.

Do not reintroduce constructor families whose differences only reflect the historical order features were implemented.

## Schema drift

Once an instance is created, compatibility-sensitive schema is fixed for that instance generation.

Activation with a component whose state identity, audio ports, audio-I/O policy, event ports, or parameters no longer match fails before product resources become active. Changing policy is a schema-generation change even if the port descriptors themselves remain identical.

Explicit dynamic reconfiguration exists within one immutable policy by deactivating and selecting another accepted audio configuration on reactivation. Immutable schema/policy changes are not silently accepted as ordinary activation changes.

## I/O policy

Port descriptors answer **what ports exist**. `AudioIoPolicy` answers **which complete active-port/layout combinations are semantically supported**.

The executable policy forms are deliberately small:

```text
AnyStructurallyValid
Enumerated([configuration...])
```

Enumerated policies are validated when the schema is built. Runtime activation rejects a structurally valid proposal that is outside policy before product DSP construction, and adapters use the same policy rather than infer their own cross-port rules.

The default effect helper enumerates:

```text
stereo main in + stereo main out
or
stereo main in + stereo main out + stereo sidechain
```

A layout-switching/sidechain fixture proves one schema can reactivate across mono and stereo configurations, derive activation-specific resources from the accepted layout, and reject a structurally valid unsupported combination. A separate multi-output fixture proves routing follows dense semantic port identity even when callback buffers arrive in shuffled order.

Rule-family policies such as “main input/output must have the same supported layout” should be added only when a real component needs a non-finite or impractically large configuration family. Current evidence does not justify that complexity.

## Realtime implications

The architecture preserves:

- all stable-key validation and key -> index resolution outside processing;
- whole-I/O policy evaluation outside processing;
- no callback string allocation or lookup;
- no hidden global interner;
- small value-type process endpoints;
- no schema/configuration mutation racing an active processor;
- bounded validation/work before product DSP;
- format-specific setup mappings derived from the same semantic schema and policy.

## Proof results and current conclusion

The original schema freeze criteria have now been exercised by:

- conventional effect and CLAP adapter projection;
- sidechain/multi-layout effect with reactivation and policy rejection;
- zero-audio event processor;
- event-driven instrument with no audio input and stereo output-only buffers;
- multiple semantically distinct output buses with shuffled callback order;
- direct non-plugin/embedded runtime use;
- runtime-owned dynamic/hosted audio and event metadata.

Those cases fit the same schema/runtime model without a new lifecycle, richer policy language, or mandatory new convenience constructors. The remaining broad Chassis work—modulation, event output/MIDI/expression, tail/bypass, background lifecycle, editor/device/graph/hosting layers, and additional plugin formats—may still expose changes, but there is no current evidence for reopening the ownership model speculatively.

Treat `ComponentSchema` + `InstanceRuntime` as a candidate-stable architecture and change it only when a concrete later layer exposes a semantic mismatch.
