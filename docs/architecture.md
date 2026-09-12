# Architecture

## Purpose

Chassis is a Rust framework for building audio software.

Plugins are expected to be the most common authoring/deployment path and are the first qualified surface, but the core model is not a plugin abstraction. The same component/runtime model should support effects, instruments, event processors, standalone applications, plugin hosts, processing graphs, audio engines/DAWs, offline processing, and embedded use.

Chassis owns broadly reusable audio infrastructure. Applications own their product, document, workflow, and visual semantics. A DAW should be able to use Chassis for its processing engine, devices, graph/scheduling, hosted plugins, transport/audio-engine primitives, media/offline integration, and rendering without Chassis defining tracks, clips, arrangements, project UX, editing workflows, or other DAW-specific document models.

The scope rule is not "minimal at all costs" and not "own every dependency." Chassis should own semantics, lifecycle, safety boundaries, and reusable audio infrastructure that would otherwise be repeatedly reimplemented across audio software. It should integrate strong existing Rust crates for mature low-level facilities where appropriate instead of reimplementing them merely for ownership.

## Architectural layers

The intended direction is layered rather than plugin-centric:

```text
Applications / products
  plugins, instruments, standalone apps, hosts, DAWs, analyzers, utilities
                     |
                     v
Application audio infrastructure
  graph / scheduling / devices / plugin hosting / media / offline rendering
                     |
                     v
Deployment and UI integration
  CLAP / VST3 / AU / standalone / editor-toolkit adapters
                     |
                     v
Format-independent audio runtime
  component / processor / ports / events / parameters / state / RT communication
                     |
                     v
Common DSP utilities and reviewed low-level dependencies
```

These are logical layers. They become separate crates only when dependency, safety, optionality, testing, or release boundaries justify a crate split.

## Design principles

### General semantics, convenient common cases

The common plugin/effect case should require little framework plumbing, but common-case convenience belongs in helpers/facades rather than by making core semantics inaccurate for instruments, event-only processors, graph nodes, standalone applications, or offline engines.

Useful conventions can include:

- helpers for conventional mono/stereo effects and optional sidechains;
- typed parameters with stable identities, metadata, automation, gestures, and state integration;
- versioned/migration-aware persistent state;
- editor/host attachment, validation, and packaging paths;
- realtime communication primitives with explicit ownership and overflow semantics.

A convention must not become an invisible realtime cost. An unused sidechain, GUI, telemetry stream, smoother, worker facility, device layer, graph layer, or plugin adapter should not add process-time work merely because Chassis supports it.

### One owner for each mutable guarantee

The framework must distinguish authority from projections. Avoid two independently mutable representations of the same state.

The intended conceptual ownership is:

```text
Component definition
  immutable semantic schema, capabilities, factories

Instance runtime (framework-owned)
  lifecycle state
  canonical parameter/control state
  canonical custom semantic state
  accepted activation configuration
  state publication/loading coordination
  task generations/cancellation when task support is present

Processor
  exclusive mutable realtime DSP state while active
  process-local automation/modulation cursors
  activation-time resources

Deployment/application owners
  host/device/graph/editor orchestration and projections

Shared handles
  explicitly synchronized observations or immutable publications only
```

The exact Rust traits are not frozen. The ownership relationships are.

A plugin `MainThread`, standalone application object, graph controller, editor bridge, or host bridge is not a second authority for processor state merely because it owns environment-specific integration.

### Structural realtime safety

Realtime restrictions apply to the audio callback and other explicitly deterministic hot paths, not indiscriminately to the whole framework.

On a realtime path:

- no heap allocation or deallocation after the relevant preparation/activation boundary;
- no blocking I/O, filesystem, network, or contended/unbounded locks;
- queues, scratch memory, retries, event counts, graph work, and other work have explicit bounds;
- destruction of replaced large objects is deferred off the realtime thread;
- callbacks into hosts/devices/graphs are represented as narrow realtime-safe capabilities;
- topology/resource changes are prepared away from the callback and published transactionally.

On control/editor/tooling/offline paths, use the simplest correct ownership and synchronization model. Do not introduce lock-free structures, static allocation, custom allocators, or zero-copy complexity without a realtime or measured performance reason.

### Realtime and offline are both first-class

Chassis is not a realtime-only framework. The process model distinguishes execution contexts rather than forcing one algorithmic mode:

- realtime: caller has a delivery deadline;
- buffered realtime/prefetch: may be scheduled ahead but retains realtime-safe behavior;
- offline: no wall-clock delivery deadline and may choose a more expensive deterministic path.

Sequential block processing does not need to represent every future offline workflow. Region/asset/random-access analysis or multi-pass processing should use a separate abstraction when required rather than distorting the realtime `Processor::process` contract.

### Validate boundaries; assert internal invariants

Host data, device data, persisted bytes, format metadata, file/media metadata, and FFI inputs are untrusted boundaries. Validate them and return/report typed failures without partially publishing invalid state.

Impossible states after successful validation are framework bugs. Assert non-obvious invariants where doing so turns silent corruption or undefined behavior into an observable failure. Never unwind through foreign ABI boundaries; adapters must define containment behavior.

### Backend independence

Product-facing core APIs do not expose CLAP, VST3, Audio Unit, Clack, clap-wrapper, GUI toolkit, CPAL/device-backend, codec, or operating-system types.

Current plugin export direction is:

```text
Chassis component API
        |
        v
  chassis-clap
        |
        v
      Clack
        |
        v
       CLAP
        |
        v
 clap-wrapper where it remains correct
   |- VST3
   |- AUv2/AUv3
   `- simple standalone bootstrap where useful
```

This is an implementation strategy, not a semantic dependency. Native adapters may replace wrappers when capability, correctness, lifecycle, or maintenance evidence justifies owning them.

The same rule applies to future device, media, and DSP integrations: use mature dependencies when they fit, but keep their types and quirks behind Chassis-owned semantic boundaries where long-term framework behavior depends on them.

## Core component model

The common component/runtime contracts include:

- lifecycle, activation, reset, and teardown;
- audio/event ports and whole-component I/O negotiation;
- process buffers and realtime/offline context;
- typed parameters, automation, modulation, and gestures;
- note/MIDI/event streams without assuming every component is an effect;
- transport information exposed to processors;
- versioned persistent state and migrations;
- latency/tail/bypass metadata where broadly meaningful;
- optional editor integration boundaries;
- narrow realtime-safe telemetry/control primitives;
- background-work/result-publication semantics where repeated audio workloads require them.

Effects, instruments, graph nodes, event processors, embedded processors, and offline renderers should use the same component/runtime semantics where those semantics apply. An instrument may have no audio input and multiple outputs. A note/MIDI processor may have event input/output and no audio. An effect may expose sidechain or auxiliary buses. A graph node is not a special processor type merely because another Chassis layer schedules it.

Core should not default semantically to "stereo effect" merely because stereo effects are the most common plugin example. Conventional effect helpers can provide that authoring convenience above the neutral core contract.

## Audio ports and channel layouts

Stereo is the first qualified target, not an architectural assumption.

Stable author-facing port identity is independent of backend numeric IDs or runtime dense indices. Adapters/runtime setup may derive compact indices after schema validation; those derived indices are not persistent identity.

Whole I/O configurations are validated and committed atomically while inactive. The representation must leave room for:

- zero-audio event processors;
- mono/stereo effects and instruments;
- multiple input/output buses;
- sidechains and auxiliary buses;
- discrete channels;
- labeled surround layouts;
- ambisonics/immersive channel beds.

Dolby Atmos object/metadata/renderer workflows are not merely another speaker layout and should remain a distinct future capability.

## Process buffers

The framework preserves actual buffer relationships rather than imposing unconditional copies. Rust references may only be constructed after an adapter/application boundary proves the relevant aliasing invariants.

Exact in-place aliasing, disjoint input/output buffers, input-only, and output-only cases are distinct. Unexpected partial overlap or illegal aliasing is a boundary failure; never manufacture overlapping `&`/`&mut` references because an environment supplied suspicious pointers.

Convenience in-place processing may copy a bounded block when separate buffers were supplied. Controlled copies are acceptable when they simplify ownership and are not a material measured cost; zero-copy is not a goal by itself.

## Parameters and persistent state

Parameter identity and metadata are immutable schema. Rust field names and UI labels are not compatibility identity.

Chassis distinguishes:

1. canonical/base parameter state owned by the framework instance runtime;
2. process-time automated trajectories derived from host/application events for a block;
3. process-time modulation;
4. effective values consumed by DSP after product policy.

Automation/modulation views are derived and cannot independently become persistent authorities. Realtime endpoint publication must not overwrite a newer control edit or state load.

State loading is transactional from the application's perspective: decode, validate, and migrate into temporary non-live state first, then publish the accepted state through one defined runtime boundary. Partial state loads never mutate the live component.

Persistent state uses a format-independent semantic model with explicit schema versions. The current CHSS binary envelope is pre-alpha and not yet a compatibility promise.

## DSP utilities

Common DSP utilities are in scope when they create a coherent reusable audio foundation. Likely families include units/gain conversion, smoothing/ramps, interpolation, delay/ring buffers, metering, filters, oscillators/envelopes, FFT/STFT helpers, oversampling, resampling integration, convolution helpers, and common buffer/channel operations.

Do not infer a crate split merely from this domain list. Utilities may begin as modules or integrations and move behind optional crates/features only when dependency/release boundaries justify it.

Do not reimplement strong existing FFT, resampling, codec, or other low-level crates without a concrete correctness, semantics, maintenance, or performance reason. Chassis's value is coherent audio semantics and engineering, not ownership of every primitive.

Specialized processors such as mastering limiters, restoration models, instrument engines, and product-specific algorithms remain clients unless repeated use reveals a genuinely reusable building block.

## GUI/editor boundary

Chassis owns editor lifecycle and audio-software integration, not product visual design or a GUI toolkit.

Common responsibilities include parent/native-window attachment, sizing/scaling negotiation, parameter binding/gestures, main-thread scheduling, realtime-to-GUI telemetry, recreation/teardown, and format/host lifecycle projection.

GUI toolkit adapters remain optional. Headless processors and non-GUI applications do not depend on them.

## Devices and standalone deployment

Audio/MIDI device infrastructure and standalone deployment are planned framework scope, not separate product architectures.

The device layer should provide reusable semantics over platform/device backends for:

- enumeration and capability inspection;
- open/close/reconfiguration;
- sample-rate/block-size negotiation;
- audio/MIDI input and output;
- clock/timestamp integration where available;
- xrun/error reporting and recovery policy;
- handoff into the same Chassis processing/graph runtime used elsewhere.

Standalone applications reuse the same component/editor/state implementation rather than growing a second DSP path.

## Graph and scheduling

A processing graph is planned Chassis infrastructure and sits above the component core.

It should provide generic audio-engine facilities such as:

- ordinary Chassis processors/components as nodes;
- audio/event fan-in/fan-out;
- routing and bus connectivity;
- validated immutable/transactional execution plans;
- latency propagation and compensation;
- bounded realtime-safe scheduling;
- topology/resource mutation away from the callback;
- parallel scheduling only when measurements and workloads justify it.

The graph does not know what a DAW track, clip, arrangement, mixer strip, project, or session view is.

## Plugin hosting

Plugin hosting is planned Chassis infrastructure for DAWs, hosts, test tools, and larger audio applications.

Hosting should eventually cover format discovery/scanning, validation/isolation policy, instantiation, audio/event processing, parameter/state access, editor hosting, and integration with Chassis graph/scheduling. Host-specific plugin state remains behind the host layer rather than leaking into the semantic processor core.

Sandboxing/out-of-process hosting may be added when a concrete crash-isolation requirement justifies it.

## Media and offline processing

General audio applications need reusable media/offline infrastructure. Chassis should provide coherent integration for:

- audio source/sink metadata;
- stream/read/write/seek semantics;
- decode/encode through reviewed codec libraries;
- sample-rate/channel adaptation through reviewed DSP libraries;
- deterministic offline rendering;
- file/asset processing that feeds or consumes Chassis processors/graphs.

Chassis does not need to own codec implementations. Application media libraries, project asset management, and editing workflows remain outside the framework.

Random-access/multi-pass processing may later gain a dedicated abstraction when restoration, analysis, mastering, or DAW workflows prove requirements that sequential block processing cannot express cleanly.

## Transport and time

Processor-facing transport remains an availability-aware semantic view rather than a DAW document model.

The broader application layers may provide reusable time/clock primitives needed to drive processors and graphs: sample time, seconds, beats, tempo/time-signature information, looping/cycle state, and deterministic offline position. More elaborate tempo-map/edit models should be added only if they can remain independent of one application's arrangement model.

## Application boundary

Chassis should support building a DAW without becoming one.

Application-owned examples include:

- tracks and track types;
- clips/regions and arrangement/session models;
- piano-roll/editor semantics;
- project files beyond processor/framework state;
- undo/redo and editing command models;
- mixer/workflow UX;
- media-library/project-management policy;
- mastering/restoration product workflows.

Those applications consume Chassis device, graph, host, processor, media, rendering, and UI-integration facilities.

## Rust and dependency policy

Development follows the repository's `stable` Rust toolchain and Edition 2024. During pre-alpha there is no promised MSRV beyond what Edition 2024/tooling require; do not pin a compiler merely to chase the latest release, and do not raise a future declared MSRV without an explicit reason.

Format-independent crates deny unsafe code. FFI/platform adapters isolate necessary unsafe code, use `unsafe extern` where required by Edition 2024, require a `// SAFETY:` explanation for every unsafe block, and keep `unsafe_op_in_unsafe_fn` denied.

Public API is deliberate: use domain newtypes/enums, avoid accidental re-exports, and keep implementation-only dense indices/backend IDs out of stable author-facing identity.

Core dependencies stay small and audited. Realtime-path dependencies receive stricter review for allocation, locks, unsafe invariants, maintenance quality, and measurable overhead than non-realtime tooling dependencies.
