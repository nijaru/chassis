# Roadmap

Chassis aims to be a reusable Rust audio framework, not only a plugin SDK. Applications own product DSP, visual design, and DAW document/workflow semantics. [Architecture](architecture.md) defines that boundary.

This roadmap records unfinished capabilities, not a requirement to build every layer before any useful surface can stabilize. [Current execution plan](next-runtime-slice.md) owns immediate sequencing. Package names and boundaries are chosen from implementation needs, not this list.

## What exists

- One format-independent schema/instance/processor model, including complete semantic state and activation-scoped audio I/O.
- Direct runtime fixtures for effects, event-only components, a note-driven instrument, multiple outputs, sidechains/layout switching, and dynamic metadata.
- Safe buffer traversal, f32/f64 processing, typed parameters, sample-accurate automation, latency/restart, realtime/buffered/offline modes, and bounded scalar telemetry.
- Native CLAP deployment for fixed mapped audio configurations, scalar parameters/state, automation, render mode, and latency.
- Rust/concurrency/allocation tests, packaged CLAP validator/fuzz gates, and dated host/performance baselines.

Core capability is not deployment support. CLAP note/event/custom-state projection remains incomplete; VST3/AU, editors, devices, graphs, hosting, and media integrations are not implemented. [Validation](design/validation.md) owns exact evidence and its revision limits.

## Near-term semantic and deployment work

1. Resolve direct-runtime versus CLAP automation endpoint carryover authority. Extend complete-state projection only with coherent ownership; unsupported CLAP custom fields/event ports are now rejected.
2. Unify author-defined parameter mapping/formatting/parsing policy and adapter conversion.
3. Define modulation independently from automation/base state and qualify its projection.
4. Establish tail and host-bypass contracts without prescribing one DSP algorithm.
5. Export a non-effect fixture: note ports/input, honest descriptor features, then bounded event output and expression/MIDI families.
6. Add prepared control-to-DSP publication/reclamation and background-work lifecycle from a resource-backed fixture.

These are independently deliverable contracts. Do not turn “core closure” into a blanket freeze or defer all deployment feedback until after a large core redesign.

## Remaining framework directions

| Area | Required contract/evidence before support claims |
| --- | --- |
| Common DSP utilities | Reusable units, ramps, buffers, metering, filters, and reviewed FFT/resampling/convolution integrations as actual workloads require; numerical behavior and realtime/offline parity |
| Editor integration | Product gestures with echo suppression, observation/binding, attach/detach/recreation, sizing/scaling/focus/teardown; optional toolkit dependencies; a real editor fixture |
| VST3 / Audio Unit | Reviewed wrapper or justified native implementation; differential identity/state/audio/event/automation/modulation/latency/tail/bypass and editor qualification |
| Devices / standalone | Audio/MIDI enumeration, open/close/reconfiguration, negotiation, timestamps, xruns/device loss/recovery; reuse the same runtime rather than a second DSP path |
| Graph / scheduling | Audio/event routing, validated execution plans, topology publication, latency compensation, bounded execution, deterministic offline behavior; parallelism only with evidence |
| Plugin hosting | Scanning/loading/lifecycle, processing/parameters/state/editor access, failure policy, graph integration; plugin ABI types stay out of semantic runtime APIs |
| Media / offline applications | Reviewed codecs, source/sink/seek/write semantics, rate/channel adaptation, deterministic rendering; separate random-access/multi-pass abstraction only if required |
| Authoring / delivery | Executable examples, useful public packaging, identity/export metadata, build/validation/packaging tools, signing hooks where required, compatibility fixtures and documented guarantees |

A graph does not own tracks or clips. Media integration does not own project asset management. Editor integration does not own a widget library or visual identity. Existing product ports are useful evidence, not completion gates or migration commitments.

## Conditional work

AAX depends on SDK/licensing/access and demand. ARA, LV2, sandboxed hosting, surround/ambisonics, immersive/object metadata, network audio, and specialized ML integrations require explicit supported use cases. Labeled channel layouts need real mapping/ordering evidence; a generic channel count is insufficient. These directions do not justify prebuilt abstraction families.

## Promotion and stability

Promote a facility when its owner, lifecycle, failure behavior, and boundaries are explicit; it removes concrete reusable plumbing; realtime work/resources are bounded; and deployments preserve its semantics or reject unsupported use. Add focused fixtures for the uncovered risks, not a catalog of named demos.

Stabilize a supported surface only after its actual clients and deployment boundaries exercise it. State/identity compatibility, MSRV, supported feature/platform combinations, host evidence, and known limitations must be explicit. Unrelated future graph/device/media work need not block a narrower qualified release, and a narrower release does not imply support for the whole roadmap.

The project remains unpublished pre-alpha. Version milestones and publication require explicit release decisions; this roadmap does not authorize them.
