# Runtime Ownership Model

Status: durable single-owner `InstanceRuntime` and owned activation I/O are implemented in core and the CLAP audio path now activates through that runtime. The current source has not yet passed the post-refactor local Rust gate, and CLAP cross-domain scalar publication remains an adapter-local provisional mechanism. Public API is pre-alpha and not frozen.

## Goal

Make ownership, mutation authority, replacement, and teardown difficult to get wrong while keeping ordinary plugin authors focused on product DSP.

The framework may have several thread/capability domains, but each mutable semantic guarantee has one defined authority/publication order.

## Runtime roles

```text
Component definition
  immutable schema/metadata/capabilities/factories
  borrowed only while preparing activation

InstanceRuntime<P>
  durable ParameterStore for one core instance
  active/inactive lifecycle
  owned accepted active I/O configuration
  optional active Processor

Processor
  exclusive mutable realtime DSP/runtime history while active
  activation-time resources/scratch

Process<S>
  sample-representation-specific process capability over borrowed ProcessBlock<S>

Deployment publication bridge
  only where a host requires cross-domain lifetime/synchronization
  projects host/control state into InstanceRuntime before/during activation
```

A simple embedded/standalone effect can retain one `InstanceRuntime<P>` directly across activation cycles. A plugin format whose host object lifetimes split durable control state from the active audio object may require a synchronized publication layer; that layer must remain a projection with a defined publication order rather than an unrelated semantic authority.

## Component definition

`Component` is not the live mutable plugin instance. It describes/provides stable product schema and creates processors.

`Component::activate()` remains the simplest activation hook. `Component::activate_with_parameters()` additionally receives the runtime's current validated `ParameterStore` and defaults to `activate()`. This lets state loaded before activation influence preparation without moving persistent state ownership into `Processor`.

The component definition itself is deliberately **not** stored inside `InstanceRuntime<P>`. A deployment such as CLAP may move the active runtime/processor across threads while keeping the component definition on its main/control thread. The deployment therefore needs `P: Send` only when its lifecycle actually transfers `P`; it does not accidentally require `Component: Send`.

## Instance runtime

`InstanceRuntime<P>` now owns:

- the validated immutable parameter schema and durable base/control values through `ParameterStore`;
- whether a processor is active;
- the active process resource bounds;
- an owned copy of the accepted `ConfiguredAudioPort` values;
- the active processor while one exists.

Construction validates the parameter schema once. Activation borrows a compatible `Component<Processor = P>`, verifies that its parameter schema still matches the runtime schema, structurally validates proposed I/O, copies the accepted port configuration while non-realtime, and constructs the processor from the current base state.

Deactivation destroys only active processor/resources. The runtime's parameter state remains alive for a later activation.

The older `Activated<'a, P>` and free `activate()` path remain temporarily so existing code can migrate. They are explicitly activation-local and must not be treated as persistent state authority. The CLAP production path has moved to `InstanceRuntime`.

## Activation and I/O ownership

Activation establishes:

- sample rate;
- optional guaranteed positive minimum block size;
- non-zero maximum block size;
- maximum normalized parameter event count;
- accepted audio I/O configuration;
- product-owned precomputation/resources.

`InstanceRuntime` copies accepted `ConfiguredAudioPort` values into active storage before processor construction. `ActivationConfig` is then only a borrowed view over runtime-owned storage.

This removes the dynamic-I/O lifetime trap where an active runtime would otherwise have to borrow adapter-owned `Vec<ConfiguredAudioPort>` storage for its entire lifetime. Setup-time allocation is allowed; callback-time port-vector allocation is not.

Whole-layout policy is still not frozen. `AudioIoConfiguration::validate` proves structural facts such as known/unique/required ports, not whether every representable layout is semantically supported by a particular product.

## Processor ownership

While active, `Processor` is exclusively owned by the processing domain. It owns transient mutable DSP history such as filters, delay lines, envelopes, oscillators, lookahead buffers, and preallocated scratch.

Editor/state/control code never receives unrestricted mutable access to it.

`reset` preserves durable/control state while resetting transient DSP history. Stateless processors may use the default no-op implementation.

A processor may inspect current base parameter state through `ProcessBlock::parameters()`. Products that derive realtime-safe resources from changing base values must keep those resources coherent with process-time changes. Heavy background preparation/replacement requires a later explicit generation/reclamation capability; `activate_with_parameters()` does not imply that active state changes automatically rebuild arbitrary processor internals.

## Process block ownership

`ProcessBlock` borrows safe `ChannelBuffer<S>` views, the current base `ParameterStore`, and `ProcessContext`.

Its constructor is framework-private. Product code can inspect/process the borrowed block but cannot manufacture a fake framework block directly.

Per-call validation covers facts that can change each callback:

- callback frame count against activation min/max guarantees;
- event context frame count;
- activation-owned event count bound;
- every supplied channel slice length;
- parameter event values against the current schema.

Stable endpoint/port translation should be resolved during setup, not by rescanning semantic endpoints with allocation or unbounded work each callback.

The current parameter event prototype still carries stable string keys. Before the process API freezes or high-count automation is promoted, schema/runtime setup should resolve `ParameterKey` values to dense `ParameterIndex` values. Persistent state/authoring continue to use stable keys; realtime matching/cursors use dense indices.

## State replacement

`InstanceRuntime::apply_parameter_state_for_product()` is the current full-state runtime boundary.

It:

1. rebuilds a temporary candidate from validated schema defaults;
2. validates product identity/schema and every supplied parameter entry;
3. rejects unknown parameters;
4. requires exactly one entry for every framework-managed current parameter;
5. swaps the candidate into the runtime only after all validation succeeds.

Failure leaves the current runtime state unchanged. The lower-level `ParameterStore::apply_state_for_product()` remains patch-like and should be treated as a semantic helper rather than the final plugin/project state-load contract.

Future migration handling belongs before the complete-current-state check:

```text
bytes
 -> bounded decode
 -> migrate semantic product schema
 -> materialize complete current state
 -> validate
 -> publish
```

## CLAP host lifetime

CLAP separates durable shared/main-thread objects from the audio-processor object that exists only while activated. Therefore storing the only durable state inside the CLAP audio processor would lose it on deactivate and recreate the original bug.

The current CLAP scalar bridge instead keeps a synchronized adapter-local publication object across those host domains. On each activation:

1. create an `InstanceRuntime<P>` from the component schema;
2. synchronize the durable published CLAP scalar state into the runtime;
3. construct the processor through `InstanceRuntime::activate()` so preloaded state is visible to preparation;
4. keep the runtime as the semantic audio-domain projection while active.

During processing, host/control publication is synchronized into the runtime at bounded block boundaries. Automation endpoints publish back only if the generation they were derived from is still current.

This bridge is deliberately not yet generalized into core. The current scalar implementation is useful evidence, but a deployment-independent publication primitive must handle typed parameter semantics, progress, ordering, and reclamation rather than simply standardizing the first CLAP `f64` representation.

## Cross-domain generation requirements

Any generalized publication mechanism must preserve these rules:

1. control edits and accepted state replacements have a total publication order;
2. an active process block observes one coherent base generation at its synchronization boundary;
3. sample-accurate automation derives from that base plus host events;
4. the resulting automation endpoint may publish only if the observed generation is still current;
5. stale realtime/asynchronous completion never overwrites newer state;
6. state save observes one completed generation and never serializes arbitrary live `Processor` fields;
7. the audio thread never spins or waits for a control writer;
8. replaced nontrivial resources are not accidentally reclaimed on the audio thread.

The current CLAP scalar bridge implements writer serialization, coherent snapshots, and stale-generation rejection locally. Before promoting a subtle atomic primitive into `chassis-core`, model/property-test its ordering and progress behavior (for example with Loom).

## Deactivation and destruction

At deactivation:

- no process/reset borrow may remain in flight according to the backend contract;
- processor-owned resources are destroyed only in a legal domain;
- durable base/control state remains alive at the instance or deployment-publication level;
- active I/O storage is released and can be replaced by the next activation.

Plugin/module unload becomes stricter once background tasks, native callbacks, timers, editors, or deferred reclamation exist.

## Background work / shared state / editor

These remain outside the base runtime until real clients prove them.

When added:

- shared/editor state exposes explicit projections/capabilities, not general mutable processor access;
- background work is tied to instance/module generations and fenced at teardown;
- cancellation does not substitute for preventing stale completion;
- large replaced objects are reclaimed off the audio thread;
- no hidden process-global executor exists without a dylib unload/shutdown contract.

## Failure rules

- malformed structural I/O fails before product activation;
- mismatched component/runtime parameter schema fails before product activation;
- product activation errors are distinct from framework validation errors;
- callback dimension/event violations fail before product DSP runs;
- state failure is transactional;
- no panic may unwind through a format FFI boundary.

## Validation status

New external `instance_runtime` conformance tests cover:

- durable base parameters across deactivate/reactivate;
- activation observing current base state;
- owned dynamic audio configuration outliving caller storage;
- complete transactional state replacement;
- processing from durable base state.

The CLAP adapter has been migrated to activate through `InstanceRuntime` and synchronize host-published state before processor creation.

These changes are **implemented but not yet compile-qualified** in the current checkpoint. A temporary GitHub Actions workflow was attempted, but the job failed before a runner or any step started and therefore provided no Rust evidence. The next gate is local `fmt`/workspace tests/Clippy/deny/machete/release conformance build, followed by dense parameter-index work and then native CLAP qualification.
