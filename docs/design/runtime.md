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
  optional active Processor

Processor
  exclusive mutable realtime DSP history/resources while active

Process<S>
  sample-representation-specific processing over borrowed ProcessBlock<S>

Deployment publication bridge
  adapter-only when host object lifetimes require cross-domain synchronization
```

Each mutable semantic guarantee has one authority. A host/shared/editor representation is a synchronized projection, not an independently mutable second owner.

## Construction and activation

`InstanceRuntime::new()` validates the immutable parameter schema before product activation. Activation then:

1. verifies the component schema still matches the runtime;
2. validates the proposed structural audio configuration;
3. copies accepted configured ports while non-realtime;
4. constructs `ActivationConfig` from runtime-owned data;
5. calls `Component::activate_with_state()` with the current complete semantic state;
6. publishes the resulting processor as the active child only after success.

`Component::activate()`, `activate_with_parameters()`, and `activate_with_state()` are layered product hooks. Products override the narrowest one they need.

The component definition is not stored inside `InstanceRuntime`. A deployment may therefore transfer the active runtime/processor according to its own thread contract without forcing `Component: Send` in core.

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

## Parameter publication

Persistent/authoring identity is `ParameterKey`; realtime identity is schema-local `ParameterIndex`.

The CLAP host lifetime splits durable shared/main-thread state from the active audio processor. The adapter therefore owns an adapter-local scalar publication bridge and synchronizes it into a fresh `InstanceRuntime` on activation.

The bridge uses even completed generations and odd in-progress writer tokens. Realtime publication is one-shot/nonblocking; realtime snapshots have a fixed retry bound; non-realtime control/state snapshots may wait for an in-progress writer.

A realtime automation endpoint is published only from the generation observed before processing. If a newer control edit or state load has already advanced the generation, the stale realtime publication is rejected.

`u64::MAX - 1` is the terminal stable generation; generation wraparound is not a correctness assumption.

## Active state save

State save uses the same publication authority:

- the audio thread never waits for save;
- the non-realtime save path obtains one coherent completed scalar generation;
- a save racing one process block may linearize immediately before or immediately after that block's endpoint publication;
- it cannot accept a mixed cross-parameter generation;
- a newer control/state generation defeats stale realtime completion.

Local executable publication tests cover coherent completed snapshots, an in-progress multi-value write, and stale-generation rejection. Native active-save host qualification remains a separate Phase-2 gate.

## Deactivation and teardown

Deactivation removes and destroys active processor/resources only after the deployment contract has ended process/reset access. Durable semantic state remains for later activation; active I/O storage may be replaced on the next activation.

Future background tasks, callbacks, editors, deferred reclamation, and module unload require explicit fencing/shutdown owners before they enter common runtime infrastructure.

## Failure rules

- invalid parameter schema fails at runtime construction;
- malformed structural I/O or component/runtime schema mismatch fails before product activation;
- product activation error remains distinct from framework validation error;
- callback dimensions/events are validated before product DSP;
- complete state replacement is failure-atomic;
- no panic may unwind through a format FFI boundary.

## Remaining freeze gates

- remove the temporary activation-local `Activated<P>` / free `runtime::activate()` compatibility surface after source cleanup;
- finish semantic whole-I/O policy for products with multiple accepted layouts;
- add mechanical realtime allocation/work-bound evidence;
- qualify active save and automated rendered output through a real CLAP host;
- let real FX clients determine any higher-level runtime conveniences.
