# Architecture

## Purpose

Chassis is a convention-first Rust framework for professional audio components. Its first production use is plugins, beginning with effects, but the core model must not assume that a DAW is always the owner of processing. The same product component should be deployable as a plugin, standalone processor, or embedded component when the surrounding adapter exists.

The framework should own behavior that is common across a large fraction of effects and instruments. Product-specific DSP, visual identity, project/document semantics, and domain workflows remain outside Chassis.

## Design principles

### Convention over configuration

Common plugin behavior should be built in and require little ceremony. Defaults must be replaceable when a product needs different behavior.

Examples:

- an effect defaults to stereo main input/output;
- a stereo sidechain is a standard optional port;
- parameter types provide conventional host metadata and formatting hooks;
- state is versioned by default;
- realtime-safe communication has standard framework primitives;
- validation and packaging are normal framework workflows rather than product-specific scripts.

### Structural realtime safety

Thread ownership should be reflected in types and capabilities rather than primarily in comments.

The intended conceptual split is:

```text
Product
├── Processor      audio-thread-owned mutable processing state
├── Controller     main/control-thread product state and host interaction
├── Shared         explicitly thread-safe shared state only
└── Editor         optional UI/main-thread capability
```

The exact traits are not frozen yet. The important invariant is that an editor or state serializer should not receive unrestricted mutable access to realtime processing state, and audio-thread code should not gain accidental access to blocking or allocation-heavy framework services.

### Backend independence

The product-facing core API must not expose CLAP, VST3, AU, Clack, clap-wrapper, GUI toolkit, or operating-system types.

Initial format strategy:

```text
Chassis product API
        ↓
  chassis-clap
        ↓
      Clack
        ↓
       CLAP
        ↓
  clap-wrapper where suitable
   ├── VST3
   ├── AUv2/AUv3
   └── standalone bootstrap
```

Native adapters may replace wrappers later when measured capability, correctness, or maintenance requirements justify them.

### Product ownership

Chassis does not decide how an EQ filters, how a compressor detects gain reduction, how a synth allocates voices, or how a mastering application models revisions. Those semantics belong to the product.

Chassis may eventually provide optional reusable utilities when repeated real products establish that their semantics are genuinely common.

## Core component model

A Chassis audio component needs common contracts for:

- lifecycle and activation;
- processing configuration;
- audio ports and channel layouts;
- realtime/offline process context;
- parameters, automation, and modulation;
- note/MIDI/event streams;
- host transport information;
- latency, tail, bypass, and restart/change notifications;
- versioned state and migration;
- controller/main-thread communication;
- optional editor integration;
- bounded background work and realtime-safe telemetry.

Effects and instruments share this model. An instrument may have no audio input and multiple outputs. A MIDI processor may have event input/output and no audio. An effect may expose a sidechain or auxiliary buses.

## Audio ports and channel layouts

Stereo is the first production target, not an architectural assumption.

The framework models named/identified ports with semantic roles and channel layouts. The initial convenience convention for effects is:

```text
main input:     stereo, required
main output:    stereo, required
sidechain:      stereo, optional
```

The representation must be extensible to:

- mono and arbitrary discrete channel counts;
- surround layouts;
- ambisonics;
- immersive/Atmos-style layouts;
- multiple input/output buses;
- instrument multi-output routing.

Chassis should provide named convenience layouts only where their semantics are stable. Format adapters remain responsible for mapping Chassis layouts to each host API.

## Parameters and state

Parameters should eventually provide conventional typed definitions for float, integer, boolean, and enumerated values, with:

- stable IDs;
- normalized/plain conversion;
- units and display parsing/formatting hooks;
- host metadata;
- automation/modulation delivery;
- begin/change/end gestures;
- optional smoothing helpers with replaceable product semantics.

State should be versioned by convention and support explicit migrations. Host state is a product contract and must remain deterministic and testable.

Preset browsing, tagging, cloud sync, and product-specific preset UX are not core responsibilities. Common preset serialization/storage helpers can be added once requirements are proven.

## GUI boundary

Chassis owns editor lifecycle and host-window integration, not visual design.

Core responsibilities include:

- editor creation/destruction;
- parent/native-window attachment;
- logical/physical sizing and scale handling;
- host resize negotiation;
- parameter gestures and value observation;
- main-thread scheduling;
- realtime-to-GUI telemetry primitives.

Toolkit adapters such as `chassis-iced` can make the conventional path easy without requiring every Chassis product to use that toolkit.

## Standalone and embedded deployment

Standalone is a planned first-class deployment mode, especially useful for instruments and analyzers. A product component should not need a second DSP/state implementation to run outside a plugin host.

A future standalone layer may own:

- audio and MIDI device selection;
- sample rate/buffer configuration;
- application window/editor embedding;
- persistence and preset access;
- transport where relevant.

Embedded deployment allows a Chassis component to run inside another Rust audio application without pretending to be a plugin.

## Larger audio applications

The core should leave room for future `chassis-host`, `chassis-device`, and `chassis-graph` layers. Those could support plugin hosts, live processors, mastering applications, or DAW-like software.

They are not part of the initial implementation scope. Chassis should not define application-specific concepts such as timelines, projects, arrangements, media libraries, revisions, mixers, mastering QC, or delivery workflows.

The boundary is intentional: Chassis can eventually supply a reusable realtime/audio-component runtime underneath an application without becoming the application's product model.

## Dependency policy

Core dependencies should be small, well-audited, and compatible with Chassis's AGPL/commercial dual-licensing model. Prefer permissive dependencies for code incorporated into commercial builds.

Unsafe code is denied in format-independent workspace crates by default. If an adapter requires unsafe/FFI code, keep it isolated, document invariants, and test it independently.
