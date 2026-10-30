# Architecture

## Scope

Chassis is a Rust framework for reusable audio infrastructure. Plugins are its first deployment path, not its core abstraction. Effects, instruments, event processors, embedded components, and offline processing use the same component/runtime model. Devices, graphs, hosting, editors, media, and standalone deployment are intended outer layers, not currently implemented capabilities.

Applications own product algorithms, visual design, documents, and workflows. Chassis may supply a DAW's audio engine, routing, devices, hosted plugins, and rendering without defining tracks, clips, arrangements, projects, undo models, or mixer UX. Specialized restoration/ML algorithms and instrument engines remain clients unless repeated use establishes a reusable primitive.

Use mature libraries for codecs, FFTs, resampling, device backends, and toolkits when they fit the safety, semantics, maintenance, and license requirements. Owning an integration does not require reimplementing its algorithms.

## Layers and packages

```text
Products / applications
        |
        +-- deployment and editor integration
        +-- device / graph / hosting / media infrastructure [planned]
        |
        v
Format-independent component runtime
        |
        v
Lightweight utilities / reviewed dependencies
```

These are logical responsibilities, not a prescribed crate graph. The current production packages are `chassis-runtime` and `chassis-clap`; [package boundaries](design/crate-boundaries.md) explains the actual split. New boundaries require dependency, safety, platform, optionality, or independent-use pressure.

The CLAP adapter uses pinned Clack as its ABI/lifetime trust boundary. VST3/AU may initially use a reviewed wrapper; native implementations are justified by concrete correctness/capability/lifecycle limits. A wrapper target is not supported merely because it builds.

Keep format, toolkit, device, codec, and OS concepts at their integration boundaries. Do not wrap a dependency merely to hide its name; wrap where Chassis must own stronger semantic or lifecycle guarantees.

## Ownership

| Owner | Authority |
| --- | --- |
| `Component` | Immutable schema, complete-state defaults/validation/migrations, and activation factory |
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

Keep durable/base values, automation trajectories, modulation, and effective DSP values distinct. Explicit host ramps are not automatically smoothed. Opted-in global float modulation adds a held plain-unit delta and clamps only the effective value; reset, reactivation, and state load preserve that external signal, while save excludes it. Timed streams preserve offsets and the ordering the source actually supplies; independent streams do not acquire a causal total order merely from equal timestamps.

State load borrows component-owned policy and decodes, migrates, and validates a temporary complete candidate before publication. Typed documents are valid at construction, including migration output. Accepted active custom-state changes request reactivation. Once observed at the process boundary, old resources retain their base/modulation projection and timed controls are checked but withheld until reactivation; desired external modulation still advances independently of saved state; an already-running old-generation block may finish without overwriting desired state. Desired complete state remains saveable while old resources continue processing. Active save captures one completed semantic generation. Newer control/state publication defeats stale realtime completion. Processor history is not persisted merely because it is serializable.

Core note/event capabilities are broader than current CLAP projection. Each deployment must map them faithfully or reject unsupported use. Do not silently discard custom state, note identity, modulation, or labeled layout semantics. CHSS bytes and public APIs remain provisional.

Detailed owners: [parameters/state](design/parameters-state.md), [state format](design/state-format.md), [events/transport](design/events-transport.md), and [unresolved decisions](design/open-questions.md).

## Realtime and failure boundaries

Deterministic callbacks have activation-defined resource/work bounds: no heap allocation/deallocation, blocking I/O, contended/unbounded locks, or unbounded queues/retries. Prepare topology/resources away from the callback, publish accepted replacements transactionally, and reclaim replaced large objects off-thread.

Unused editor, worker, telemetry, graph, or device facilities impose no hidden process-time work. Do not add a lock-free framework, unsafe fast paths, custom allocation, SIMD, cache padding, or `no_std` without a concrete requirement and evidence.

`chassis-runtime` forbids unsafe code. Necessary adapter unsafe code must have narrow explicit validity/alias/lifetime/threading invariants and matching verification. Untrusted bytes, host/device metadata, and FFI inputs are validated at boundaries; impossible internal states are bugs, not occasions for silent success-shaped fallback.

Panics never unwind through foreign ABI callbacks. Pinned Clack contains activation/process panics; failed activation permits reuse, but process failure terminates the run. Arbitrary DSP state is not promised reusable after a process panic.

Prepared-resource publication, reclamation, tasks, and telemetry are specified in [runtime services](design/runtime-services.md).

## Qualification and evolution

Separate Rust correctness, realtime bounds, format conformance, state/identity compatibility, performance, and production-host support. Historical baselines do not qualify later revisions. [Validation](design/validation.md) owns exact evidence; [execution plan](next-runtime-slice.md) owns immediate sequencing; [roadmap](roadmap.md) owns remaining framework scope.

Development tracks stable Rust and Edition 2024 without a declared MSRV promise. CI's explicit compiler is a validation baseline. Dependency/source changes follow [dependency policy](dependencies.md) and the AGPL/commercial distribution constraints in [licensing](licensing.md).
