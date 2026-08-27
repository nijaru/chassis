# Runtime Ownership Model

Status: first explicit lifecycle slice implemented; public API still pre-alpha and not frozen.

## Goal

Make ownership, mutation authority, replacement, and teardown difficult to get wrong while keeping ordinary plugin authors focused on product DSP.

The framework may have several thread/capability domains, but each mutable guarantee has one owner.

## Runtime roles

```text
Component definition
  immutable schema/metadata/capabilities/factories

InstanceRuntime (eventual framework-owned authority)
  lifecycle state machine
  canonical parameter/control state
  accepted inactive I/O configuration
  state generation/publication
  host-facing capability/notification coordination

Processor
  exclusive mutable realtime DSP/runtime history while active
  activation-time resources/scratch

Process<S>
  sample-representation-specific process capability over borrowed ProcessBlock<S>

MainThread / Shared / Editor
  optional non-RT product capabilities/projections added only when required
```

The names are less important than the authority split. A simple effect should require neither custom `MainThread` nor `Shared` state.

## Implemented explicit slice

`chassis-core::runtime` now provides the first executable manual API:

- `Component` is the immutable product definition/factory;
- `Component::audio_ports()` defaults to the standard effect schema and can be overridden;
- `Component::activate()` creates one realtime `Processor` after structural I/O validation;
- `Processor` owns reset/lifecycle semantics;
- `Process<S>` is a separate capability for processing one sample representation;
- `Activated<P>` holds the immutable activation configuration, canonical base `ParameterStore`, and exclusively owned processor;
- `Activated::process()` validates context, activation event bounds, and callback-varying frame dimensions, then validates borrowed parameter events against the active schema before calling product DSP with a borrowed `ProcessBlock`;
- `Activated::deactivate(self)` consumes the active shell so processor destruction happens only after the caller has ended process/reset borrows.

This split is deliberate. `Processor` is not parameterized by `f32`: a future processor can implement both `Process<f32>` and `Process<f64>` without duplicating lifecycle/DSP ownership.

The format-independent `Processor` trait also does not globally require `Send`. Same-thread embedded deployment is a valid Chassis use case, and safe Rust already prevents a non-`Send` `Activated<P>` from being moved through ordinary thread-transfer APIs. A deployment boundary whose lifecycle actually transfers processor ownership across threads—CLAP is expected to be one—must require `P: Send` there. Exclusive processing does not imply `Sync`.

The current conformance processor is deliberately `Send` and has a compile-time assertion for that property so it remains suitable for the first plugin-adapter proof without making plugin threading a universal core restriction.

The current `Activated` type is **not** the final `InstanceRuntime`. It intentionally does not invent parameter/state synchronization, background execution, editor generations, or host callbacks before those contracts are proven.

## Component definition

`Component` is not the live mutable plugin instance. It describes/provides stable product schema and creates runtime parts.

The current explicit slice consumes audio-port schema, typed parameter schema, process context, and processor activation. Product identity, state publication, host event translation, editor factories, and richer I/O policy remain separate design contracts to add as executable requirements reach this layer.

Compatibility metadata should normally come from Chassis/Cargo metadata + the frozen identity manifest rather than duplicated associated constants.

## Activation and I/O

Framework activation currently performs structural `AudioIoConfiguration` validation before product activation. That catches duplicate/unknown/missing stable ports and prevents malformed configuration from reaching product construction.

The default `Component::audio_ports()` uses the standard effect descriptors, but **whole-layout policy is not frozen yet**. In particular, the current structural validator does not by itself mean that every layout representable by `ChannelLayout` is valid for every effect. A dedicated semantic I/O-policy layer remains an API-freeze gate.

Activation establishes the resource bounds needed by realtime processing:

- sample rate;
- optional guaranteed positive minimum block size;
- non-zero maximum block size;
- accepted audio I/O configuration;
- product-owned precomputation/resources.

Per-call scheduling mode is not activation state because VST3 can change realtime/prefetch mode without reactivation.

## Processor ownership

While active, `Processor` is exclusively owned by the processing domain. It owns mutable DSP history such as filters, delay lines, envelopes, oscillators, lookahead buffers, and preallocated scratch.

Editor/state/control code never receives unrestricted mutable access to it.

Thread-transfer capability is deployment-specific: an adapter that moves active processor ownership from its activation/control domain to another processing thread must require `Send` and prove its host lifecycle makes that transfer exclusive. Chassis core should not require `Send` merely for same-thread embedded or specialized runtimes that do not cross that boundary.

`reset` preserves persistent/control state while resetting transient DSP history. Stateless processors may use the current default no-op implementation.

`process` receives only borrowed realtime-safe audio and process-context views through `ProcessBlock<S>`.

## Process block ownership

`ProcessBlock` borrows safe `ChannelBuffer<S>` views and carries actual frame count plus `ProcessContext`.

Its constructor is framework-private. Product code can inspect/process the borrowed block but cannot manufacture a fake framework block directly.

`ProcessContext` carries the per-call mode, optional block-start transport snapshot,
and a borrowed `ParameterEvents` view. Event slices are bounded and sample-sorted;
`Activated::process()` validates context/bounds and dimensions before checking
event values against the active schema. A float cursor evaluates linear
trajectories lazily without per-sample materialization.

Per-call construction validates only facts that can change per callback without introducing hidden unbounded work:

- callback frame count against activation min/max guarantees;
- event context was validated for the same callback frame count;
- every supplied safe channel slice has exactly that callback length.

Stable endpoint/port translation should be resolved by runtime/adapter setup, not by rescanning all semantic endpoints with allocation or quadratic work in every audio callback.

## Instance runtime authority still to implement

The eventual framework-owned `InstanceRuntime` remains the authority for state that must stay coherent across host/control/process domains:

- lifecycle phase/generation;
- canonical base parameter values;
- accepted inactive I/O configuration;
- persistent-state publication/replacement generation;
- host bridge capabilities and legal notification scheduling;
- ownership of framework communication resources associated with the instance.

Do not create independent mutable copies of those guarantees in `MainThread`, editor bindings, `Shared`, and `Processor`.

The concrete storage/synchronization strategy must fit the access pattern. The runtime concept does **not** imply one giant mutex/object shared across threads.

## Deactivation and destruction

At deactivation:

- no new process/reset borrow may exist;
- in-flight callback ownership must have ended according to the backend contract;
- processor-owned resources may be destroyed only in a domain where their destructors are legal;
- I/O/state can then be reconfigured for a future activation.

The current `Activated::deactivate(self)` is the smallest executable ownership proof. Plugin/module unload becomes stricter once background tasks, native callbacks, timers, editor resources, or deferred reclamation exist.

Teardown should be idempotent at adapter boundaries where hosts may produce repeated/partial cleanup sequences.

## Parameters / state generations

The current runtime shell owns validated canonical base parameter values and
validates each block's derived automation view. It does not yet provide the final
cross-domain `InstanceRuntime` synchronization contract: host gesture delivery,
automation-to-base publication, coherent active state snapshots, or generation-
checked state replacement.

When added, framework instance state remains authoritative; process
automation/effective values remain derived block views. A state load is prepared
and validated off-thread, then published as one accepted generation. Stale
completion cannot overwrite newer authority.

State save while active must use an explicitly documented snapshot consistency model and never serialize arbitrary live `Processor` fields.

## Background work / shared state / editor

These remain outside the base runtime until a real requirement proves them.

When added:

- `Shared` is a projection/capability, not general shared mutability;
- background tasks are tied to instance/module generations and teardown;
- cancellation does not substitute for joining/fencing stale completion;
- large replaced objects are reclaimed off the audio thread;
- no hidden process-global executor exists until dylib unload/shutdown lifetime is designed;
- editor callbacks/subscriptions are invalidated by one owner and never obtain `&mut Processor`.

## Failure rules

- malformed structural I/O fails before product activation;
- product activation errors are distinguished from framework validation errors;
- callback frame violations fail before product DSP runs;
- partial initialization cleans up only resources that were successfully acquired;
- impossible validated internal state is a framework bug;
- no panic may unwind through a future FFI boundary.

## Validation status

`crates/chassis-core/tests/conformance.rs` exercises the explicit API externally with deterministic processing, separate buffers, exact in-place buffers, reset/deactivation ownership, malformed activation, callback-size rejection, zero-frame behavior where no positive minimum is promised, sample-accurate parameter set/linear trajectories, invalid event rejection before DSP, transport context, and a compile-time `Send` assertion for the future plugin path.

This is still a pre-alpha conformance slice. It does not yet prove state
publication/generations, host event translation, adapters, FFI, host behavior,
allocation instrumentation, or production readiness.
