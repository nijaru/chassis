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
  instance-lifetime held external modulation (not persisted)
  active/inactive lifecycle
  accepted active I/O configuration
  activation-scoped LatencySamples
  completed active TailLength snapshot
  optional active Processor

Processor
  exclusive mutable realtime DSP history/resources while active
  caches dense process-time identities/resources established before processing
  reports activation latency and current conservative tail bound

Process<S>
  sample-representation-specific processing over borrowed ProcessBlock<S>
  returns Result<(), ProcessError>

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

`InstanceRuntime::for_component()` is the only public constructor. It obtains one coherent schema, derives parameter defaults from its descriptors, validates/canonicalizes `Component::default_custom_state()`, and calls `Component::validate_state()` over both halves. Identity-free components follow the same admission without a fabricated persistence ID. The default component policy rejects custom fields; products supporting them validate required/unknown fields and cross-field relationships explicitly.

Activation then:

1. obtains the component generation's schema;
2. rejects semantic state identity/version drift;
3. rejects audio/event/parameter schema or audio-I/O-policy drift;
4. validates the proposed structural audio configuration against the runtime-owned audio schema;
5. rejects structurally valid configurations not accepted by the schema-owned `AudioIoPolicy`;
6. resolves accepted stable audio keys to activation-owned dense metadata and owns the configured ports while non-realtime;
7. constructs `ActivationConfig` from runtime-owned data;
8. revalidates complete state through `Component::validate_state()` and calls the single `Component::activate(config, parameters, custom_state)` preparation hook;
9. reads `Processor::latency()` and `Processor::tail()` from the successfully constructed processor;
10. publishes the processor, accepted configuration, resolved endpoint metadata, latency, and initial tail snapshot together as the active child.

A failed activation never publishes a partial active child. Executable negative-space coverage verifies failure leaves the runtime inactive, preserves durable state, and permits later successful activation.

The component definition is intentionally not stored inside `InstanceRuntime`; construction, complete loads, and activation borrow it. A deployment may transfer the active runtime/processor without forcing `Component: Send` in core. Component validation/migrations and their boxed `ComponentStateError` payloads stay strictly off callbacks.

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

Any failure leaves live parameter/custom state and restart status unchanged. Typed documents cannot contain non-finite floats or invalid choice identifiers, including migration output.

Complete loads borrow the component and reject schema drift; `state_migrations()` supplies adjacent migrations rather than the loader supplying a separate policy. Defaults are construction input, not missing-field recovery: a current-schema load must remain complete, and migrations explicitly introduce new required fields.

An accepted active custom-state change moves the first pre-load parameter store and snapshots its consumed modulation off-thread into one activation-owned frozen control projection, then requests reactivation. Until deactivation, `ProcessBlock` receives that projection and no parameter trajectories. Original event bounds/domains are still checked; notes, MIDI, transport, and audio continue. Base events are discarded without durable endpoint completion; accepted modulation endpoints update only the desired held signal for reactivation. Later loads and direct control edits affect saveable desired state only. Reset, repeated loads, and state reversion cannot refresh or release the frozen projection. Deactivation reclaims it with the old resources off-callback; reactivation validates/prepares the latest desired state.

This intentionally ignores base automation for as long as the environment defers restart, while retaining desired modulation independently. Merely freezing base values while forwarding automation is unsafe for coupled resource bounds. Parameter-only loads before this gate remain block-visible without an automatic runtime restart. Processor-originated restart requests retain their existing parameter/automation behavior; they do not activate the custom-state gate.

Persistent state is semantic versioned data, not Rust object layout or transient DSP history.

## Activation-scoped latency

`LatencySamples` is a sample-count newtype. `Processor::latency()` defaults to zero and is queried once after successful processor construction.

The runtime snapshots latency rather than querying mutable processor state during processing. This gives deployment adapters/graphs a stable value for one active lifetime and prevents externally visible latency from drifting without the lifecycle transition needed to rebuild compensation/execution plans.

A processor whose lookahead, convolution, oversampling path, or other activation resource changes latency establishes the new value during the next activation. `Processor::restart_requested()` signals that the active resources are no longer the desired generation while requiring the current processor to remain valid until replacement.

Tail is separate: `active_tail()` exposes the last completed activation/process/reset bound, or `None` while inactive. It follows consumed controls/resources rather than desired replacements. [Runtime services](runtime-services.md#tail) owns its reporting and deployment contract.

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
- name a channel outside the active layout;
- repeat an endpoint, omit an active endpoint, or supply a view with no endpoints.

Activation prepares reusable endpoint-coverage scratch. Every accepted view consumes at least one unique endpoint, bounding validation traversal by the accepted whole configuration; callback frame/event bounds are rejected before that traversal. Silent channels are explicit views, not omitted endpoints.

Adapters creating safe process views remain responsible for their own raw pointer, aliasing, null-buffer, and format-specific proofs before core receives the views. The semantic endpoint legality contract itself is format-independent and activation-owned.

A source must repeat the same finite topology and dimensions across traversals. The current runtime traverses it once for complete endpoint coverage and again for block dimensions/processing. Keep that simple bounded behavior until representative measurement justifies a combined execution plan or stronger source-qualification API.

## Processing

`Processor` exclusively owns mutable active DSP history such as filters, delay lines, envelopes, lookahead buffers, oversampling state, models, and scratch.

`ProcessBlock<S>` borrows:

- callback frame count and process mode;
- canonical consumed base parameter state and held modulation baseline;
- sample-accurate bounded parameter/note/event sources;
- transport/process context;
- a safe `ProcessBufferSource<S>`;
- one optional caller-owned bounded note/MIDI output sink.

Adapters/embeddings prove raw pointer/alias facts before safe channel views enter core. Setup owns stable-to-dense endpoint translation and whole-I/O policy acceptance. Callback code consumes pre-resolved dense identities without callback-owned channel vectors or stable-string lookup.

Outside the pending custom-state gate, successful DSP retains final automation points as durable parameter values. Event-free blocks, state export, and reactivation use those completed values. Failed validation, a returned `ProcessError`, or a processor/tail-query panic commits no endpoints or new tail snapshot. Product errors propagate before the tail hook; audio, DSP history, and already delivered output are not rolled back. CLAP terminates that processing run rather than promising recovery. The completed tail hook runs before retaining base or modulation endpoints. Source ramps remain block-local; sources supply clipped continuation points rather than relying on hidden ramp state.

`crates/chassis-runtime/tests/realtime_alloc.rs` mechanically checks post-activation allocation/deallocation with float/integer/boolean/choice completion, including choice replacement after control edits and state loading. Full adapter-path allocation/work and performance evidence remains separate.

## Deployment state publication

Persistent/authoring identity is `ParameterKey`; realtime identity is schema-local `ParameterIndex`.

CLAP host lifetimes split durable shared/main-thread desired state from the active audio processor. The adapter owns one complete parameter/custom-state projection and admits a coherent snapshot through component policy into a fresh `InstanceRuntime` on activation. This is deployment plumbing, not a second core state authority.

The bridge publishes scalars and a custom-resource epoch in one transaction. Custom payload ownership stays behind a control-only mutex; the callback reads only fixed-width metadata and never adopts or reclaims custom payloads. Realtime publication is one-shot/nonblocking; realtime snapshots have a fixed retry bound. Non-realtime save/activation capture a coherent completed parameter/custom generation before encoding or preparing resources.

A separate bounded scalar projection retains global modulation across CLAP runtime lifetimes; it is excluded from state publication and survives a failed base CAS. It is seeded only after a matching readable base/custom generation, so old resources cannot consume newer desired modulation. Flush updates that desired projection without directly mutating active DSP.

CLAP projects the runtime's completed store rather than replaying automation. Publication uses the generation actually synchronized before processing, so newer control/state publication defeats stale completion. A changed custom epoch gates parameter synchronization and automation until deactivation; desired replacements remain saveable while old resources continue. An already-running old-generation block may finish and establish the frozen local baseline, but cannot overwrite the replacement. Snapshot contention withholds trajectories for that block without permanently latching the gate. [Parameters/state](parameters-state.md#complete-clap-state-publication) owns the full concurrency contract.

CLAP load runs component-owned migrations/complete validation before publishing a candidate. Shared creation validates original defaults against deployment wire bounds; the separately retained definition must admit those defaults rather than substitute its own. Activation and load reject component schema drift. Unsupported event dialects or unrepresentable port mappings still fail explicitly.

## Deactivation and teardown

Deactivation removes and destroys active processor/resources only after the deployment contract has ended process/reset access. Durable semantic state, held external modulation, and immutable schema remain for later activation. Active I/O, latency, and the core tail snapshot disappear with the active child and are recomputed on the next activation.

Future background tasks, callbacks, editors, deferred reclamation, devices, graphs, and module unload require explicit fencing/shutdown owners before they enter common runtime infrastructure.

## Failure rules

- invalid complete component schema fails at runtime construction;
- component/runtime schema, audio policy, or state-identity drift fails before product activation;
- malformed structural I/O or an unsupported whole-I/O configuration fails before product activation;
- product activation error remains distinct from framework validation error;
- failed activation publishes neither processor nor latency/tail metadata;
- callback dimensions, parameter/note/MIDI events and payload bounds, and dense audio endpoint legality are validated before product DSP;
- complete state replacement is failure-atomic;
- no panic may unwind through a format FFI boundary.

## Remaining runtime/core freeze gates

The ownership/schema/identity migration is no longer the blocker. Remaining core work should be driven by concrete audio semantics:

1. extend the qualified global float modulation subset only when a client needs per-note parameter modulation or other domains; keep external signals separate from durable state;
2. add MIDI 2/UMP contracts beyond the implemented native notes and MIDI 1/SysEx without lossy down-conversion;
3. add bounded control -> DSP non-parameter publication and background-work/reclamation lifecycle;
4. add bus/port convenience or a richer `AudioIoPolicy` only if materially different clients show the current explicit model is insufficient;
5. continue cross-platform, allocation, concurrency, adapter, and real-host qualification as these contracts change.

The current `ComponentSchema` / `InstanceRuntime` ownership shape is a candidate-stable architecture. Do not reopen it merely to add speculative generality; change it when a concrete client exposes a semantic or ergonomic deficiency.
