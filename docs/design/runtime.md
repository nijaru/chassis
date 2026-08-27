# Runtime Ownership Model

Status: design direction; trait names/signatures are not frozen.

## Goal

Make ownership, mutation authority, replacement, and teardown difficult to get wrong while keeping ordinary plugin authors focused on product DSP.

The framework may have several thread/capability domains, but each mutable guarantee has one owner.

## Runtime roles

```text
Component definition
  immutable schema/metadata/capabilities/factories

InstanceRuntime (framework-owned)
  lifecycle state machine
  canonical parameter/control state
  accepted inactive I/O configuration
  state generation/publication
  host-facing capability/notification coordination
  optional task/snapshot generation ownership

Processor
  exclusive mutable realtime DSP/runtime history while active
  activation-time resources/scratch
  process-local event/parameter cursors

MainThread (optional product-owned)
  editor/product non-RT orchestration only

Shared (optional product-defined projection)
  deliberately synchronized observations or immutable handles

Editor
  main-thread UI capability
```

Names are conceptual. The key contract is authority, not a particular trait spelling.

A simple effect should require neither custom `MainThread` nor `Shared` state.

## Component definition

`Component` describes the product and creates runtime parts. It is not the live mutable plugin instance.

It provides/points to stable product identity, parameter/state schema, ports/I/O policy, event capabilities, and processor/editor factories.

Compatibility metadata should normally come from Chassis/Cargo metadata + the frozen identity manifest rather than duplicated associated constants.

## Instance runtime authority

A framework-owned per-instance runtime coordinates state that must remain coherent across host/control/process domains.

It owns at least:

- current lifecycle phase/generation;
- canonical base parameter values;
- accepted inactive I/O configuration;
- persistent-state publication/replacement generation;
- host bridge capabilities and legal notification scheduling;
- ownership of any framework communication resources associated with the instance.

Do not create independent mutable copies of these guarantees in `MainThread`, editor bindings, `Shared`, and `Processor`.

The concrete storage/synchronization strategy must fit the access pattern. The runtime concept does **not** imply one giant mutex/object shared across threads.

## Processor ownership

While active, `Processor` is exclusively owned by the processing domain.

It owns mutable DSP state such as filters, delay lines, detector envelopes, oscillators, lookahead buffers, and preallocated scratch. Those values are not casually observable/mutable from editor/state/control code.

Activation establishes all resources/dimensions required for realtime processing: sample rate, maximum frames, accepted I/O configuration, sample precision capability, and product-specific precomputation.

`process` receives only realtime-safe borrowed capabilities/views.

`reset` must satisfy the strictest valid host call context supported by adapters; do not assume reset is always an unrestricted non-RT operation.

## Deactivation and destruction

The framework needs one explicit lifecycle state machine rather than independent “active/editor/task” booleans that can contradict each other.

At deactivation:

- no new process calls may borrow the active Processor;
- in-flight callback ownership must have ended according to the backend contract;
- processor-owned resources can be moved/destroyed only in a domain where their destructors are legal;
- I/O/state may then be reconfigured for a future activation.

Plugin/module unload requires stronger proof once background tasks/native callbacks exist. No callback, worker, timer, or deferred reclamation may retain executable/state references past the owner that joins/cancels/fences it.

Teardown should be idempotent at adapter boundaries where hosts may produce repeated/partial cleanup sequences.

## MainThread

`MainThread` is optional **product** non-realtime state/orchestration. It is not the canonical parameter/state owner.

Possible uses:

- constructing/controlling a product editor;
- file/resource selection;
- accepting a completed non-RT preparation into framework state;
- product-specific host/control reactions.

It communicates structural DSP changes through framework publication/reconfiguration mechanisms rather than mutating `Processor`.

## Shared

`Shared` is a projection/capability, not general shared mutability.

Examples that may be appropriate:

- atomic latest-value telemetry;
- immutable prepared-data handle with a defined publication/reclamation owner;
- bounded typed queue endpoints.

Every shared primitive must specify who owns creation, mutation/publication, observation, replacement, and final destruction.

Do not place a value in `Arc<Mutex<_>>` merely because several domains want it; first identify which domain is authoritative and whether the others need commands, snapshots, or observations.

## Parameters / state generations

The `InstanceRuntime` owns canonical base parameter/persistent state.

Processor automation/effective values are derived block views. A state load is prepared and validated off-thread, then published as one accepted generation. The processor adopts the generation only at a defined safe boundary.

If a newer state/edit arrives while older preparation is in flight, generation rules determine which result may publish. Stale completion cannot overwrite newer authority.

State save while active must use the explicitly documented snapshot consistency model; it never calls arbitrary serialization on live `Processor` state.

## Control -> Processor updates

Parameter automation is carried through the process-time parameter/event path.

Non-parameter structural data (IRs, kernels, wavetables, etc.) uses typed immutable/prepared publication if/when Chassis adds that service.

A block-boundary adoption operation must have bounded realtime work and must not cause a large old object to destruct on the audio thread. Reclamation ownership is part of the design.

## Background work

Background execution is not a base-runtime prerequisite.

When added, task ownership is tied to an instance/module generation and teardown contract. Cancellation does not mean “the task has stopped”; the owner must still ensure late completion cannot access/publish into freed or replaced state.

Do not introduce a hidden process-global executor until dynamic-library unload and last-instance shutdown are designed/tested.

Standalone/application deployments may receive execution services from their outer runtime; plugin hosts may provide workers where the host contract is sufficient.

## Editor

The editor receives typed parameter/control handles and explicitly published telemetry. It never gets `&mut Processor`.

Closing/destroying an editor invalidates its callbacks/subscriptions through one owner. A host reopening an editor creates a valid new UI generation without requiring Processor recreation unless the adapter contract demands it.

GUI toolkit adapters sit above this boundary; Chassis core does not require Iced/egui/custom rendering.

## Lifecycle sketch

```text
construct immutable Component definition/schema
        ↓
create InstanceRuntime + optional product MainThread/Shared
        ↓
validate/accept inactive I/O + initial persistent state
        ↓
activate generation N
  create Processor + bounded resources
        ↓
process/reset under exclusive Processor ownership
        ↓
stop/deactivate
  callback borrows end; reclaim/destroy in legal domain
        ↓
reconfigure/state load/reactivate generation N+1
        or
close editor/tasks/callbacks and destroy instance/module safely
```

Exact trait construction order remains to be proven by the conformance runtime and Clack adapter. The owner/generation invariants should not change merely to match one backend's callback naming.

## Failure rules

- expected invalid host/state/configuration input returns a typed adapter/runtime failure before publication;
- partial initialization cleans up only resources that were successfully acquired;
- impossible validated internal state is asserted/diagnosed as a framework bug;
- no panic unwinds through FFI;
- process-time adapter failure has a format-specific safe containment path rather than continuing with invalid Rust references/state.

## Design references

Clack's shared/main/audio domains and nice-plug's exclusive processing ownership are useful evidence that Rust can encode plugin thread domains. Chassis does not copy either runtime model wholesale; its higher-level invariant is one authoritative instance runtime plus an exclusively mutable realtime Processor and narrow projections/capabilities around them.
