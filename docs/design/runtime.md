# Runtime Ownership Model

Status: `InstanceRuntime<P>` is the durable format-independent authority for one component instance. It now owns one validated `ComponentSchema`, canonical mutable semantic state derived from that schema, and the active processor lifecycle. Public API spelling remains pre-alpha while proof-era constructors and endpoint identities are removed.

## Runtime roles

```text
Component
  immutable component definition / processor factory
  current migration hook constructs ComponentSchema

ComponentSchema
  semantic state identity/version when identified
  audio-port schema
  event-port schema
  parameter schema
  stable -> dense setup identity resolution

InstanceRuntime<P>
  owned validated ComponentSchema
  durable ParameterStore derived from that schema
  durable validated custom semantic state
  active/inactive lifecycle
  accepted active I/O configuration
  activation-scoped LatencySamples
  optional active Processor

Processor
  exclusive mutable realtime DSP history/resources while active
  caches dense process-time identities/resources established before processing
  reports latency established by this activation

Process<S>
  sample-representation-specific processing over borrowed ProcessBlock<S>

Deployment publication bridge
  adapter-only when host object lifetimes require cross-domain synchronization
```

Each mutable semantic guarantee has one authority. Host/shared/editor representations are synchronized projections, not independently mutable second owners.

## Schema ownership

`ComponentSchema` is the target immutable authority for one component generation. It validates and owns the audio/event/parameter schemas together rather than allowing three independently evolving metadata families.

Persistent/setup identity and process-time identity are separate:

```text
PortKey       -> AudioPortIndex
EventPortKey  -> EventPortIndex
ParameterKey  -> ParameterIndex
```

Stable keys belong to schema, persistence, host mapping, tooling, and author-facing lookup. Dense indices are schema-local process/setup projections and are never persistence identity.

The current audio and event stable key types still use static strings; migrating them to owned validated identities is a remaining pre-v1 step so dynamically loaded/constructed components can be represented naturally. Dense process-time identities prevent that ownership change from putting strings or allocations in the callback.

`ComponentSchema::unidentified()` and the legacy runtime constructors exist only as migration bridges while current proof components/adapters are moved to schema-owned state identity. They are not stable design targets.

## Construction and activation

The preferred construction path is:

```rust,ignore
let mut runtime = InstanceRuntime::for_component(&component)?;
runtime.activate(&component, process_config, requested_audio_io)?;
```

`InstanceRuntime::for_component()` asks the component for one coherent schema, validates it, stores the complete schema, and constructs canonical base parameter state from the schema's parameter descriptors.

Activation then:

1. reconstructs/obtains the component generation's schema;
2. rejects semantic state identity/version drift;
3. rejects audio/event/parameter schema drift;
4. validates the proposed structural audio configuration against the runtime-owned audio schema;
5. owns the accepted configured ports while non-realtime;
6. constructs `ActivationConfig` from runtime-owned data;
7. calls `Component::activate_with_state()` with the current complete semantic state;
8. reads `Processor::latency()` from the successfully constructed processor;
9. publishes the processor, accepted configuration, and latency snapshot together as the active child.

A failed activation never publishes a partial active child. Executable negative-space coverage verifies failure leaves the runtime inactive, preserves durable state, and permits later successful activation.

The component definition is intentionally not stored inside `InstanceRuntime`. A deployment may transfer the active runtime/processor according to its thread contract without forcing `Component: Send` in core.

## Semantic state identity

Complete runtime state now derives semantic identity and schema version from the owned `ComponentSchema`:

```text
ComponentSchema
  StateIdentity
    ComponentId
    StateSchemaVersion
         |
         v
InstanceRuntime state save/load/migration
```

Normal complete-state operations no longer accept duplicate identity/version arguments:

- `state_document()`;
- `encode_state(...)`;
- `apply_state(...)`;
- `apply_state_bytes(...)`.

Explicit `*_for_product` methods remain temporary migration paths for deployment adapters that still own proof-era state identity. They should disappear once adapter state uses the component schema directly.

State identity is semantic persistence identity. CLAP IDs, VST3 class IDs, Audio Unit identifiers, executable/bundle IDs, display names, and vendor strings are deployment metadata and must map explicitly rather than being silently reused as semantic state identity.

## Durable state

Normal project/preset load is complete transactional replacement:

```text
bytes
 -> bounded decode
 -> adjacent state-schema migrations
 -> temporary complete parameter candidate
 -> temporary canonical custom-state candidate
 -> framework validation
 -> product validation over both candidates
 -> publish both together
```

Any failure leaves live parameter and custom state unchanged. Persistent state is semantic versioned data, not Rust object layout or transient DSP history.

## Activation-scoped latency

`LatencySamples` is a sample-count newtype. `Processor::latency()` defaults to zero and is queried once after successful processor construction.

The runtime snapshots latency rather than querying mutable processor state during processing. This gives deployment adapters/graphs a stable value for one active lifetime and prevents externally visible latency from drifting without the lifecycle transition needed to rebuild compensation/execution plans.

A processor whose lookahead, convolution, oversampling path, or other activation resource changes latency establishes the new value during the next activation. `Processor::restart_requested()` signals that the active resources are no longer the desired generation while requiring the current processor to remain valid until replacement.

## Audio configuration and process endpoints

Stable audio configuration is selected while inactive from `PortKey` + layout metadata. Dense `AudioPortIndex` is now available as the process-time identity.

The CLAP setup path resolves stable port keys to dense Chassis indices once. Real CLAP callbacks now carry those resolved indices into Chassis endpoint values. Existing synthetic/core fixtures are being migrated in the same direction.

The temporary endpoint representation still carries both stable key and optional dense index while old fixtures migrate. The target is dense-only callback endpoints:

```text
InputEndpoint
  AudioPortIndex
  channel

OutputEndpoint
  AudioPortIndex
  channel
```

Stable keys should not travel through every realtime sample/block simply because they were convenient in the first proof implementation.

A remaining correctness gap is full endpoint legality validation for generic embedded `ProcessBufferSource` implementations. Before the process API freezes, the active runtime must reject endpoints that:

- target an unknown/inactive audio port;
- use the wrong input/output direction;
- name a channel outside the active layout;
- disagree with the setup-resolved schema index;
- violate a declared relationship/layout invariant.

This validation should use activation-owned resolved metadata, not repeated string lookup or allocation in the callback. The likely representation is a dense activation-time audio configuration indexed by `AudioPortIndex`; finalize it before adding graph/device layers that would otherwise duplicate the same mapping problem.

## Processing

`Processor` exclusively owns mutable active DSP history such as filters, delay lines, envelopes, lookahead buffers, oversampling state, models, and scratch.

`ProcessBlock<S>` borrows:

- callback frame count and process mode;
- canonical base parameter state;
- sample-accurate bounded parameter/note/event sources;
- transport/process context;
- a safe `ProcessBufferSource<S>`.

Adapters/embeddings prove raw pointer/alias facts before safe channel views enter core. Setup owns stable-to-dense endpoint translation. Callback code consumes pre-resolved dense identities without callback-owned channel vectors or stable-string lookup.

`crates/chassis-core/tests/realtime_alloc.rs` mechanically checks post-activation allocation/deallocation on the callback test thread. Full adapter-path allocation/work and performance evidence remains separate.

## Parameter publication

Persistent/authoring identity is `ParameterKey`; realtime identity is schema-local `ParameterIndex`.

The CLAP host lifetime currently splits durable shared/main-thread scalar state from the active audio processor. The adapter therefore owns an adapter-local scalar publication bridge and synchronizes it into a fresh `InstanceRuntime` on activation. This remains deployment plumbing, not a second core state authority.

The bridge uses even completed generations and odd in-progress writer tokens. Realtime publication is one-shot/nonblocking; realtime snapshots have a fixed retry bound; non-realtime control/state snapshots may wait for an in-progress writer.

A realtime automation endpoint is published only from the generation observed before processing. If a newer control edit or state load has advanced the generation, stale realtime publication is rejected.

The long-term adapter refactor should project schema-owned complete state rather than continuing to duplicate semantic state identity in CLAP constants.

## Deactivation and teardown

Deactivation removes and destroys active processor/resources only after the deployment contract has ended process/reset access. Durable semantic state and immutable schema remain for later activation. Active I/O and latency disappear with the active child and are recomputed on the next activation.

Future background tasks, callbacks, editors, deferred reclamation, devices, graphs, and module unload require explicit fencing/shutdown owners before they enter common runtime infrastructure.

## Failure rules

- invalid complete component schema fails at runtime construction;
- component/runtime schema or state-identity drift fails before product activation;
- malformed structural I/O fails before product activation;
- product activation error remains distinct from framework validation error;
- failed activation publishes neither processor nor latency;
- callback dimensions/events are validated before product DSP;
- audio endpoint legality must become a runtime guarantee before the process API freezes;
- complete state replacement is failure-atomic;
- no panic may unwind through a format FFI boundary.

## Remaining freeze gates

1. Finish dense-only audio endpoints and activation-owned endpoint legality validation.
2. Make direct coherent `ComponentSchema` authority the normal `Component` API and remove the default-stereo-effect core assumption/legacy runtime constructors.
3. Migrate stable audio/event keys and display metadata to owned validated forms suitable for dynamic/hosted components.
4. Move deployment adapters to schema-owned semantic state identity and remove explicit-ID compatibility paths.
5. Finish semantic whole-I/O policy for components with multiple legal layouts.
6. Preserve/extend allocation, concurrency, cross-platform, adapter, and real-host evidence as these boundaries change.
