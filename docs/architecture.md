# Architecture

## Purpose

Chassis is a convention-first Rust framework for professional realtime audio components.

Its first production surface is audio plugins, beginning with effects. The core model is intentionally usable outside a DAW so the same product processor/state model can later run as a plugin, standalone application, or embedded component. Optional hosting/device/graph layers may eventually support larger audio applications, but application-specific product semantics remain outside Chassis.

The scope rule is not "minimal at all costs." Chassis should own behavior that is common enough across effects and instruments that authors should not repeatedly implement it. Product-specific DSP, visual identity, content models, timelines/projects, and domain workflows remain product responsibilities.

## Design principles

### Convention over configuration

The normal professional-plugin case should require little framework plumbing, with explicit escape hatches for products that need different behavior.

Initial conventions include:

- audio effects default to stereo main input/output;
- an optional stereo sidechain is available by convention;
- parameters have typed definitions, stable identities, host metadata, automation, gestures, and state integration;
- persistent state is versioned and migration-aware;
- editor/host attachment, validation, and packaging have framework-supported paths;
- realtime communication uses framework primitives with explicit ownership and overflow semantics.

A convention must not become an invisible realtime cost. An unused sidechain, GUI, telemetry stream, smoother, or worker facility should not add process-time work merely because Chassis supports it.

### One owner for each mutable guarantee

The framework must distinguish authority from projections. Avoid two independently mutable representations of the same state.

The intended conceptual ownership is:

```text
Component definition
  immutable product schema, capabilities, identity, factories

Instance runtime (framework-owned)
  lifecycle state
  canonical parameter/control state
  host bridge/capabilities
  state publication/loading coordination
  task generations/cancellation when task support is present

Processor
  exclusive mutable realtime DSP state while active
  process-local automation/modulation cursors
  activation-time resources

MainThread
  optional product-owned non-realtime state/editor orchestration

Shared
  explicitly thread-safe projections/handles only

Editor
  main-thread UI capability; never owns Processor
```

The exact Rust traits are not frozen. The ownership relationships are.

`Shared` is not a second authority for product state. It exposes deliberately synchronized observations or immutable snapshots whose owner and reclamation path are explicit.

### Structural realtime safety

Realtime restrictions apply to the audio callback and other explicitly deterministic hot paths, not indiscriminately to the whole framework.

On the audio path:

- no heap allocation after activation;
- no blocking I/O, filesystem, network, or contended/unbounded locks;
- queues, scratch memory, retries, and work have explicit bounds derived from activation/product requirements;
- destruction of replaced large objects is deferred off the audio thread;
- host/FFI callbacks available to processing are represented as narrow realtime-safe capabilities.

On main/control/editor/tooling paths, use the simplest correct ownership and synchronization model. Do not introduce lock-free structures, static allocation, custom allocators, or zero-copy complexity without a realtime or measured performance reason.

### Validate boundaries; assert internal invariants

Host data, persisted bytes, format metadata, and FFI inputs are untrusted boundaries. Validate them and return/report typed failures without partially publishing invalid state.

Impossible states after successful validation are framework bugs. Assert non-obvious invariants where doing so turns silent corruption or undefined behavior into an observable failure. Never unwind through foreign ABI boundaries; adapters must define their containment behavior.

### Backend independence

Product-facing core APIs do not expose CLAP, VST3, Audio Unit, Clack, clap-wrapper, GUI toolkit, or operating-system types.

Initial export direction:

```text
Chassis component API
        ↓
  chassis-clap
        ↓
      Clack
        ↓
       CLAP
        ↓
 clap-wrapper where it remains correct
   ├── VST3
   ├── AUv2/AUv3
   └── simple standalone bootstrap
```

This is an implementation strategy, not a semantic dependency. Native adapters may replace wrappers when capability, correctness, lifecycle, or maintenance evidence justifies owning them.

Dependencies on realtime-critical paths require explicit review for allocation, locks, unsafe invariants, maintenance quality, and measurable overhead. Permissive licensing alone is not sufficient.

## Core component model

The common component contracts include:

- lifecycle, activation, reset, and teardown;
- audio/event ports and whole-component I/O negotiation;
- process buffers and realtime/offline context;
- typed parameters, automation, modulation, and gestures;
- note/MIDI/event streams without assuming every component is an effect;
- host transport information;
- versioned persistent state and migrations;
- latency/tail/bypass metadata where broadly meaningful;
- optional editor integration;
- narrow realtime-safe telemetry/control primitives.

Background execution, richer diagnostics, preset storage, analyzer transport, smoothing utilities, and similar common conveniences belong in Chassis when their lifecycle and semantics are proven, but they do not all need to be part of the first core implementation.

Effects and instruments share the same lifecycle/process model. An instrument may have no audio input and multiple outputs. A note/MIDI processor may have event input/output and no audio. An effect may expose sidechain or auxiliary buses.

## Audio ports and channel layouts

Stereo is the first production target, not an architectural assumption.

The default effect convention is:

```text
main input:     stereo, required
main output:    stereo, required
sidechain:      stereo, optional/inactive
```

Stable author-facing port identity should be human-readable and independent of backend numeric IDs or runtime dense indices. Adapters/runtime setup may derive compact indices after schema validation; those derived indices are not persistent product identity.

Whole I/O configurations are validated and committed atomically while inactive. The representation must leave room for mono, discrete channels, labeled surround, ambisonics, immersive channel beds, multiple buses, and instrument multi-output routing without pretending Dolby Atmos object/metadata workflows are merely another speaker enum.

## Process buffers

The framework preserves host buffer relationships rather than imposing unconditional copies. Rust references may only be constructed after adapters prove the relevant aliasing invariants.

Exact in-place aliasing, disjoint input/output buffers, input-only, and output-only cases are distinct. Unexpected partial overlap or illegal aliasing is an adapter-boundary failure; never manufacture overlapping `&`/`&mut` references because a host supplied suspicious pointers.

Convenience in-place processing may copy a bounded block when the host supplied separate buffers. Controlled copies are acceptable when they simplify product ownership and are not a material measured cost; zero-copy is not a goal by itself.

## Parameters and persistent state

Parameter identity and metadata are immutable schema. Rust field names and UI labels are not compatibility identity.

Chassis distinguishes:

1. canonical/base parameter state owned by the framework instance runtime;
2. process-time automated trajectories derived from host events for a block;
3. effective/modulated values consumed by DSP.

The process view is derived and cannot independently become a second persistent authority. The current core validates borrowed block automation against the canonical schema and exposes lazy set/linear trajectories. Realtime endpoint publication is conditional on the generation observed before processing; a newer control edit or state load wins. Active state save serializes one coherent completed generation. See [parameters and state](design/parameters-state.md) for the publication contract.

State loading is transactional from the product's perspective: decode, validate, and migrate into temporary non-live state first, then publish the accepted state through one defined runtime boundary. Partial state loads never mutate the active product.

Persistent state uses a format-independent semantic model with explicit schema versions. The current small Chassis-owned binary envelope is a prototype, not yet a compatibility promise.

## GUI boundary

Chassis owns editor lifecycle and host integration, not visual design.

Common responsibilities can include parent/native-window attachment, sizing/scaling negotiation, parameter binding/gestures, main-thread scheduling, and realtime-to-GUI telemetry. GUI toolkit adapters remain optional; headless components do not depend on them.

## Standalone and embedded deployment

Standalone is a planned first-class deployment mode, particularly useful for instruments, analyzers, and processors that make sense outside a DAW. It should run the same product processor/state implementation rather than a second DSP path.

Embedded deployment allows a component to run inside another Rust audio application without pretending to be a plugin.

## Larger audio applications

Future `chassis-host`, `chassis-device`, and `chassis-graph` layers may provide reusable plugin hosting, audio/MIDI device integration, and realtime graph/scheduling infrastructure when a real application requires them.

They are not prerequisites for plugin v0.1 and must not pull DAW/application concepts into the component core. Chassis does not own timelines, projects, arrangements, media libraries, mixer UX, mastering revisions/QC, or delivery workflows.

## Rust and dependency policy

Development follows the repository's `stable` Rust toolchain and Edition 2024. During pre-alpha there is no promised MSRV beyond what Edition 2024/tooling require; do not pin a compiler merely to chase the latest release, and do not raise a future declared MSRV without an explicit reason.

Format-independent crates deny unsafe code. FFI adapters isolate necessary unsafe code, use `unsafe extern` where required by Edition 2024, require a `// SAFETY:` explanation for every unsafe block, and keep `unsafe_op_in_unsafe_fn` denied.

Public API is deliberate: use domain newtypes/enums, avoid accidental re-exports, and keep implementation-only dense indices/backend IDs out of the stable author-facing identity model.

Core dependencies stay small, audited, and compatible with both AGPL distribution and the intended commercial license. Realtime-path dependencies receive a stricter mechanical-sympathy audit than non-realtime tooling dependencies.
