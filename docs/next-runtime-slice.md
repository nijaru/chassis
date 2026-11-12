# Current Execution Plan

Finish and qualify the initial DSP core before building higher layers. [Architecture](architecture.md) owns the scope; [roadmap](roadmap.md#core-stability-gates) owns the stability gates and deferred extension decisions; [validation](design/validation.md) owns executed evidence. This plan changes sequencing, not current implementation or support claims. APIs and CHSS bytes remain pre-alpha.

## Baseline

Retain the `ComponentSchema` / `InstanceRuntime<P>` / `Processor` ownership split and the runtime-versus-CLAP package boundary. Typed handles, scoped channel views, coherent complete-state publication, native notes/expression, opaque MIDI transport, block-start transport, and activation-scoped latency are implemented. State-validity closure uses independent descriptor domains and custom-only validation; the remaining M1 work is in step 1 below. Their current limitations remain in the matching [design contracts](design/runtime.md); completed qualification belongs to the validation guide rather than a historical completion log here.

Current custom-state replacement uses deferred reactivation and withholds controls from old resources. Compatible prepared adoption and a general background-work lifecycle are not implemented. The [pinned Clack audit](research/clack-pinned-adapter-audit.md) remains authoritative; this plan does not authorize a dependency refresh.

## Milestone mapping

[The roadmap](roadmap.md#usable-milestones-within-phase-1) owns each milestone's usable outcome, required slices, acceptance criteria and exclusions. The sections below order the implementation work that reaches those outcomes:

| Milestone | Work in this plan |
| --- | --- |
| M1 — dependable fixed-resource DSP | Step 1, plus the relevant authoring/cost checks in step 4 |
| M2 — complete control handling | Step 2, retaining M1 qualification and extending step 4's workload checks |
| M3 — resource-backed DSP | Step 3, preserving M1/M2 guarantees and applying step 4 to replacement workloads |
| M4 — qualified core baseline | Step 5 and the integrated consolidation from step 4 |

The current target is M1, continuing with lifecycle/failure and capacity qualification below. Do not wait for M4 to make earlier subsets runnable, testable and documented. Milestone completion does not freeze the pre-alpha API or authorize a version/release.

Implementation planning selects concrete API proposals, test structure, module/file organization and task order within these slices. Resolve consequential semantic choices against the owning contracts and evidence before coding; do not redefine milestone scope, exclusions or acceptance criteria as an implementation convenience. No per-file task catalog or speculative API shape is prescribed here.

## 1. Close state/control validity first

Owners: [parameters and state](design/parameters-state.md#state-validity-closure), [runtime lifecycle](design/runtime.md), and [events/transport](design/events-transport.md).

The state-validity slice is implemented: parameter descriptors define independent domains, while `Component::validate_custom_state` checks custom fields and their relationships. Coupled values that cannot be independently valid belong together in custom state. Direct-edit, successful-automation, and actual-entry CLAP flush/save/reopen regressions reproduced the former rejection before the API migration. [The contract](design/parameters-state.md#state-validity-closure) owns the enforced boundary and remaining resource distinction; [validation](design/validation.md) owns qualification.

CLAP now admits defaults and post-migration custom candidates against the fixed save budget for every descriptor-valid parameter combination. Supported output SysEx attempts consume byte capacity even when oversized. A contained CLAP reset panic disables that activation until deactivation rather than allowing damaged DSP to resume. Their contracts live with [state admission](design/state-format.md#adapter-behavior), [output events](design/events-transport.md#output-events), and [runtime failures](design/runtime.md#failure-rules).

Remaining M1 work:

- Qualify activation latency/tail hook panics and DSP panics with pending base/modulation events. Protect no-partial-child/metadata publication, no failed-block endpoint completion, durable desired state, and teardown without resuming damaged DSP.
- Consolidate supported event-targeting, frame/layout, failure and capacity outcomes in the fixed-resource subset proof. Include essential events, controls and observations; keep rejection bounded and observable without realtime logging or hidden allocation.
- Complete the fixed-resource public-API subset proof and relevant authoring/cost checks from step 4. Preserve state failure atomicity, frozen generations, held external modulation, and off-callback ownership/reclamation.

Do not mark M1 complete from state closure alone or advance to broader control/resource work while these supported lifecycle and capacity outcomes remain unqualified.

## 2. Finish portable control and processing semantics

Owners: [parameters/state](design/parameters-state.md#trajectory-portability-before-stabilization), [events/transport](design/events-transport.md), and [authoring](design/authoring-api.md).

- Specify trajectory interpolation domains and block-boundary anchors. VST3's normalized linear queues and implicit sample -1 anchor cannot generally be projected as the current sample-zero plain-unit ramps under nonlinear mapping. Use a bounded reference fixture; choose faithful prepared evaluation or bounded expansion before freezing the representation. A VST3 adapter is not required for this proof.
- Add typed begin/change/end gestures, origin-aware control publication, observation and echo suppression through a headless control fixture. Include stopped processing, host-originated edits, and state/resource transitions. A GUI toolkit or native window is not required.
- Extend M1's failure/capacity policies to control publication, including contention, stopped processing and concurrent state changes. Preserve explicit precedence and observable outcomes without callback waits or hidden allocation.
- Keep unsupported per-note parameter modulation and timestamped transport explicit instead of expanding all event families as a closure requirement.

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

Apply these checks to each M1–M3 slice, then consolidate across the complete supported subset in M4. Authoring usability and bounded execution costs are not postponed until final qualification.

- Exercise an effect with linked channels/sidechain, an instrument, an event-only processor, and the resource-backed workload through public APIs. Include direct embedding/composition without building a graph subsystem.
- Compare realistic channel/parameter/event workloads, including the raw alias boundary and independent trajectory cursors. Evaluate span/segment access or indexing only where the workload demonstrates an ergonomic or execution-cost problem.
- Add a small helper where it removes repeated DSP plumbing without hiding costs. Kernels such as ramps or meters do not need a component schema; a filter/oscillator/FFT catalog, graph DSL, macros, and an umbrella crate are not completion gates.
- Remove superseded paths, tests and documentation as interfaces change. Pre-alpha compatibility shims are unnecessary.

## 5. Qualify and review the declared core

M4 consolidates milestone evidence and completes the [core stability gates](roadmap.md#core-stability-gates), not a broad application-framework checklist. Earlier milestones must already carry the checks relevant to their supported behavior; record missing evidence as a blocker rather than implying qualification:

- run current Rust/dependency gates and focused allocation, concurrency, failure and unsafe-boundary checks;
- after adapter changes, rebuild/package all exported fixtures and run the pinned validator plus bounded fuzz;
- refresh real-host automation, save/reopen, active save, restart and PDC for the supported baseline; historical REAPER evidence does not qualify changed artifacts;
- record representative performance with exact workloads and conditions, without turning microbenchmarks into cross-framework or production guarantees;
- review compatibility fixtures, supported capabilities/platforms, compiler policy and the separate API/state-format commitments before any stability claim.

Only then select a narrow extension from the roadmap. Do not start editor rendering, devices/standalone, a graph scheduler, hosting, media pipelines, or another production format as a substitute for closing these gates. Existing CLAP integration feedback continues throughout core work.

No product migration, version milestone, publication, or deployment is implied by this plan.
