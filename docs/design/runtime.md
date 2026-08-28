# Runtime Ownership Model

Status: first explicit lifecycle slice implemented; adapter-local cross-domain parameter publication is now hardened, but the persistent instance authority remains the next core runtime slice. Public API is still pre-alpha and not frozen.

## Goal

Make ownership, mutation authority, replacement, and teardown difficult to get wrong while keeping ordinary plugin authors focused on product DSP.

The framework may have several thread/capability domains, but each mutable guarantee has one owner.

## Runtime roles

```text
Component definition
  immutable schema/metadata/capabilities/factories

InstanceRuntime (next framework-owned authority)
  lifecycle state machine
  canonical parameter/control state
  accepted inactive I/O configuration
  state generation/publication
  host-facing capability/notification coordination

Active runtime / Processor
  activation-local realtime projection of accepted control state
  exclusive mutable realtime DSP/runtime history while active
  activation-time resources/scratch

Process<S>
  sample-representation-specific process capability over borrowed ProcessBlock<S>

MainThread / Shared / Editor
  optional non-RT product capabilities/projections added only when required
```

The names are less important than the authority split. A simple effect should require neither custom `MainThread` nor `Shared` product state.

## Implemented explicit slice

`chassis-core::runtime` now provides the first executable manual API:

- `Component` is the immutable product definition/factory;
- `Component::audio_ports()` defaults to the standard effect schema and can be overridden;
- `Component::activate()` creates one realtime `Processor` after structural I/O validation;
- `Processor` owns reset/lifecycle semantics;
- `Process<S>` is a separate capability for processing one sample representation;
- `Activated<P>` holds immutable activation configuration, an activation-local validated `ParameterStore`, and the exclusively owned processor;
- `Activated::process()` validates context, activation event bounds, and callback-varying frame dimensions, then validates borrowed parameter events against the active schema before calling product DSP with a borrowed `ProcessBlock`;
- `Activated::deactivate(self)` consumes the active shell so processor destruction happens only after the caller has ended process/reset borrows.

This split is deliberate. `Processor` is not parameterized by `f32`: a future processor can implement both `Process<f32>` and `Process<f64>` without duplicating lifecycle/DSP ownership.

The format-independent `Processor` trait also does not globally require `Send`. Same-thread embedded deployment is a valid Chassis use case, and safe Rust already prevents a non-`Send` `Activated<P>` from being moved through ordinary thread-transfer APIs. A deployment boundary whose lifecycle actually transfers processor ownership across threads—CLAP is expected to be one—must require `P: Send` there. Exclusive processing does not imply `Sync`.

The current conformance processor is deliberately `Send` and has a compile-time assertion for that property so it remains suitable for the first plugin-adapter proof without making plugin threading a universal core restriction.

The current `Activated` type is **not** the durable instance authority. Its parameter store is created at activation and destroyed with the active shell, so it must be treated as a validated realtime/control projection for the current proof rather than the canonical state that survives deactivate/reactivate. The CLAP scalar slice currently keeps a provisional synchronized projection outside core to preserve host-visible values across activations. The next core slice replaces that accidental ownership split with `InstanceRuntime`.

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

The fixed-stereo CLAP proof can retain the current `'static` default configuration. Before general sidechain/multibus negotiation lands, the accepted dynamic configuration must become runtime-owned rather than requiring `Activated<'a, P>` to borrow adapter-owned configuration storage for its entire lifetime. Setup-time ownership/allocation is acceptable; callback-time allocation is not.

Processor activation must also eventually observe the accepted current parameter/state generation when activation-time preparation depends on control values. Recreating a processor from parameter defaults after a state load is not an acceptable production contract.

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

The current event prototype carries stable string keys because it proves semantics directly. Before the parameter/process API freezes or high-count automation is promoted, schema setup should resolve stable `ParameterKey` values to dense runtime `ParameterIndex` values. Persistent state and authoring continue to use stable keys; realtime event matching/cursors use the dense index and do not repeatedly compare strings or scan schema mappings.

## Instance runtime authority — next core slice

`InstanceRuntime` becomes the durable owner for state that must stay coherent across host/control/process domains:

- lifecycle phase/generation;
- canonical base parameter values that survive deactivate/reactivate;
- accepted inactive I/O configuration;
- persistent-state publication/replacement generation;
- host bridge capabilities and legal notification scheduling;
- ownership of framework communication resources associated with the instance.

The active processor receives a derived realtime projection of the accepted generation. That projection is not independently authoritative. Realtime automation may publish its resulting base endpoint back toward the instance authority, but that publication is conditional on the generation from which it was derived. If a newer control edit or state replacement has already published, stale realtime completion is discarded rather than overwriting newer state.

A full state replacement is prepared and validated off the audio thread, then published as one generation. After migration to the current product schema, a full state document must materialize every framework-managed parameter/current persistent field. Missing fields require an explicit migration/default rule or cause rejection; partial parameter patches are a distinct operation and are not implicit plugin-state semantics.

The current CLAP scalar projection implements this precedence locally: serialized writers prevent mixed snapshots, coherent state save waits for a completed generation off the audio thread, and stale realtime endpoint publication cannot overwrite a newer control/state generation. That mechanism remains adapter-local evidence, not shared core infrastructure. Before a subtle atomic publication primitive is generalized into `chassis-core`, model/property-test its ordering and progress behavior (for example with Loom).

Do not create independent mutable semantic authorities in `MainThread`, editor bindings, `Shared`, and `Processor`. The concrete storage/synchronization strategy must fit each deployment; `InstanceRuntime` does **not** imply one giant mutex/object shared across threads.

## Deactivation and destruction

At deactivation:

- no new process/reset borrow may exist;
- in-flight callback ownership must have ended according to the backend contract;
- processor-owned resources may be destroyed only in a domain where their destructors are legal;
- canonical parameter/state authority remains alive at the instance level;
- I/O can then be reconfigured for a future activation.

The current `Activated::deactivate(self)` is the smallest executable processor-ownership proof. Plugin/module unload becomes stricter once background tasks, native callbacks, timers, editor resources, or deferred reclamation exist.

Teardown should be idempotent at adapter boundaries where hosts may produce repeated/partial cleanup sequences.

## Parameters / state generations

Framework instance state is authoritative; process automation/effective values are derived block views.

Required generation rules for the next runtime slice:

1. a control edit or accepted state replacement publishes a new canonical generation;
2. an active process block observes a coherent base generation at its defined synchronization boundary;
3. sample-accurate automation is evaluated from that base plus host events;
4. the resulting automation endpoint may update canonical base state only if it is still derived from the current generation;
5. stale asynchronous/realtime completion never overwrites a newer generation;
6. a state save observes one completed canonical generation and never serializes arbitrary live `Processor` fields.

No audio-thread operation may wait for a control writer. If publication is temporarily unavailable or loses a generation race, the realtime path continues and a newer control/state generation wins.

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

The current CLAP scalar bridge additionally has adapter-level regression coverage for generation-checked realtime publication and complete parameter-state replacement. This is still pre-alpha evidence. It does not replace model testing for a future shared atomic primitive, native CLAP requalification of the updated artifact, broader host behavior, allocation instrumentation, or production readiness.
