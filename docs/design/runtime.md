# Runtime Ownership Model

Status: `InstanceRuntime<P>` is the durable format-independent authority for one component instance. It owns one validated `ComponentSchema`, canonical mutable semantic state derived from that schema, and the active processor lifecycle. The proof-era constructor, endpoint-identity, audio-policy, metadata-ownership, and duplicate state-identity migrations are complete; exact public API spelling remains pre-alpha while the remaining cross-cutting audio contracts are finished.

## Runtime roles

```text
Component
  immutable component definition / processor factory
  constructs one coherent ComponentSchema

ComponentSchema
  semantic state identity/version when identified
  owned-capable audio-port schema
  whole-component AudioIoPolicy
  owned-capable event-port schema
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

`ComponentSchema` is the immutable authority for one component generation. It validates and owns the audio/event/parameter schemas and whole-audio-I/O policy together rather than allowing independently evolving metadata families.

Persistent/setup identity and process-time identity are separate:

```text
PortKey       -> AudioPortIndex
EventPortKey  -> EventPortIndex
ParameterKey  -> ParameterIndex
```

Stable keys belong to schema, persistence, host mapping, tooling, and author-facing lookup. Audio/event keys and display metadata can be borrowed static data or runtime-owned metadata. Dense indices are schema-local process/setup projections and are never persistence identity.

`ComponentSchema::unidentified()` is an intentional identity-free form for transient/embedded components that do not persist semantic state. Deployments that save state may require an identified schema at their boundary.

The schema/runtime model is now exercised directly by conventional effects, zero-audio event processors, instruments, multiple output buses, multiple legal layouts with sidechain, runtime-owned hosted metadata, and CLAP deployment. Those proofs did not require parallel component lifecycles or a more general I/O-policy language.

## Construction and activation

The preferred format-independent path is:

```rust,ignore
let mut runtime = InstanceRuntime::for_component(&component)?;
runtime.activate(&component, process_config, requested_audio_io)?;
```

`InstanceRuntime::for_component()` asks the component for one coherent schema, validates it, stores the complete schema, and constructs canonical base parameter state from the schema's parameter descriptors.

Activation then:

1. obtains the component generation's schema;
2. rejects semantic state identity/version drift;
3. rejects audio/event/parameter schema or audio-I/O-policy drift;
4. validates the proposed structural audio configuration against the runtime-owned audio schema;
5. rejects structurally valid configurations not accepted by the schema-owned `AudioIoPolicy`;
6. resolves accepted stable audio keys to activation-owned dense metadata and owns the configured ports while non-realtime;
7. constructs `ActivationConfig` from runtime-owned data;
8. calls `Component::activate_with_state()` with the current complete semantic state;
9. reads `Processor::latency()` from the successfully constructed processor;
10. publishes the processor, accepted configuration, resolved endpoint metadata, and latency snapshot together as the active child.

A failed activation never publishes a partial active child. Executable negative-space coverage verifies failure leaves the runtime inactive, preserves durable state, and permits later successful activation.

The component definition is intentionally not stored inside `InstanceRuntime`. A deployment may transfer the active runtime/processor according to its thread contract without forcing `Component: Send` in core.

## Semantic state identity

Complete runtime state derives semantic identity and schema version from the owned `ComponentSchema`:

```text
ComponentSchema
  StateIdentity
    ComponentId
    StateSchemaVersion
         |
         v
InstanceRuntime state save/load/migration
```

Runtime-level complete-state operations have one identity authority and do not accept duplicate product identity/version arguments:

- `state_document()`;
- `encode_state(...)`;
- `apply_state(...)`;
- `apply_state_bytes(...)`.

The old runtime `*_for_product`, parameter-only state export/load, and duplicate load-error surfaces are removed. `ParameterStore` may still accept explicit identity internally as a serialization primitive; it is not a competing component identity authority.

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

Stable audio configuration is selected while inactive from `PortKey` + layout metadata and accepted by the schema-owned `AudioIoPolicy`. Dense `AudioPortIndex` is the process-time identity.

The runtime resolves stable keys to dense indices once and owns a `ResolvedAudioIoConfiguration` for the active lifetime. `InputEndpoint` / `OutputEndpoint` carry only `AudioPortIndex` + channel:

```text
InputEndpoint
  AudioPortIndex
  channel

OutputEndpoint
  AudioPortIndex
  channel
```

Before product DSP runs, `InstanceRuntime` rejects callback endpoints that:

- target an unknown or inactive audio port;
- use the wrong input/output direction;
- name a channel outside the active layout.

Adapters creating safe process views remain responsible for their own raw pointer, aliasing, null-buffer, and format-specific proofs before core receives the views. The semantic endpoint legality contract itself is format-independent and activation-owned.

The current runtime may traverse a buffer source once for endpoint legality and again for block dimensions/processing. Keep that simple bounded behavior until representative measurement justifies a combined execution plan or stronger source-qualification API.

## Processing

`Processor` exclusively owns mutable active DSP history such as filters, delay lines, envelopes, lookahead buffers, oversampling state, models, and scratch.

`ProcessBlock<S>` borrows:

- callback frame count and process mode;
- canonical base parameter state;
- sample-accurate bounded parameter/note/event sources;
- transport/process context;
- a safe `ProcessBufferSource<S>`.

Adapters/embeddings prove raw pointer/alias facts before safe channel views enter core. Setup owns stable-to-dense endpoint translation and whole-I/O policy acceptance. Callback code consumes pre-resolved dense identities without callback-owned channel vectors or stable-string lookup.

`crates/chassis-core/tests/realtime_alloc.rs` mechanically checks post-activation allocation/deallocation on the callback test thread. Full adapter-path allocation/work and performance evidence remains separate.

## Parameter publication

Persistent/authoring identity is `ParameterKey`; realtime identity is schema-local `ParameterIndex`.

The CLAP host lifetime currently splits durable shared/main-thread scalar state from the active audio processor. The adapter therefore owns an adapter-local scalar publication bridge and synchronizes it into a fresh `InstanceRuntime` on activation. This is deployment plumbing, not a second core state authority.

The bridge uses even completed generations and odd in-progress writer tokens. Realtime publication is one-shot/nonblocking; realtime snapshots have a fixed retry bound; non-realtime control/state snapshots may wait for an in-progress writer.

A realtime automation endpoint is published only from the generation observed before processing. If a newer control edit or state load has advanced the generation, stale realtime publication is rejected.

Schema-owned semantic state identity is already used by CLAP save/load. Future adapter work should continue reducing duplicated projection mechanics without moving mutable semantic authority out of `InstanceRuntime`.

## Deactivation and teardown

Deactivation removes and destroys active processor/resources only after the deployment contract has ended process/reset access. Durable semantic state and immutable schema remain for later activation. Active I/O and latency disappear with the active child and are recomputed on the next activation.

Future background tasks, callbacks, editors, deferred reclamation, devices, graphs, and module unload require explicit fencing/shutdown owners before they enter common runtime infrastructure.

## Failure rules

- invalid complete component schema fails at runtime construction;
- component/runtime schema, audio policy, or state-identity drift fails before product activation;
- malformed structural I/O or an unsupported whole-I/O configuration fails before product activation;
- product activation error remains distinct from framework validation error;
- failed activation publishes neither processor nor latency;
- callback dimensions, parameter/note events, and dense audio endpoint legality are validated before product DSP;
- complete state replacement is failure-atomic;
- no panic may unwind through a format FFI boundary.

## Remaining runtime/core freeze gates

The ownership/schema/identity migration is no longer the blocker. Remaining core work should be driven by concrete audio semantics:

1. finish parameter formatting/value mapping and explicit sample-accurate modulation without conflating it with durable/base state;
2. define common tail and bypass semantics only where deployments can map them faithfully;
3. finish the event/output/expression/MIDI contracts needed by real instruments and event processors;
4. add bounded control -> DSP non-parameter publication and background-work/reclamation lifecycle;
5. add bus/port convenience or a richer `AudioIoPolicy` only if materially different clients show the current explicit model is insufficient;
6. continue cross-platform, allocation, concurrency, adapter, and real-host qualification as these contracts change.

The current `ComponentSchema` / `InstanceRuntime` ownership shape is a candidate-stable architecture. Do not reopen it merely to add speculative generality; change it when a concrete client exposes a semantic or ergonomic deficiency.
