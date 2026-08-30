# Runtime Ownership Model

Status: the durable runtime/state/parameter architecture is qualified through `62b96cc`. CLAP scalar publication is adapter-local and Loom-qualified. Complete parameter + custom semantic state is owned transactionally by `InstanceRuntime`. General CLAP I/O is the current implementation slice. Public API remains pre-alpha and is not frozen.

## Goal

Make ownership, mutation authority, replacement, and teardown difficult to get wrong while keeping ordinary plugin authors focused on product DSP.

The framework may span several thread/capability domains, but each mutable semantic guarantee has one defined authority and publication order.

## Runtime roles

```text
Component definition
  immutable schema / metadata / capabilities / factories
  borrowed while preparing activation

InstanceRuntime<P>
  durable ParameterStore
  durable validated custom semantic StateEntry values
  active/inactive lifecycle
  owned accepted active I/O configuration
  optional active Processor

Processor
  exclusive mutable realtime DSP/runtime history while active
  activation-time resources and scratch

Process<S>
  sample-representation-specific processing over borrowed ProcessBlock<S>

Deployment publication bridge
  only where host lifetimes require cross-domain synchronization
  projects host/control state into the runtime authority
```

A simple embedded/standalone effect can retain one `InstanceRuntime<P>` directly. A plugin format whose host object lifetimes separate durable control objects from its active audio object may require a synchronized projection, but that projection must not become a second independently mutable semantic authority.

## Component and activation

`Component` describes a product and creates processors; it is not the live mutable instance.

Activation hooks are layered for compatibility:

- `Component::activate()` — simplest product preparation;
- `activate_with_parameters()` — additionally sees validated durable base parameters;
- `activate_with_state()` — additionally sees validated canonical custom semantic state.

Each default delegates to the earlier hook. Products only override the narrowest hook they need.

The component definition is deliberately not stored inside `InstanceRuntime<P>`. A deployment can therefore move an active runtime/processor across threads without accidentally requiring the immutable component definition itself to be `Send`.

## Instance runtime authority

`InstanceRuntime<P>` owns:

- immutable validated parameter descriptors and durable values through `ParameterStore`;
- validated non-`parameter/` semantic `StateEntry` values in canonical key order;
- active/inactive lifecycle;
- active process resource bounds;
- an owned copy of accepted `ConfiguredAudioPort` values;
- the active processor while one exists.

Construction validates parameter schema once. Activation verifies component/runtime schema compatibility, structurally validates I/O, copies accepted port configuration while non-realtime, then constructs the processor from the current complete semantic state.

Deactivation destroys active processor/resources only. Durable parameter and custom semantic state remain for later activation.

The older `Activated<'a, P>` convenience path remains activation-local and must not become persistent authority.

## State replacement

Normal project/preset state load is complete replacement, not a patch.

The qualified complete path is:

```text
bytes
 -> bounded StateDocument decode
 -> adjacent product-schema migrations
 -> temporary complete current parameter candidate
 -> temporary canonical custom-state candidate
 -> framework parameter validation
 -> product validation over both candidates
 -> publish both together
```

`InstanceRuntime::apply_state_for_product()` and `apply_state_bytes()` publish parameter + custom state only after every check succeeds. Product validation receives both candidates, allowing cross-field invariants without mutating live state. Failure at decode, migration, framework validation, or product validation leaves both live domains unchanged.

Persistence remains semantic rather than Rust-layout based. `InstanceRuntime` stores canonical `StateEntry` values; it is not generic over arbitrary serialized product structs.

Parameter-only state methods remain compatibility helpers for adapters still projecting only framework parameters. They intentionally leave custom state unchanged.

## Parameter identity

Persistent state and authoring use stable `ParameterKey` values. Runtime/process work uses schema-local dense `ParameterIndex` values.

`ParameterStore` resolves stable keys during setup/control work. CLAP bindings retain their `ParameterIndex`, and normalized process events use dense indices. `ParameterStore::set_index()` permits an already-resolved projection to update the validated base store without another string lookup.

The bounded globally sample-sorted event slice remains the process representation until measurement justifies another per-parameter event index.

## Activation and I/O ownership

Activation establishes sample rate, block bounds, parameter-event bounds, accepted audio configuration, and product-owned resources.

`InstanceRuntime` copies accepted `ConfiguredAudioPort` values before activation. `ActivationConfig` then borrows runtime-owned storage. Setup-time allocation is permitted; callback-time port-vector allocation is not.

`AudioIoConfiguration::validate` proves structural facts—known/unique/required ports—not all product-specific layout policy.

The CLAP adapter is now moving stable port translation into a setup-owned mapping. Explicit CLAP audio IDs are separate from direction-local dense indices, mirroring the stable-vs-runtime identity split used for parameters.

The current `ProcessBlock` still borrows a flat `&mut [ChannelBuffer<'_, S>]`. That works well for statically bounded topologies but creates a real constraint for arbitrary runtime port/channel counts: lifetime-bearing channel views cannot be retained in processor storage, while constructing a `Vec<ChannelBuffer>` per callback would violate realtime allocation rules. General CLAP I/O must therefore prove a no-allocation borrow shape rather than hide this constraint.

## Processor and process ownership

While active, `Processor` exclusively owns mutable DSP history such as filters, delay lines, envelopes, oscillators, lookahead buffers, and scratch. Editor/state/control code never receives unrestricted mutable processor access.

`ProcessBlock` contains safe `ChannelBuffer` views whose aliasing has already been proved by the adapter/runtime boundary. Its constructor validates callback-varying facts such as frame count, parameter-event bounds/context, channel slice lengths, and parameter event values.

Stable endpoint translation belongs to setup. Callback code should consume dense/setup-resolved mappings rather than scan semantic schemas or allocate.

## CLAP host lifetime and scalar publication

CLAP keeps shared/main-thread objects alive while the audio processor exists only during activation. Therefore durable host-visible state cannot live only inside the active `InstanceRuntime`.

The adapter keeps a CLAP-local synchronized scalar projection across those host domains. On activation it creates a fresh runtime, synchronizes durable published scalar state into it, then activates the product from that state.

The publication protocol uses even completed generations and odd writer tokens:

```text
writer
  CAS G -> G+1       AcqRel / Acquire failure
  payload stores      Release
  store G+2           Release
  set pending          Release

reader/sync
  consume pending with CAS true -> false  AcqRel / Acquire failure
  load generation      Acquire
  load payload          Acquire
  load generation      Acquire
  accept same even generation only
```

Realtime writer acquisition is one-shot/nonblocking; realtime snapshots use a fixed retry bound. Control/state paths may wait for a short in-flight writer.

`u64::MAX - 1` is the terminal stable generation. No wraparound/ABA assumption remains.

Loom qualification found and fixed a real notification race: `pending.swap(false, AcqRel)` could write `false` while failing to observe a concurrent `true`, erasing a notification. The consumer now uses `compare_exchange(true, false, AcqRel, Acquire)` so an unsuccessful observation is read-only. All five Loom models passed after the fix.

This scalar bridge remains adapter-local. A deployment-independent publication primitive would additionally need typed non-scalar semantics and a reclamation strategy.

## Deactivation and destruction

At deactivation:

- no process/reset borrow may remain in flight according to the backend contract;
- processor-owned resources are destroyed in a legal domain;
- durable semantic state remains alive at the runtime or deployment-publication level;
- active I/O storage is released and may be replaced on the next activation.

Background tasks, native callbacks, editors, deferred reclamation, and module unload require explicit future fencing/shutdown contracts.

## Failure rules

- malformed structural I/O fails before product activation;
- component/runtime parameter-schema mismatch fails before activation;
- product activation errors remain distinct from framework validation errors;
- callback dimension/event violations fail before product DSP;
- complete state replacement is transactional across framework and product fields;
- no panic may unwind through a format FFI boundary.

## Validation status

The full local Rust gate passed through `62b96cc`, including formatting, workspace tests, strict all-feature/all-target Clippy, cargo-deny, cargo-machete, and the release CLAP conformance build. Cargo-deny reported only the existing unmatched-license-allowance warnings.

That checkpoint covers durable runtime ownership, dense parameter identity, Loom-qualified CLAP scalar publication, bounded state parsing, adjacent migration, checked-in golden state bytes, adversarial state inputs, complete parameter/custom semantic ownership, failure atomicity, state export, and activation visibility.

General CLAP audio mapping/sidechain work after `62b96cc` is implemented on `main` but remains unqualified until the next full local gate passes.
