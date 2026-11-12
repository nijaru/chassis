# Architecture

## Scope

Chassis is a Rust DSP framework for building audio software. The initial target is a complete, correct, and stable component execution core: audio buffers and topology, timed controls/events, semantic state, activation, resource replacement, and bounded realtime/control communication. Plugins are its first deployment path, not its core abstraction. Effects, instruments, event processors, direct Rust embedding, and sequential offline processing exercise the same runtime.

Here, the core DSP engine means component execution and its shared lifecycle/resource machinery, not a device engine, multi-node graph, plugin host, or DAW. Stabilize that bounded surface before expanding into higher layers. Existing CLAP deployment remains a maintained integration and qualification boundary; a full editor, standalone application, or second plugin format is not a prerequisite for core stability.

Devices, graphs, hosting, editor/toolkit integration, media pipelines, and standalone bootstrap are possible later extensions, not accepted maintenance commitments. Audio-software categories inform core requirements without making every facility those products use part of Chassis. Each extension needs a separate scope, dependency, qualification, and maintenance decision after the core gates in [the roadmap](roadmap.md#core-stability-gates) are met.

Applications own product algorithms, visual design, documents, and workflows. A DAW can embed Chassis processors without Chassis defining tracks, clips, arrangements, projects, undo, or mixer UX. Chassis is audio-first: bounded observations belong to the core; visualizers and editor adapters may build on them, but a widget toolkit, rendering engine, video pipeline, and audiovisual composition model do not.

Keep DSP kernels lightweight: a filter, ramp, or meter does not need a component schema or persistence identity. Add a small reusable helper where an established workload justifies its semantics and maintenance, not a comprehensive DSP catalog. Use mature libraries for substantial algorithms and optional integrations when their safety, numerical behavior, dependencies, and licenses fit; owning a contract does not require reimplementing its backend.

## Layers and packages

```text
Products / applications
        |
        +-- deployment and editor integration
        +-- device / graph / hosting / media infrastructure [possible later]
        |
        v
Format-independent component runtime
        |
        v
Lightweight utilities / reviewed dependencies
```

The upper layers are extension boundaries, not a promise to implement them. These are logical responsibilities, not a prescribed crate graph. The current production packages are `chassis-runtime` and `chassis-clap`; [package boundaries](design/crate-boundaries.md) explains the actual split. New boundaries require dependency, safety, platform, optionality, or independent-use pressure.

The CLAP adapter uses pinned Clack as its ABI/lifetime trust boundary. If another format is selected later, compare a reviewed wrapper with a native adapter against the supported semantics and lifecycle. A wrapper target is not supported merely because it builds; source-contract probes can test core assumptions before another adapter exists.

Keep format, toolkit, device, codec, and OS concepts at their integration boundaries. Do not wrap a dependency merely to hide its name; wrap where Chassis must own stronger semantic or lifecycle guarantees.

## Ownership

| Owner | Authority |
| --- | --- |
| `Component` | Immutable schema, custom-state defaults/validation, state migrations, and activation factory |
| `ComponentSchema` | Audio/event/parameter metadata, whole-audio-I/O policy, semantic state identity, stable-to-dense lookup |
| `InstanceRuntime<P>` | Validated schema generation, durable parameter/custom state, held external modulation, accepted activation, lifecycle coordination |
| `Processor` | Exclusive active DSP history and prepared resources |
| Deployment/application layer | Host/device/editor/graph orchestration and faithful projections |
| Shared handles | Explicit synchronized observations/publications, not independent semantic state |

Runtime-owned metadata is allowed at setup. Callback identity is dense and schema-local; stable keys, labels, declaration order, Rust names, and backend IDs are not interchangeable. Semantic persistence identity is independent of export identity.

The runtime/schema ownership model is exercised by heterogeneous fixtures. That is evidence against speculative abstraction growth, not a prohibition on refactoring when a concrete client reveals a mismatch. Pre-alpha interfaces are replaceable; maintain no compatibility shims solely because an earlier proof used them.

## Activation and processing

Accept and resolve one whole I/O configuration before creating active resources. Reject structural, policy, or schema drift before activation publishes a processor. Instrument output-only, event-only, multiple buses, sidechain, and different legal layouts are ordinary uses of the same model.

Latency is established and snapshotted for one activation. A resource/latency change requests restart/reactivation; it does not make new latency authoritative mid-run. The current resources remain valid until the environment completes the lifecycle transition. Tail is a separate completed bound on potential post-input output, not a latency or activity counter; [runtime services](design/runtime-services.md) owns tail and explicit bypass semantics.

Safe buffers preserve exact in-place, disjoint, input-only, and output-only relationships. Every callback covers the accepted endpoint set uniquely and completely. Adapters classify whole-topology aliases before constructing sample references, while host allocation/provenance/concurrency obligations remain explicit. Illegal partial or cross-channel writable overlap is a boundary failure. Bounded copies are acceptable where they simplify ownership; zero-copy is not a goal by itself.

Process modes distinguish realtime, buffered realtime, and offline work. Offline can choose a more expensive deterministic algorithm, but must not depend on wall-clock task races. Random-access or multi-pass workflows may need a separate contract rather than distorting block processing.

Detailed owners: [runtime](design/runtime.md), [component schema](design/component-schema.md), [audio I/O](design/audio-ports.md), and [buffers](design/process-buffers.md).

## Parameters, events, and state

Keep durable/base values, automation trajectories, modulation, and effective DSP values distinct. Explicit host ramps are not automatically smoothed. Opted-in global float modulation adds a held plain-unit delta and clamps only the effective value; reset, reactivation, and state load preserve that external signal, while save excludes it. Timed streams preserve offsets and the ordering the source actually supplies; independent streams do not acquire a causal total order merely from equal timestamps. Borrowed block-start transport retains independently available musical/sample/second timelines, meter/bar/loop information, pre-roll, and source tempo slope without inventing clocks or an intra-block transport stream.

Parameter domains are independently valid with every admitted custom state; dependent structural values belong together in custom state. [State validity](design/parameters-state.md#state-validity-closure) owns that boundary. State load borrows component-owned custom policy and decodes, migrates, and validates a temporary complete candidate before publication. Typed documents are valid at construction, including migration output. Accepted active custom-state changes request reactivation. Once observed at the process boundary, old resources retain their base/modulation projection and timed controls are checked but withheld until reactivation; desired external modulation still advances independently of saved state; an already-running old-generation block may finish without overwriting desired state. Desired complete state remains saveable while old resources continue processing. Active save captures one completed semantic generation. Newer control/state publication defeats stale realtime completion. Processor history is not persisted merely because it is serializable.

Core and CLAP support native notes/expression, opaque MIDI 1/SysEx, and opaque four-word MIDI 2/UMP envelopes, with bounded shared output and native voice-lifetime feedback. UMP transport preserves numeric words without protocol parsing or conversion. Each deployment must preserve source ordering and map capabilities faithfully or reject unsupported use. Do not silently discard custom state, note identity, modulation, or labeled layout semantics. CHSS bytes and public APIs remain provisional.

Detailed owners: [parameters/state](design/parameters-state.md), [state format](design/state-format.md), [events/transport](design/events-transport.md), and [unresolved decisions](design/open-questions.md).

## Realtime and failure boundaries

Deterministic callbacks have activation-defined resource/work bounds: no heap allocation/deallocation, blocking I/O, contended/unbounded locks, or unbounded queues/retries. Prepare topology/resources away from the callback, publish accepted replacements transactionally, and reclaim replaced large objects off-thread.

Unused editor, worker, telemetry, graph, or device facilities impose no hidden process-time work. Do not add a lock-free framework, unsafe fast paths, custom allocation, SIMD, cache padding, or `no_std` without a concrete requirement and evidence.

`chassis-runtime` forbids unsafe code. Necessary adapter unsafe code must have narrow explicit validity/alias/lifetime/threading invariants and matching verification. Untrusted bytes, host/device metadata, and FFI inputs are validated at boundaries; impossible internal states are bugs, not occasions for silent success-shaped fallback.

Panics never unwind through foreign ABI callbacks. Pinned Clack contains activation/process/reset panics; failed activation permits reuse, but process failure terminates the run. A CLAP reset panic disables further DSP, reset, and processing starts until deactivation and fresh activation. Arbitrary DSP state is not promised reusable after a process or reset panic.

Prepared-resource publication, reclamation, tasks, and telemetry are specified in [runtime services](design/runtime-services.md).

## Qualification and evolution

Separate Rust correctness, realtime bounds, format conformance, state/identity compatibility, performance, and production-host support. Historical baselines do not qualify later revisions. [Validation](design/validation.md) owns exact evidence; [execution plan](next-runtime-slice.md) owns immediate sequencing; [roadmap](roadmap.md) owns the core-stability gates and the boundary between committed work and possible extensions. No stability claim, version milestone, or release follows automatically from completing a phase.

Development tracks stable Rust and Edition 2024 without a declared MSRV promise. CI's explicit compiler is a validation baseline. Dependency/source changes follow [dependency policy](dependencies.md) and the AGPL/commercial distribution constraints in [licensing](licensing.md).
