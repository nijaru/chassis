# Open Questions and API Freeze Gates

This file records unresolved design decisions that still block API or stability claims. Completed proof steps belong in `docs/design/validation.md`; execution order belongs in `docs/next-runtime-slice.md`; the broad completion bar belongs in `docs/roadmap.md`.

Chassis is unpublished pre-alpha. Existing product migrations and current public APIs are evidence sources, not compatibility constraints.

## Component schema and runtime authority

The target direction is documented in `docs/design/component-schema.md`.

Implemented and exercised:

- one coherent `Component::schema()` authority for state identity, audio ports, audio-I/O policy, event ports, and parameters;
- `InstanceRuntime<P>` ownership of one validated immutable schema generation;
- neutral core semantics with conventional effect helpers above the core model;
- stable audio/event/parameter identities with dense activation-local indices for processing;
- dense-only audio callback endpoints and dense note/event port addressing;
- runtime-owned audio/event metadata for dynamically constructed/hosted components;
- semantic state identity/version separated from deployment/export identity;
- schema and policy drift rejection before product activation;
- activation-owned resolved audio endpoint legality;
- schema-owned whole-I/O policy;
- runtime state save/load with one schema-owned identity authority;
- removal of proof-order constructors and runtime explicit-product-ID/parameter-only state compatibility APIs;
- direct non-plugin runtime proofs plus conventional effect, event-only, instrument, multi-output, multi-layout/sidechain, dynamic-metadata, and CLAP adapter proofs.

The current `ComponentSchema` / `InstanceRuntime` ownership model is therefore a candidate-stable architecture. It is not a compatibility promise yet, but there is no unresolved schema migration that justifies reopening the model speculatively.

Remaining questions that could still affect this surface:

- whether materially larger configuration families eventually justify rule-based `AudioIoPolicy` variants beyond explicit enumeration;
- whether repeated real component patterns justify more authoring conveniences without creating parallel lifecycles;
- whether graph/device/hosting layers expose a concrete capability or ownership requirement not expressible by the current schema.

Do not add generality until one of those clients proves the need.

## Process buffers and I/O

Implemented:

- generic safe buffer sources without callback-owned channel vectors;
- exact in-place, separate, input-only, and output-only relationships;
- dense activation-local audio endpoints;
- activation-owned endpoint legality validation for active port, direction, and channel range;
- whole-I/O policy acceptance before activation;
- dynamic reactivation across accepted layouts;
- multi-output routing by semantic dense identity rather than callback ordering.

Remaining freeze questions:

- whether recurring bus/port access patterns justify a more ergonomic view API without weakening explicit legality;
- backend-specific inactive/null/zero-buffer semantics in each deployment adapter;
- benchmark controlled copy/conversion paths before adding unsafe or ownership complexity merely to remove them;
- cross-format layout parity once VST3/AU projections exist;
- surround/ambisonics/immersive semantics only with explicit ordering/mapping fixtures.

Native f64 processing is already qualified headlessly. Wire-precision preference remains deployment policy.

## Parameters, automation, modulation, and gestures

Implemented:

- typed parameter schema/store;
- dense realtime parameter indices;
- borrowed sample-accurate automation with set/linear trajectories;
- CLAP float/integer/boolean/choice projection;
- generation-checked scalar publication;
- state value rescan after load;
- sample-accurate exported automation for f32/f64.

Freeze gates:

- parameter formatting/value-mapping helpers with explicit round-trip/domain tests;
- product/editor begin/change/end gesture semantics and echo suppression;
- sample-accurate modulation distinct from durable/base state publication and ordinary automation;
- adapter-specific automation/modulation semantics must be preserved rather than normalized to a weaker invented model;
- production-host automated-render evidence for supported formats.

## State identity, save/load, and compatibility

Implemented:

- semantic state identity and schema version owned by `ComponentSchema` / `InstanceRuntime`;
- complete runtime state export/load without duplicate identity/version arguments;
- runtime explicit-product-ID and parameter-only compatibility paths removed;
- active state save serializes one coherent completed scalar generation;
- newer control/state generations win over stale realtime endpoint publication;
- state load is bounded, migratable, transactional, and failure-atomic;
- CLAP state save/load consumes schema-owned semantic identity while CLAP deployment identity remains separate;
- direct tests, adversarial decode/load tests, Loom, and in-process host coverage exercise the current behavior.

Remaining freeze gates:

- cross-format state round trips for every supported deployment format;
- decide when the CHSS wire envelope becomes a compatibility promise;
- define semver/state compatibility/versioning policy before a stable release;
- establish canonical product/export manifest/tooling without making deployment IDs semantic-state identity.

## Latency, tail, bypass, and delayed processing

Implemented:

- `LatencySamples`;
- activation-scoped latency snapshot;
- processor restart request semantics;
- CLAP latency projection/notification;
- delayed probe and recorded REAPER PDC evidence.

Still open:

- common tail semantics;
- bypass semantics and whether hard/soft/product bypass require distinct capabilities;
- authoring/resource helpers for lookahead, oversampling, convolution, etc. only where repeated use proves value;
- graph-level latency propagation/compensation;
- cross-format parity.

## Realtime communication and background work

Implemented:

- `F32Telemetry` fixed-width coherent DSP -> non-realtime snapshots;
- nonblocking publication and bounded coherent reads;
- concurrent generation tests;
- measured callback allocation/deallocation coverage;
- meter fixture through the public runtime.

Remaining:

- immutable control -> DSP publication for non-parameter state;
- deferred destruction/reclamation of replaced large objects;
- larger/high-rate analyzer transport only if fixed snapshots are the wrong representation;
- per-instance background-task ownership, cancellation, generation/stale-result rejection, shutdown/unload, result publication, and offline determinism.

No hidden global executor or generalized lock-free container library.

## Events, notes, MIDI, and output

Implemented core foundation:

- owned-capable event-port schema/keys and dense indices;
- runtime-owned event schema and schema-drift checks;
- note IDs/channels/keys and wildcard-aware note addresses;
- semantic note on/off/choke/end;
- bounded borrowed note-event streams in process context/block;
- validation against event-port direction/note capability;
- zero-audio/event-schema runtime fixture;
- event-driven instrument fixture using output-only audio buffers;
- multi-output runtime fixture.

Before stable instrument/event claims:

- CLAP note-port projection/input translation preserving source order;
- note tuning and per-note expression;
- raw MIDI 1/SysEx;
- non-destructive MIDI 2/UMP representation;
- sample-accurate parameter modulation;
- bounded output-event sinks and note-end output;
- typed span/change-boundary processing without fabricated cross-family total order;
- high-event-count allocation/work evidence;
- event passthrough/transform and fuller synth fixtures through deployment adapters.

## DSP utility layer

Common DSP infrastructure is explicit Chassis scope, but the package/API boundary is not frozen.

Questions to resolve through concrete use:

- which utilities are generic enough to belong in Chassis versus a product;
- which mature dependencies should be exposed directly versus wrapped behind Chassis semantics;
- whether large optional dependencies justify a separate DSP/integration crate;
- consistent realtime/offline behavior for smoothing, metering, filters, FFT/STFT, oversampling, resampling, convolution, and buffer/channel helpers;
- performance/property/reference tests for promoted primitives.

Do not create a DSP crate merely to mirror a domain taxonomy, and do not reimplement mature primitives without a concrete reason.

## Editor/UI integration

Chassis owns editor lifecycle/integration, not visual design.

Before a stable editor claim:

- parent/native-window attach/detach;
- resize/scale/high-DPI;
- recreation/teardown;
- parameter observation and gestures;
- telemetry observation;
- focus/input semantics required by supported deployments;
- accessibility path where practical;
- optional GUI toolkit adapters without dependency leakage into headless users.

## Plugin deployment

CLAP is the first qualified plugin deployment. Existing headless and recorded REAPER evidence remains valuable.

Already true:

- CLAP audio/parameter setup derives from the coherent component schema;
- CLAP audio mapping is checked against schema-owned audio policy;
- CLAP state uses schema-owned semantic state identity;
- f32/f64 processing, latency, restart, active save, panic containment, re-entrancy, and allocation/work bounds have headless/in-process coverage.

Before stable plugin-format claims:

- replace/generalize proof-target adapter surfaces such as `ClapStereoEffect` when adding non-effect CLAP deployment makes the concrete requirement clear;
- complete CLAP event/note projection and output-event support;
- VST3/AU native validators;
- identity/state/automation/modulation/audio/event/latency/editor differential fixtures;
- real-host save/reopen/render scenarios for each supported format;
- native adapters only when concrete wrapper limitations justify them.

AAX/LV2 remain additional deployment targets rather than core semantic dependencies.

## Devices and standalone

Devices/standalone are planned framework scope.

Before stable claims, prove:

- audio device enumeration/open/close/reconfiguration;
- audio input/output/duplex and sample-rate/block-size negotiation;
- MIDI device input/output and timestamps where available;
- xrun/device-loss/error/recovery behavior;
- reuse of the same component/runtime/editor/state implementation;
- reviewed backend dependency boundaries.

## Graph/routing/scheduling

`chassis-graph` is planned framework infrastructure above the semantic core.

Before a stable graph claim, prove:

- ordinary Chassis processors as nodes;
- audio/event fan-in/fan-out and routing;
- validated immutable/transactional execution plans;
- latency propagation/compensation;
- realtime-safe bounded execution;
- topology/resource changes away from the callback;
- deterministic offline execution;
- parallel scheduling only where measurements justify it.

Tracks, clips, arrangements, project workflows, mixer UX, and DAW document semantics remain outside Chassis.

## Plugin hosting

Plugin hosting is planned framework scope for DAWs, hosts, test tools, and larger audio applications.

Before stable hosting claims, define/prove:

- discovery/scanning and capability metadata;
- loading/instantiation/lifecycle;
- graph/runtime processing integration;
- parameter/state/automation access;
- editor hosting;
- failure/crash policy at the supported isolation level;
- format adapters without leaking plugin ABI types into core.

Sandboxing/out-of-process hosting remains later until required.

## Media, transport, and offline application infrastructure

General audio applications need reusable media/offline infrastructure, but Chassis does not own a DAW project model.

Open work:

- source/sink metadata and stream/read/write/seek abstractions;
- codec integration through reviewed libraries;
- resampling/channel adaptation integration;
- deterministic component/graph offline rendering;
- reusable application transport/time primitives that drive processors/graphs;
- a random-access/multi-pass processing abstraction if sequential block processing cannot express real requirements cleanly.

Project media libraries, clip editing, arrangements, undo/workflow semantics, and content management remain application responsibilities.

## Validation breadth

Maintain separate evidence for:

- Rust/API correctness;
- realtime allocation/work bounds;
- concurrency/lifecycle correctness;
- state/identity compatibility;
- format conformance;
- device/graph/host correctness as those layers appear;
- deterministic offline behavior;
- representative performance;
- real-host/device support claims.

Do not convert existing tests into compatibility anchors for an obsolete pre-alpha API. Migrate useful fixtures to the target architecture and delete obsolete compatibility-only coverage.

## Governance and release

Before accepting substantive outside code or offering a commercial license:

- establish contributor/relicensing terms;
- define commercial license terms;
- verify notices/attribution generation;
- re-check dependency compatibility with AGPL and proprietary licensing.

Before a stable/public compatibility promise:

- complete the supported framework layers in `docs/roadmap.md` to the declared release scope;
- establish curated facade, packaging, validation, examples, and reference docs;
- define semver/MSRV/state compatibility policies;
- exercise the final supported API through the actual deployment/device/graph/host layers rather than assuming core proofs alone guarantee every outer contract.
