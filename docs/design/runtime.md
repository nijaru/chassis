# Runtime Ownership Model

Status: design direction; traits are not frozen.

## Goal

Make the realtime ownership rules difficult to violate accidentally while keeping ordinary plugins simple to implement.

The product-facing API should separate objects by the thread/capability domain in which they are valid instead of exposing one giant plugin object with unrelated audio-thread, UI-thread, state, and lifecycle methods.

## Proposed roles

```text
Component
├── static/declarative product definition
├── parameter/state schema
├── supported I/O configurations
└── factory for runtime parts

Shared
└── explicitly thread-safe state that may be observed across domains

MainThread
├── non-realtime product control state when needed
├── editor creation/control
├── custom non-realtime state hooks
└── background-work orchestration

Processor
├── exclusive mutable realtime DSP state
├── activation-time configuration
├── reset
└── process
```

The names describe ownership rather than a particular plugin format. `MainThread` is preferred over `Controller` because `Controller` has format-specific meanings (notably VST3) and can imply more authority than the object should have.

A simple effect should be able to use `()` or framework defaults for `Shared` and `MainThread` and implement mostly its processor and parameter declaration.

## Component definition

`Component` is the product definition and factory, not the realtime object.

It should eventually describe or provide:

- stable product identity and metadata;
- parameter definitions;
- audio/event ports and supported I/O configurations;
- state schema/version;
- optional editor capability;
- factories for the runtime parts.

Avoid requiring product metadata to be compile-time constants when doing so would make composition unnecessarily difficult. Declarative/conventional APIs can still evaluate once at instance creation.

## Processor ownership

While active, the `Processor` is exclusively owned by the processing domain. The editor, state serializer, background workers, and host main-thread callbacks do not receive `&mut Processor` or an unrestricted shared reference to it.

Activation is the point where sample-rate, block-size limits, active I/O configuration, and other allocation-dependent resources are established. Expensive allocation/precomputation belongs before realtime processing starts.

`reset` is realtime-safe because hosts may require it in realtime-sensitive contexts.

`process` receives only capabilities valid for realtime use.

## Main-thread ownership

`MainThread` is optional product state for work that genuinely requires a non-realtime owner. Many simple effects should not need a custom implementation.

Possible responsibilities include:

- editor construction;
- file/asset selection orchestration;
- custom state that is not a parameter;
- dispatching bounded background work;
- reacting to host-level changes that cannot occur on the audio thread.

It must communicate DSP changes through explicit framework mechanisms rather than mutating the live processor directly.

## Shared state

`Shared` is not an escape hatch for arbitrary shared mutability. Types placed here must be intentionally thread-safe and suitable for the access patterns they expose.

Typical uses:

- atomics for cheap observable values;
- immutable `Arc` data prepared off-thread and atomically swapped through a bounded/specified mechanism;
- bounded lock-free telemetry/control channels.

Ordinary mutable DSP state does not belong here.

## Parameters and automation

Parameter definitions/state are framework-managed common infrastructure, not arbitrary fields on `Processor` that the GUI also mutates.

The intended processing model is:

1. the framework maintains canonical parameter/control state outside product DSP;
2. host automation/modulation arrives as timestamped process events;
3. the processor receives values/events through a realtime process context;
4. optional Chassis smoothing helpers can turn event/value streams into conventional trajectories;
5. products may opt out and implement custom smoothing/interpretation where necessary.

This keeps sample-accurate host delivery explicit and avoids requiring the DSP to poll cross-thread atomics for every automated value.

The final API may provide ergonomic parameter handles for reading the current base value and for UI bindings, but those handles must not erase the difference between a control-thread value and sample-accurate process-time changes.

## State

State serialization must not require calling an arbitrary `save_state(&self)` method on the live realtime processor.

The framework should treat persistent state as a control/state-domain concern:

- parameters and framework-managed persistent fields have canonical non-RT representations;
- custom product state is serialized through a non-realtime state hook;
- loading parses and validates state off the audio thread;
- changes affecting realtime processing are transferred through a safe boundary, such as reactivation or an explicit block-boundary command/snapshot mechanism;
- migrations occur before the new state becomes active.

DSP history such as filter delay lines, compressor envelopes, oscillator phase, or limiter lookahead buffers is runtime state and is not persisted by convention.

## Background work

Chassis should eventually provide a conventional bounded task path so products do not invent thread pools and unsafe callback handoffs independently.

Requirements:

- dispatch from realtime is bounded and non-blocking;
- queue-full behavior is explicit;
- background completion cannot mutate `Processor` directly;
- completion can publish immutable data, enqueue a bounded control message, or schedule main-thread work;
- offline rendering semantics are deterministic and do not accidentally depend on an asynchronous task that may not complete in time.

## Editor

The editor is a main-thread capability and receives framework parameter/UI handles plus explicitly exposed telemetry. It does not receive `Processor`.

GUI toolkit adapters sit above this contract. The runtime model must work with Iced, egui, a custom renderer, or no editor.

## Lifecycle sketch

```text
create Component instance
        ↓
construct framework parameter/state storage
construct Shared
construct optional MainThread
        ↓
select/accept I/O configuration
load/validate state if required
        ↓
activate
  create/move Processor with immutable activation config
        ↓
start processing
  Processor is audio-domain exclusive
        ↓
process / reset ...
        ↓
stop processing
        ↓
deactivate
  Processor returns to non-active ownership or is destroyed
        ↓
reconfigure / load state / reactivate / destroy
```

Exact construction order remains open until the first Clack adapter and conformance component test it against real host lifecycles.

## Design references, not APIs to copy

Clack has separate shared, main-thread, and audio-processor handler domains and uses Rust lifetimes/ownership to reflect the CLAP threading model. Nice-plug instead exposes a stateful plugin object exclusively to the audio thread during processing and constructs editor/task facilities separately. Both validate the general principle that processing ownership can be kept structurally separate from GUI/non-RT facilities.

Chassis should choose its own higher-level conventions based on common product requirements and validation rather than reproducing either API.
