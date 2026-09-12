# Current Execution Plan

This file tracks the next implementation order on `main`. It is not an API compatibility promise.

## Current checkpoint

The format-independent runtime and native CLAP adapter are implemented and qualified enough to stop using an existing product migration as the primary roadmap driver. Existing Tonal EQ, delayed-probe, and conformance coverage remain useful regressions, but Chassis itself is now the priority.

The [validation guide](design/validation.md) owns executed checks, dated REAPER results, and representative adapter benchmarks. The high-level completion bar lives in [roadmap.md](../docs/roadmap.md).

## Execution principle

Implement the reusable audio-framework capabilities that are already known to be necessary across multiple project classes. Use small purpose-built fixtures to prove them. Do not block framework completion on parity with an old plugin that was not itself a production-quality target.

At the same time, do not turn “finish Chassis” into speculative framework building. Product-specific DSP, restoration/ML algorithms, synth engines, timelines, mixer UX, and application workflows stay outside core.

## Slice 1 — realtime communication primitives

This is the immediate implementation target.

Add the smallest reusable primitives needed for professional processor/editor interaction:

1. bounded DSP -> non-realtime telemetry suitable for meters and analyzer/control evidence;
2. explicit publication/coalescing semantics with no audio-thread allocation, blocking, or reclamation surprises;
3. immutable control -> DSP snapshot publication for product control state that does not fit ordinary scalar parameters;
4. tests for concurrent publication/read, stale/coalesced observations, bounded retry/work, and destruction ownership.

Start with a safe scalar/fixed-size telemetry primitive that can be proven without introducing generic unsafe lock-free containers. Expand only when analyzer or other fixture requirements demonstrate the need.

Validation fixture: a meter component that publishes block-level values and can be observed without mutating DSP ownership.

## Slice 2 — core authoring/API closure

Resolve the remaining general pre-v1 authoring gaps:

- ergonomic `Component` -> `InstanceRuntime` construction while preserving explicit validation;
- semantic whole-I/O configuration for products with multiple legal layouts;
- recurring bus/port access ergonomics only where the core buffer legality remains visible;
- canonical product/export identity owner;
- parameter formatting/value mapping with tested round trips;
- modulation as process-time control distinct from durable/base state;
- common tail/bypass semantics where adapters can map them faithfully.

Do not add proc macros or derives until these contracts settle.

Validation fixtures: gain, delay, sidechain, and layout-switching processors.

## Slice 3 — event/note/MIDI execution

Promote `docs/design/events-transport.md` from design direction into executable APIs and adapters:

- stable event-port identities;
- note on/off/choke/end and typed note addresses;
- note velocity/tuning/expression;
- raw MIDI/SysEx;
- sample-accurate modulation;
- bounded event output with explicit rejection;
- typed borrowed cursors/span processing that preserve source ordering semantics without inventing a global order.

Validation fixtures: basic synth, event passthrough/transform, and multi-output instrument.

## Slice 4 — production editor contract

Define and implement framework-level editor integration:

1. product-originated begin/change/end gestures with echo suppression;
2. parameter observation/binding;
3. telemetry observation using Slice 1 primitives;
4. parent attach/detach and editor recreation;
5. resize, scale, high-DPI, focus/input, and destruction semantics;
6. accessibility path where practical.

Select a first GUI adapter only after the lifecycle contract is clear. Keep toolkit types out of core.

Validation fixture: `EditorDemo`, deliberately simple visually but exhaustive in lifecycle/interaction behavior.

## Slice 5 — VST3 and Audio Unit parity

Project the same Chassis component semantics through reviewed adapters/wrappers and qualify them independently:

- native validators;
- state/identity fixtures;
- automation/modulation trajectories;
- audio/event layout tests;
- latency/PDC/tail/bypass parity;
- editor lifecycle/resize/scaling;
- real-host save/reopen/automated-render scenarios;
- differential comparisons against native CLAP where source semantics are equivalent.

A wrapper is acceptable only while it maps Chassis semantics faithfully. Own native adapter code when evidence shows a material limitation.

## Slice 6 — background work and larger snapshots

Once the first concrete analyzer, convolution, restoration, or model-backed fixture requires it, add explicit task support:

- per-instance ownership;
- cancellation;
- generation/stale-completion rejection;
- deterministic unload/shutdown;
- result destruction off realtime paths;
- offline determinism;
- no hidden global executor.

This slice may move earlier if a prior fixture cannot be implemented correctly without it.

## Slice 7 — standalone and devices

Use the same component/runtime/editor/state implementation outside plugin hosts:

- audio devices;
- MIDI devices;
- sample-rate/block-size negotiation;
- device changes;
- xruns and recoverable errors;
- standalone application bootstrap.

Do not create a parallel standalone DSP architecture.

## Slice 8 — graph/scheduling

Add application-layer `chassis-graph` after component/device semantics are stable:

- ordinary Chassis processors as nodes;
- audio/event fan-in/fan-out;
- validated immutable/transactional execution plans;
- latency propagation/compensation;
- realtime-safe execution;
- topology/resource changes off the callback;
- parallel scheduling only when measurements justify it.

Do not put timelines, project semantics, mixer UX, or media management into the graph layer.

## Slice 9 — release/tooling hardening

Finish the path from source to distributable project:

- canonical product/export manifest;
- generated backend metadata;
- plugin/standalone packaging;
- signing/notarization hooks;
- reproducible release builds;
- validator and host smoke-test commands;
- state/identity compatibility fixtures;
- examples/templates for common component classes.

## Existing validation retained

The following remain valuable and should not be removed merely because the roadmap changed:

- CLAP validator and bounded fuzz matrix;
- in-process Clack host tests;
- callback allocation/deallocation probes;
- adapter benchmark;
- REAPER automation/state/PDC baseline;
- Tonal EQ parameter-scale, automation, state, latency/restart, differential-DSP and host-render tests where they continue to prove framework behavior;
- delayed probe for latency/PDC.

Tonal EQ product redesign or parity beyond what proves Chassis behavior is not a Chassis milestone.

## Processor restart requests

A processor whose observed controls require different activation resources sets `Processor::restart_requested()`. It preserves the active resources and latency until the host deactivates it. `InstanceRuntime::restart_requested()` exposes this semantic signal to embeddings. The CLAP adapter publishes completed parameter endpoints before forwarding at most one host restart request per activation.

Controls received through active flush are evaluated on the next process call. A successful CLAP state load requests restart immediately after publication and value rescan because it may replace activation-time resources before processing. A host may defer either request, so processing must remain valid meanwhile; a request does not guarantee that a single-shot offline render will start with newly requested resources.
