# Current Execution Plan

Finish and qualify the initial DSP core before building higher layers. [Architecture](architecture.md) owns the scope; [roadmap](roadmap.md#core-stability-gates) owns the stability gates and deferred extension decisions; [validation](design/validation.md) owns executed evidence. This plan changes sequencing, not current implementation or support claims. APIs and CHSS bytes remain pre-alpha.

## Baseline

Retain the `ComponentSchema` / `InstanceRuntime<P>` / `Processor` ownership split and the runtime-versus-CLAP package boundary. Typed handles, scoped channel views, coherent complete-state publication, native notes/expression, opaque MIDI transport, block-start transport, and activation-scoped latency are implemented. State-validity closure remains open in step 1 below. Their current limitations remain in the matching [design contracts](design/runtime.md); completed qualification belongs to the validation guide rather than a historical completion log here.

Current custom-state replacement uses deferred reactivation and withholds controls from old resources. Compatible prepared adoption and a general background-work lifecycle are not implemented. The [pinned Clack audit](research/clack-pinned-adapter-audit.md) remains authoritative; this plan does not authorize a dependency refresh.

## 1. Close state/control validity first

Owner: [parameters and state](design/parameters-state.md#state-validity-closure).

The resource-backed table fixture exposes a reproducible contract gap: an in-domain direct parameter edit or successful automated block can produce an encoded state that the same component rejects on reload and requested reactivation. Descriptor-domain validation and component cross-field validation do not currently preserve the same set of reachable states.

- Add committed regressions for both ordinary edits and automation followed by save/reopen and reactivation; confirm the current failure before changing behavior.
- Separate semantic validity from current-resource compatibility. Define which values may be independently automated and which coupled changes require complete validated replacement.
- Make accepted durable state reloadable under the same schema/policy. Keep desired state, consumed controls and applied resources explicit; do not hide the problem with save-time repair, callback validation of arbitrary product code, or silent clamping.
- Migrate core and CLAP callers together. Preserve failure atomicity, held external modulation, and off-callback ownership/reclamation.

The invariant is accepted; its API/enforcement design must be resolved against the regressions before implementation. Do not mark the existing complete-state surface closed merely because loads themselves validate.

## 2. Finish portable control and processing semantics

Owners: [parameters/state](design/parameters-state.md#trajectory-portability-before-stabilization), [events/transport](design/events-transport.md), and [authoring](design/authoring-api.md).

- Specify trajectory interpolation domains and block-boundary anchors. VST3's normalized linear queues and implicit sample -1 anchor cannot generally be projected as the current sample-zero plain-unit ramps under nonlinear mapping. Use a bounded reference fixture; choose faithful prepared evaluation or bounded expansion before freezing the representation. A VST3 adapter is not required for this proof.
- Add typed begin/change/end gestures, origin-aware control publication, observation and echo suppression through a headless control fixture. Include stopped processing, host-originated edits, and state/resource transitions. A GUI toolkit or native window is not required.
- Review the supported event-targeting, frame/layout, reset and process-failure contracts. Keep unsupported per-note parameter modulation and timestamped transport explicit instead of expanding all event families as a closure requirement.
- Declare overload outcomes for essential events, replaceable controls, and observations. Keep rejection bounded and observable without realtime logging or hidden allocation.

## 3. Close prepared-resource ownership

Owner: [runtime services](design/runtime-services.md#control---realtime-publication).

Use an in-repository IR/table/wavetable-like workload; no external product port is required.

- Prepare compatible replacements away from the callback and adopt them with bounded work. Keep reactivation for changes that cannot preserve the accepted topology/latency/resource contract.
- Prove ownership of active, pending, superseded and retired data, including full-capacity publication and delayed consumers.
- Reject stale completions across load/reset/deactivation/destruction; define cancellation and module-unload fencing even when workers complete late.
- Reclaim retired data off-thread and make offline preparation explicit and repeatable.

Choose the publication/lifetime mechanism from that proof. Executor policy remains separate: no hidden process-global pool, generic task framework, disk streamer, or host thread-pool integration is required merely to complete resource adoption. A caller-owned worker can exercise the lifecycle.

## 4. Consolidate authoring and bounded costs

Owners: [authoring](design/authoring-api.md), [buffers](design/process-buffers.md), and the resource/control contracts above.

- Exercise an effect with linked channels/sidechain, an instrument, an event-only processor, and the resource-backed workload through public APIs. Include direct embedding/composition without building a graph subsystem.
- Compare realistic channel/parameter/event workloads, including the raw alias boundary and independent trajectory cursors. Evaluate span/segment access or indexing only where the workload demonstrates an ergonomic or execution-cost problem.
- Add a small helper where it removes repeated DSP plumbing without hiding costs. Kernels such as ramps or meters do not need a component schema; a filter/oscillator/FFT catalog, graph DSL, macros, and an umbrella crate are not completion gates.
- Remove superseded paths, tests and documentation as interfaces change. Pre-alpha compatibility shims are unnecessary.

## 5. Qualify and review the declared core

Complete the [core stability gates](roadmap.md#core-stability-gates), not a broad application-framework checklist:

- run current Rust/dependency gates and focused allocation, concurrency, failure and unsafe-boundary checks;
- after adapter changes, rebuild/package all exported fixtures and run the pinned validator plus bounded fuzz;
- refresh real-host automation, save/reopen, active save, restart and PDC for the supported baseline; historical REAPER evidence does not qualify changed artifacts;
- record representative performance with exact workloads and conditions, without turning microbenchmarks into cross-framework or production guarantees;
- review compatibility fixtures, supported capabilities/platforms, compiler policy and the separate API/state-format commitments before any stability claim.

Only then select a narrow extension from the roadmap. Do not start editor rendering, devices/standalone, a graph scheduler, hosting, media pipelines, or another production format as a substitute for closing these gates. Existing CLAP integration feedback continues throughout core work.

No product migration, version milestone, publication, or deployment is implied by this plan.
