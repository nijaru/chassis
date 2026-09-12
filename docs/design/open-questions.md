# Open Questions and API Freeze Gates

This file records unresolved design decisions that still block API or stability claims. Completed proof steps belong in `docs/design/validation.md`; execution order belongs in `docs/next-runtime-slice.md`; the broad completion bar belongs in `docs/roadmap.md`.

Chassis is unpublished pre-alpha. Existing product migrations and current public APIs are evidence sources, not compatibility constraints.

## Component schema and runtime authority

The target direction is documented in `docs/design/component-schema.md`.

Implemented foundation:

- `Component` / `InstanceRuntime<P>` / `Processor` lifecycle ownership;
- validated typed parameter schema/store;
- stable event-port schema with runtime-owned dense indices;
- `AudioPortIndex` plus standalone audio-schema validation/dense lookup;
- initial owned `ComponentSchema` with semantic component/state identity, state-schema version, audio/event/parameter schemas, and dense port lookup.

Before freezing the core authoring surface:

- migrate `Component` from separate incremental schema methods to one coherent schema authority;
- make `InstanceRuntime` own/validate the complete immutable schema snapshot, including audio schema;
- remove proof-era constructors such as `new_with_event_ports` once the complete schema constructor is established;
- remove the core semantic default that makes every `Component` a conventional stereo effect; keep conventional effect helpers above neutral core semantics;
- migrate audio process endpoints from stable string keys to dense schema-local `AudioPortIndex` values;
- replace static-only audio/event stable identities where necessary so dynamically constructed/hosted components are representable without callback string work;
- separate semantic state identity/version from deployment/export identities such as CLAP/VST3/AU IDs;
- ensure schema drift is rejected before activation and dynamic configuration changes have explicit semantics.

Do not freeze macros/derives until this model is exercised by effects, event-only processors, instruments, multi-output components, embedded/graph use, and plugin deployment.

## Process buffers and I/O

The generic safe buffer-source shape is implemented and the CLAP adapter traverses setup-mapped channels without callback-owned channel collections.

Freeze gates:

- complete dense audio-port migration;
- semantic whole-I/O policy for components with multiple legal configurations;
- ergonomic bus/port views that preserve explicit buffer legality;
- target-specific legality for inactive/null/zero-buffer cases;
- benchmark controlled copy/conversion paths before adding unsafe/ownership complexity merely to remove them;
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
- sample-accurate modulation distinct from durable/base state publication;
- adapter-specific automation/modulation semantics must be preserved rather than normalized to a weaker invented model;
- production-host automated-render evidence for supported formats.

## State identity, save/load, and compatibility

Implemented CLAP consistency behavior:

- active state save serializes one coherent completed scalar generation;
- newer control/state generations win over stale realtime endpoint publication;
- state load is transactional and requests host value rescan after accepted publication;
- direct tests/Loom/in-process-host coverage exercise the current behavior.

Freeze gates:

- make semantic component/state identity and schema version runtime-owned through `ComponentSchema` rather than caller-supplied on every operation;
- derive/export deployment identities from explicit product/export metadata without conflating them with semantic state identity;
- migrate runtime save/load APIs to the canonical identity owner;
- cross-format state round trips;
- decide when the CHSS wire envelope becomes a compatibility promise;
- define compatibility/versioning policy before a stable release.

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

- stable event-port schema/keys and dense indices;
- runtime-owned event schema and schema-drift checks;
- note IDs/channels/keys and wildcard-aware note addresses;
- semantic note on/off/choke/end;
- bounded borrowed note-event streams in process context/block;
- validation against event-port direction/note capability;
- zero-audio/event-schema runtime fixtures.

Before stable instrument/event claims:

- CLAP note-port projection/input translation preserving source order;
- note tuning and per-note expression;
- raw MIDI 1/SysEx;
- non-destructive MIDI 2/UMP representation;
- sample-accurate parameter modulation;
- bounded output-event sinks and note-end output;
- typed span/change-boundary processing without fabricated cross-family total order;
- high-event-count allocation/work evidence;
- synth/event-transform/multi-output fixtures.

## DSP utility layer

Common DSP infrastructure is now explicit Chassis scope, but the package/API boundary is not frozen.

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

Before stable plugin-format claims:

- generalize proof-era CLAP concepts such as `ClapStereoEffect` after the neutral component-schema migration;
- derive audio/event/parameter projection from the complete component schema and explicit deployment metadata;
- VST3/AU native validators;
- identity/state/automation/modulation/audio/event/latency/editor differential fixtures;
- real-host save/reopen/render scenarios;
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
- exercise the final core API across materially different clients instead of one plugin class.
