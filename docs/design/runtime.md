# Runtime Ownership Model

Status: `InstanceRuntime<P>` is the durable format-independent authority for one component instance. The current CLAP adapter projects host-visible scalar state into that runtime during activation; its scalar publication protocol is Loom-qualified. Public API spelling remains pre-alpha.

## Runtime roles

```text
Component
  immutable schema / metadata / capabilities / processor factory

InstanceRuntime<P>
  durable ParameterStore
  durable validated custom semantic state
  active/inactive lifecycle
  accepted active I/O configuration
  activation-scoped LatencySamples
  optional active Processor

Processor
  exclusive mutable realtime DSP history/resources while active
  reports latency established by this activation

Process<S>
  sample-representation-specific processing over borrowed ProcessBlock<S>

Deployment publication bridge
  adapter-only when host object lifetimes require cross-domain synchronization
```

Each mutable semantic guarantee has one authority. A host/shared/editor representation is a synchronized projection, not an independently mutable second owner.

## Construction and activation

`InstanceRuntime::new()` validates the immutable parameter schema before product activation. `parameters_mut()` returns a value-only mutation view: callers can update or reset validated values but cannot replace the store or its schema. Activation then:

1. verifies the component schema still matches the runtime;
2. validates the proposed structural audio configuration;
3. copies accepted configured ports while non-realtime;
4. constructs `ActivationConfig` from runtime-owned data;
5. calls `Component::activate_with_state()` with the current complete semantic state;
6. reads `Processor::latency()` from the successfully constructed processor;
7. publishes the processor, accepted configuration, and latency snapshot together as the active child.

`Component::activate()`, `activate_with_parameters()`, and `activate_with_state()` are layered product hooks. Products override the narrowest one they need.

The component definition is not stored inside `InstanceRuntime`. A deployment may therefore transfer the active runtime/processor according to its own thread contract without forcing `Component: Send` in core.

Activation failure does not publish a partial active child. Executable negative-space coverage verifies a failed activation leaves the runtime inactive, preserves durable state, and permits a later successful activation.

## Activation-scoped latency

`LatencySamples` is a sample-count newtype. `Processor::latency()` defaults to zero and is queried once after successful processor construction.

The runtime deliberately snapshots latency rather than querying mutable processor state during processing. This gives deployment adapters a stable value for one active lifetime and prevents host-visible latency from drifting without the lifecycle transition required by plugin formats.

A processor whose lookahead, convolution, oversampling path, or other activation resource changes latency must establish the new value during the next activation. The CLAP adapter exposes the latency extension, updates its main-thread latency value after successful activation, and calls the host latency `changed` callback when that activation produces a different value.

The current conformance processor reports zero. Nonzero PDC qualification must use DSP that actually delays output by the declared number of samples.

## Durable state

Normal project/preset load is complete transactional replacement:

```text
bytes
 -> bounded decode
 -> adjacent product-schema migrations
 -> temporary complete parameter candidate
 -> temporary canonical custom-state candidate
 -> framework validation
 -> product validation over both candidates
 -> publish both together
```

Any failure leaves live parameter and custom state unchanged.

Persistent state is semantic, versioned data rather than Rust object layout or transient DSP history.

## Processing

`Processor` exclusively owns mutable active DSP history such as filters, delay lines, envelopes, lookahead buffers, oversampling state, and scratch.

`ProcessBlock<S>` borrows:

- the active process configuration;
- canonical base parameter state;
- sample-sorted bounded parameter events;
- transport/process context;
- a safe `ProcessBufferSource<S>`.

Adapters prove host pointer/alias facts before safe channel views enter core. Setup owns stable endpoint-to-dense-slot translation. Callback code consumes those pre-resolved mappings without callback-owned channel vectors.

`crates/chassis-core/tests/realtime_alloc.rs` provides a mechanical post-activation allocation/deallocation check on the callback test thread. That is core-path evidence; full adapter-path allocation/work and performance evidence remains separate.

## Parameter publication

Persistent/authoring identity is `ParameterKey`; realtime identity is schema-local `ParameterIndex`.

The CLAP host lifetime splits durable shared/main-thread state from the active audio processor. The adapter therefore owns an adapter-local scalar publication bridge and synchronizes it into a fresh `InstanceRuntime` on activation.

The bridge uses even completed generations and odd in-progress writer tokens. Realtime publication is one-shot/nonblocking; realtime snapshots have a fixed retry bound; non-realtime control/state snapshots may wait for an in-progress writer.

A realtime automation endpoint is published only from the generation observed before processing. If a newer control edit or state load has already advanced the generation, the stale realtime publication is rejected. The adapter captures that token before synchronizing block state and entering product DSP; reading a fresh token at completion would incorrectly let older automation replace a state loaded during processing.

`u64::MAX - 1` is the terminal stable generation; generation wraparound is not a correctness assumption.

`CLAP_MAX_INPUT_EVENTS` bounds all input-event inspection, including unknown event types and parameter IDs, separately from `CLAP_MAX_PARAMETER_EVENTS` normalization capacity. The default total bound is 1,024; products whose declared parameter budget exceeds it must raise the total bound. Process rejects an oversized batch before DSP, while parameter flush rejects the complete batch without mutation because that CLAP callback cannot return failure. Targeted parameter values (port/channel/key/note ID) are unsupported: process rejects them and flush ignores them instead of turning them into global changes.

## Active state save

State save uses the same publication authority:

- the audio thread never waits for save;
- the non-realtime save path obtains one coherent completed scalar generation;
- a save racing one process block may linearize immediately before or immediately after that block's endpoint publication;
- it cannot accept a mixed cross-parameter generation;
- a newer control/state generation defeats stale realtime completion.

Local executable publication tests cover coherent completed snapshots, an in-progress multi-value write, stale-generation rejection, bounded realtime attempts, and terminal generation behavior. Native active-save host qualification remains a separate Phase-2 gate.

## Deactivation and teardown

Deactivation removes and destroys active processor/resources only after the deployment contract has ended process/reset access. Durable semantic state remains for later activation; active I/O and latency disappear with the active child and are recomputed on the next activation.

Future background tasks, callbacks, editors, deferred reclamation, and module unload require explicit fencing/shutdown owners before they enter common runtime infrastructure.

## Failure rules

- invalid parameter schema fails at runtime construction;
- malformed structural I/O or component/runtime schema mismatch fails before product activation;
- product activation error remains distinct from framework validation error;
- failed activation publishes neither processor nor latency;
- callback dimensions/events are validated before product DSP;
- complete state replacement is failure-atomic;
- no panic may unwind through a format FFI boundary.

## Remaining freeze gates

- decide final authoring ergonomics for constructing a runtime from a component as real clients accumulate;
- finish semantic whole-I/O policy for products with multiple accepted layouts when a real client requires it;
- extend mechanical realtime evidence through the full CLAP adapter and representative workloads;
- qualify active save and automated rendered output through a real CLAP host;
- qualify nonzero latency/PDC with actual delayed DSP;
- let real FX clients determine higher-level runtime conveniences.
