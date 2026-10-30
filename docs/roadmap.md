# Roadmap

Chassis aims to be a reusable Rust audio framework, not only a plugin SDK. Applications own product DSP, visual design, and DAW document/workflow semantics. [Architecture](architecture.md) defines that boundary.

This roadmap records unfinished capabilities, not a requirement to build every layer before any useful surface can stabilize. [Current execution plan](next-runtime-slice.md) owns immediate sequencing. Package names and boundaries are chosen from implementation needs, not this list.

## What exists

- One format-independent schema/instance/processor model, including component-owned state defaults/validation/migrations, resource-safe deferred reactivation, and activation-scoped audio I/O.
- Direct runtime fixtures for effects, event-only components, a note-driven instrument, multiple outputs, sidechains/layout switching, and dynamic metadata.
- Complete callback topology validation, alias-classified CLAP buffers, f32/f64 processing, typed parameters/dense choice events, sample-accurate automation, explicit custom-state reactivation, latency/restart, process modes, and bounded scalar telemetry.
- Explicitly opted-in global additive float modulation, separate held-input lifetime, clamped effective cursors, and resource-safe desired/consumed signal separation in core and CLAP.
- Descriptor-owned normalized-control and formatting/parsing policy, shared by framework controls and deployment text without changing plain-unit automation/state.
- Native CLAP deployment for fixed mapped audio configurations, typed parameters, coherent complete state with deferred resource reactivation, automation, render mode, latency, completed tail snapshots, and explicit timed Boolean bypass.
- Native CLAP note ports, bounded input/output and voice-end feedback, all seven expressions, preserved mixed input order, and an output-only stereo instrument export.
- Opaque MIDI 1/SysEx and four-word MIDI 2/UMP transport in core and CLAP, callback-local payload loans, per-message port capabilities, shared note/MIDI output ordering, and a zero-audio mixed-dialect processor export.
- Rust/concurrency/allocation tests, packaged CLAP validator/fuzz gates, and dated host/performance baselines.

Core capability is not deployment support or MIDI 2 device/protocol compliance. VST3/AU, editors, devices, graphs, hosting, and media integrations are not implemented. [Validation](design/validation.md) owns exact evidence and its revision limits.

## Near-term semantic and deployment work

1. Extend per-note parameter modulation, span processing, or other event domains when a concrete client establishes the required semantics; preserve the existing bounded opaque transport without lossy conversion.
2. Add prepared adoption/reclamation and background-work lifecycle only when the resource-backed client needs more than reactivation.

Automation completion has one runtime owner; CLAP synchronizes/publishes dense values without replaying events or scanning stable choice identities on the callback. A reviewed local Clack plugin patch returns rejected-process failure without synchronous reporting. Unsupported CLAP event dialects remain explicitly rejected.

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
