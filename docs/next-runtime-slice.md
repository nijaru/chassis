# Current Execution Plan

Finish and qualify the initial DSP core before building higher layers. [Architecture](architecture.md) owns the scope; [roadmap](roadmap.md#core-stability-gates) owns the stability gates and deferred extension decisions; [validation](design/validation.md) owns executed evidence. This plan changes sequencing, not current implementation or support claims. APIs and CHSS bytes remain pre-alpha.

## Baseline

Retain the `ComponentSchema` / `InstanceRuntime<P>` / `Processor` ownership split and the runtime-versus-CLAP package boundary. Typed handles, scoped channel views, coherent complete-state publication, native notes/expression, opaque MIDI transport, block-start transport, and activation-scoped latency are implemented. The [fixed-resource subset boundary](design/validation.md#fixed-resource-subset-boundary) and [M1 exit evidence](design/validation.md#fixed-resource-execution-costs-and-m1-exit--2026-11-12) define the qualified baseline and its limits. Their current limitations remain in the matching [design contracts](design/runtime.md); completed qualification belongs to the validation guide rather than a historical completion log here.

Current custom-state replacement uses deferred reactivation and withholds controls from old resources. Compatible prepared adoption and a general background-work lifecycle are not implemented. The [pinned Clack audit](research/clack-pinned-adapter-audit.md) remains authoritative; this plan does not authorize a dependency refresh.

## Milestone mapping

[The roadmap](roadmap.md#usable-milestones-within-phase-1) owns each milestone's usable outcome, required slices, acceptance criteria and exclusions. The sections below order the implementation work that reaches those outcomes:

| Milestone | Work in this plan |
| --- | --- |
| M1 — dependable fixed-resource DSP | Step 1, plus the relevant authoring/cost checks in step 4 |
| M2 — complete control handling | Step 2, retaining M1 qualification and extending step 4's workload checks |
| M3 — resource-backed DSP | Step 3, preserving M1/M2 guarantees and applying step 4 to replacement workloads |
| M4 — qualified core baseline | Step 5 and the integrated consolidation from step 4 |

The current target is M3's prepared-resource ownership in step 3. [M2's bounded usable exit](design/validation.md#bounded-clap-product-controls-and-m2-exit--2026-11-13) qualifies prepared mapping, scalar provenance, compact timed trajectories, headless controls and optional instance-lifetime CLAP delivery while retaining M1. Preserve those semantics and declared limits during resource work; helper/model success alone is not end-to-end qualification. Do not wait for M4 to make earlier subsets runnable, testable and documented. Milestone completion does not freeze the pre-alpha API or authorize a version/release.

Implementation planning selects concrete API proposals, test structure, module/file organization and task order within these slices. Resolve consequential semantic choices against the owning contracts and evidence before coding; do not redefine milestone scope, exclusions or acceptance criteria as an implementation convenience. No per-file task catalog or speculative API shape is prescribed here.

## 1. Close state/control validity first

Owners: [parameters and state](design/parameters-state.md#state-validity-closure), [runtime lifecycle](design/runtime.md), and [events/transport](design/events-transport.md).

Preserve the qualified M1 foundation while extending controls and resources:

- Parameter descriptors define independently valid domains; `Component::validate_custom_state` checks custom fields and their relationships. Coupled values belong together in custom state. Preserve complete save/reopen, failure-atomic rejection, frozen consumed generations and desired-versus-applied modulation across replacement; [state validity](design/parameters-state.md#state-validity-closure) owns these rules.
- Keep fixed deployment state-budget admission valid for every admitted parameter combination, not defaults alone. Preserve event/count/byte attempt limits, activation/reset/process failure transitions and off-callback reclamation under the [subset boundary](design/validation.md#fixed-resource-subset-boundary).
- Retain the [Clack stream-ownership correction](research/clack-pinned-adapter-audit.md#state-stream-ownership--reviewed-2026-11-12) and its regressions; mutable I/O must not claim ownership of const host descriptors.

Step 4's useful-DSP comparisons now cover bare kernels, direct runtime and CLAP, including changing multi-parameter base/modulation, event density, focused topology/exact-alias cases and independent versus shared evaluation. Keep their output-equivalence and steady-state-variation preflights when extending the workloads. The [cost evidence](design/validation.md#realtime-allocation-and-performance) supports shared evaluation for common linked-channel controls, not a new helper/indexing requirement or a callback-tail guarantee.

## 2. Finish portable control and processing semantics

Owners: [parameters/state](design/parameters-state.md#trajectory-portability-before-stabilization), [events/transport](design/events-transport.md), and [authoring](design/authoring-api.md).

- Preserve the implemented [timed source-domain contract](design/parameters-state.md#trajectory-portability-before-stabilization) while adding control publication: explicit reseeding after plain-origin edits/loads, ordered mixed-domain points, shared event budgets and successful-DSP/tail coordinate completion. Source adapters must supply their own faithful boundary clipping; dense reference expansion and inverse/text hooks are not an implicit callback path. A VST3 adapter is not required.
- Preserve the implemented [headless control protocol](design/parameters-state.md#product-originated-edits-and-modulation): typed gestures, explicit local/external ingress, latest observations, and stale-but-closable tokens after state load. Its caller-owned synchronous effects are not a shared endpoint or backend delivery receipt. A GUI toolkit or native window is not required.
- Qualify the implemented [optional CLAP controller](design/parameters-state.md#bounded-clap-product-controls) through its [public-entry fixture](../crates/chassis-clap/tests/host_product_controls.rs): one instance-lifetime gesture owner, coherent desired plain/provenance pairs, active/inactive flush and process delivery, stopped processing and pending reactivation. Preserve next-block-only synchronization; no author mapping belongs after DSP/tail completion.
- Retain the implemented failure/capacity policy: reserve essential delivery/closure before acceptance, preserve refused suffixes, suppress only superseded Values, and retain Begin/End debt. Exercise ordinary concurrent load handoff separately from nested reentrant Busy/no-I/O rejection, deferred service after failed/panicking loads, controller disable/closure and empty-queue fault service. Preserve [versioned ingress](design/parameters-state.md#complete-clap-state-publication) independently of output claims and target completion only at touched base parameters. No audio waits, hidden allocation, or silent success-shaped loss is acceptable. Keep native heap checks, finite Loom models, Miri and packaged format qualification separate.
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
