# Roadmap

Chassis's initial target is a complete, correct, and stable DSP execution core, not a broad application framework. [Architecture](architecture.md) owns scope and ownership; [the execution plan](next-runtime-slice.md) orders current work; [validation](design/validation.md) records executed evidence. All APIs and CHSS bytes remain pre-alpha.

## Phase 1 — finish the shared DSP core

The core is component execution and the difficult machinery around it. It is not a multi-node graph or a complete audio application engine.

| Maintained concern | Initial boundary |
| --- | --- |
| Component execution | Coherent schema, accepted whole-I/O configuration, safe f32/f64 buffers, dense process identities, activation/reset/deactivation, direct embedding and sequential offline use |
| Controls and events | Typed base values and trajectories, explicit modulation, gestures/control origins, source ordering, native notes/expression and opaque MIDI transport; unsupported capabilities remain explicit |
| Semantic state | Complete validated replacement, migrations, coherent save, stable identity, and a save/load contract consistent with admitted control changes |
| Resources | Preparation away from the callback, bounded adoption of compatible prepared data, stale-result rejection, off-thread reclamation, and safe cancellation/teardown; reactivation for incompatible resources |
| Shared services | Bounded control/DSP publication, scalar observations, explicit latency/tail/bypass semantics, and observable overload/failure policies |
| Authoring | Useful typed/scoped access and a small set of workload-backed helpers; lightweight kernels need no component identity or persistence machinery |
| Existing deployment | Maintain CLAP's faithful projection, packaging, validator/fuzz checks, and actual-entry tests while the core evolves |

Not every component uses every service. Optional core capabilities must impose no hidden executor, queue, allocation, or observation work when unused. Resource lifetime/publication is committed core work; a general-purpose executor, disk streamer, FFT backend, and instrument engine are not.

The current runtime and CLAP fixtures implement much of this boundary, but prepared adoption, deployment-integrated control/gesture delivery, and other closure work remain. Implemented inventories and revision-scoped evidence belong to their contracts and the validation guide, not a second completion log here.

Established effect, instrument, resource-backed, and event-processing workloads are sufficient requirements sources. No named product migration is needed. Equally, a feature's presence in a DAW or another framework does not make it a core prerequisite. Per-note parameter modulation, timestamped transport, wider channel-layout families, and larger observation streams can remain explicitly unsupported in the initial stable surface.

## Usable milestones within Phase 1

Deliver these cumulatively: M1 → M2 → M3 → M4. They are functional checkpoints, not version numbers or automatic releases. All remain pre-alpha until an explicit compatibility/stability decision. None is declared complete by this plan; current evidence and gaps remain owned by the design contracts and validation guide.

At every milestone, provide a documented runnable public-API example or existing fixture, focused regression coverage, relevant realtime/resource-bound checks, and explicit supported behavior and exclusions. Reuse valid evidence and existing fixtures; do not create a demo catalog. Keep CLAP actual-entry and packaged qualification current when its boundary changes. Authoring usability, failure behavior, and representative cost checks accompany each slice rather than waiting for M4.

### M1 — dependable fixed-resource DSP

**Usable outcome:** build a basic effect, instrument, or event processor through direct Rust embedding or CLAP, with activation-owned resources and dependable state recall.

Required slices:

- **State/control closure:** accepted edits and automation preserve complete save/reopen and reactivation under the same schema/policy; rejected replacements remain failure-atomic.
- **Lifecycle and failure closure:** validate the supported activation/reset/deactivation and process-failure rules, endpoint completion, and input/output capacity outcomes before calling the subset dependable.
- **End-to-end subset proof:** exercise supported audio/event/control behavior and authoring through existing public-API fixtures, with bounded callback work and no allocation/reclamation regression.

**Exit:** state/control closure holds, supported lifecycle/failure cases are protected, and the fixed-resource examples run with documented limits and revision-scoped qualification. Current CLAP plain-step control and existing declared capabilities remain usable; broader trajectory portability is completed in M2.

**Not required:** compatible resource hot-swapping, new gesture APIs, a GUI, a device backend, or another format. [State validity](design/parameters-state.md#state-validity-closure) defines independent parameter domains and custom-state transactions; completing that slice alone does not finish the milestone.

### M2 — complete control handling

**Usable outcome:** application controls and host automation can drive the M1 processors without each consumer inventing its own state synchronization and notification protocol.

Required slices:

- **Trajectory fidelity:** establish source-domain and block-boundary evaluation with independent reference cases, including nonlinear mapping and the implicit preceding-sample anchor.
- **Control origins and gestures:** implement typed begin/change/end changes, host notification ordering, observation and echo suppression, including stopped processing and pending reactivation.
- **Bounded control delivery:** qualify contention/capacity outcomes and state precedence without callback waits, hidden allocation, or silent success-shaped loss of essential controls.

**Exit:** a headless control fixture and the existing CLAP boundary demonstrate correct origin/gesture behavior, values and trajectories, including state races and declared overload outcomes. M1 behavior remains qualified after any API migration.

**Not required:** an editor toolkit/window, per-note parameter modulation, timestamped transport, or a VST3 adapter. Trajectory representation and control-publication design must be chosen from their contract proofs, not fixed in the roadmap; see [parameters/state](design/parameters-state.md).

### M3 — resource-backed DSP

**Usable outcome:** a processor can replace compatible prepared tables, samples, or kernels while running, without each product rebuilding unsafe lifetime and reclamation machinery.

Required slices:

- **Preparation and adoption:** publish and adopt compatible data with bounded work; retain reactivation for incompatible configuration/resource changes.
- **Supersession and reclamation:** handle full publication capacity, delayed consumers and stale completions while keeping destruction off the callback.
- **Cancellation and teardown:** qualify load/reset/deactivation/destruction and late worker completion, including module-unload fencing and repeatable offline preparation.

**Exit:** an in-repository resource-backed public-API fixture demonstrates replacement, failure, supersession and teardown with ownership/concurrency and callback-bound evidence. M1/M2 state and control guarantees survive those transitions.

**Not required:** a general-purpose executor, decoder, disk streamer, FFT/convolution backend, or production sampler. Resolve compatible-adoption policy and object/result ownership before selecting the mechanism; see [runtime services](design/runtime-services.md#control---realtime-publication).

### M4 — qualified core baseline

**Usable outcome:** consumers can depend on an explicitly declared core surface rather than infer support from individual examples.

Required slices:

- **Integrated authoring and costs:** consolidate evidence across the effect, instrument, event-only and resource-backed cases; remove superseded APIs and resolve measured ergonomic/execution-cost problems.
- **Deployment and host qualification:** complete current Rust/dependency and relevant allocation/concurrency/unsafe checks, CLAP packaging/validator/fuzz, and the supported real-host scenarios.
- **Compatibility review:** settle the supported API/features/platforms, compiler policy, error/resource limits, state fixtures/migrations and known exclusions.

**Exit:** all [core stability gates](#core-stability-gates) have matching evidence, with blockers and unsupported cases explicit. Review API and wire-format commitments separately; an explicit stability/version/publication decision is still required.

**Not required:** expanding the core capability list, implementing a higher layer, or publishing a release. Qualification at this milestone consolidates earlier checks; it is not the first time earlier slices are tested.

## Core stability gates

Complete these before expanding into higher layers or declaring the initial core stable:

1. **Consistent semantics.** Resolve known admission/state/resource contradictions. Successful framework-generated state must reload under the same schema and component policy. Define how admitted automation, coupled state, desired controls, and applied resources preserve that property; do not repair invalid state silently at save time.
2. **Portable processing contracts.** Specify trajectory domains and boundary anchors, supported event targeting, layout semantics, offline behavior, and failure transitions. Qualify typed gestures/control origins, observation and echo suppression without a window-system dependency. Use primary format contracts and bounded source-level probes to challenge CLAP-centric assumptions without requiring another production adapter.
3. **Closed resource lifecycles.** Qualify preparation, publication, adoption, supersession, cancellation, deactivation and unload. No last-owner destruction or waiting on the callback; no background result may revive retired instance state. Preparation required for repeatable offline output must not depend on wall-clock races.
4. **Usable, bounded authoring.** Exercise linked-channel/sidechain DSP, typed controls, direct embedding, and a resource-backed processor. Measure realistic parameter/event/channel workloads; document supported budgets and overload behavior rather than equating bounded work with negligible cost. Add helpers only where they remove demonstrated repeated work.
5. **Matching evidence.** Pass repository Rust/dependency gates, focused allocation/concurrency/unsafe checks, and current CLAP package/validator/fuzz qualification. Refresh the supported real-host automation, save/reopen, restart and PDC scenarios. Keep host, performance, format, and Rust evidence separate and tied to the tested revision.
6. **Explicit compatibility decision.** Review the supported API/feature/platform matrix, error and resource limits, state/identity policy, versioned compatibility fixtures, compiler policy, and known exclusions. State the API and wire-format commitments separately. Stability, version milestones, publication, and commercial terms still require explicit decisions.

Completeness is relative to this declared boundary, not every future audio capability. Existing implementations and old tests do not freeze weak interfaces: replace them and migrate all callers while pre-alpha. A contract-level decision remains open until implementation and relevant checks support it.

## Phase 2 — review a narrow extension after core stability

Choose the next useful layer only after M4 and Phase 1's gates are met. Do not start a parallel application-framework expansion to avoid unfinished core work. The existing CLAP boundary continues to supply integration feedback throughout Phase 1.

Possible candidates are a standalone device path, an editor adapter, an additional plugin format, or serial graph composition. Select one from user value, its dependency/safety boundary, and the maintenance cost; qualification must include its lifecycle and failure cases. A complete GUI, standalone product, graph scheduler, or second format is not an initial-core gate.

## Possible directions, not maintenance commitments

| Candidate | Decision needed before adoption |
| --- | --- |
| More DSP utilities or reviewed FFT/resampling/convolution integration | Which reusable numerical contract is needed, why an existing library is insufficient on its own, and who maintains the integration |
| Editor/toolkit integration and audio visualization | Supported attachment/threading/teardown and observation needs; core gestures do not require a window system, widget library, or renderer |
| VST3 / Audio Unit | Wrapper versus native adapter, semantic parity, target hosts, packaging and sustained conformance effort |
| Audio/MIDI devices and standalone bootstrap | Supported backends/platforms, actual configuration and clock mapping, loss/recovery and xruns |
| Graph execution and PDC | A separate composition/scheduling contract, topology and latency publication, cycle policy, processor-history preservation, and measured execution cost |
| Plugin hosting | Discovery/crash handling, lifecycle/editor/state integration, third-party failure boundaries and isolation policy |
| Media, streaming, range rendering and analysis jobs | Seek/history semantics, cache and I/O budgets, underrun policy, cancellation, output integrity and repeatability; not one universal block-processing trait |

Use mature dependencies where they fit the safety, numerical, and licensing requirements. Add packages only for actual dependency, safety, platform, optionality, or release boundaries. AAX, ARA, network audio, immersive formats, mobile/web/embedded targets, and GPU/ML integrations require separate scope and maintenance decisions; none is implied by a generic core.

## Out of scope

Chassis does not own product algorithms, DAW tracks/clips/arrangements, application documents and undo, asset organization, or product UX. It does not aim to supply a general GUI toolkit, graphics engine, video pipeline, audiovisual composition system, or a replacement implementation of every DSP primitive and backend.

An application can use Chassis as one part of a DAW, editor, instrument, analyzer, or other audio product without Chassis implementing the rest of that product's engine. Future extension choices must preserve that distinction.
