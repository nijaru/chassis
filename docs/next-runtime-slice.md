# Current Execution Plan

This file owns immediate sequencing. [Roadmap](roadmap.md) owns deferred framework scope; [validation](design/validation.md) owns executed evidence. All APIs and CHSS bytes remain pre-alpha.

## Design decision after the boundary audit

Retain the format-independent `ComponentSchema` / `InstanceRuntime<P>` / `Processor` ownership split and the runtime-versus-CLAP package boundary. The heterogeneous clients and publication tests support those choices. The audit exposed weak admission and transition contracts, not a need for another lifecycle, facade, graph model, or I/O-policy language.

The implemented corrections are:

- complete, unique, activation-bounded callback audio topology rather than independent endpoint checks;
- adapter-owned raw alias classification before sample references, with host allocation/concurrency obligations explicit;
- dense parameter-local choice events rather than callback string validation;
- valid semantic documents at construction, including migration output;
- explicit reactivation requests for accepted active custom-state changes;
- faithful CLAP stepped values, discrete layouts, output metadata, choice labels, and exported reset behavior;
- explicit destructive input-to-output initialization rather than a misleading mutable accessor.

Their contracts live in [buffers](design/process-buffers.md), [parameters/state](design/parameters-state.md), [runtime](design/runtime.md), and the [Clack audit](research/clack-pinned-adapter-audit.md). They do not make the broader framework production-ready.

## Closed — realtime rejection and dense parameter projection

The existing Clack pin remains. A reviewed vendored `clack-plugin` patch bypasses reporting only for process callbacks and preserves actual `CLAP_PROCESS_ERROR`, lifecycle checks, and panic containment. Registry 0.2.0 has unchanged process reporting and is not a replacement for this fix. [Source audit](research/clack-pinned-adapter-audit.md) owns the delta; [validation](design/validation.md) owns revision-scoped evidence.

The roadmap review also found remaining stable-choice scans in CLAP synchronization, flush, and publication. The runtime store now owns a cached dense choice projection alongside canonical persistent identity; all callback projection uses fixed-width values. Setup/control/state paths still resolve stable identities. Arbitrary boxed errors and DSP panics remain outside realtime qualification.

## Closed — component-owned core state and deferred reactivation

`Component` owns custom defaults, complete-state validation, and adjacent migration registration. `for_component` is the only public runtime constructor; defaults are validated even without persistence identity. One `activate(config, parameters, custom_state)` hook replaces the historical hook chain, and loads borrow the component rather than accept per-call policy.

A resource-backed table fixture couples parameter addressing to custom data and processes before restart. The first accepted active custom-state change freezes the pre-load DSP parameter projection. Incoming base automation remains validated but is discarded, with no endpoint completion, until deactivation/reactivation. Later loads and base control edits affect saveable desired state only; admitted modulation still updates desired held input without reaching old DSP. Reset and state reversion cannot unfreeze the old resources. This deliberately sacrifices parameter automation while a host defers restart; retaining it would require a qualified resource-compatibility policy.

## Closed — complete CLAP state projection

CLAP now projects one coherent parameter/custom generation across shared/main-thread and active lifetimes. Component policy admits defaults, migrated loads, and activation snapshots; original defaults must also fit deployment wire bounds. Custom ownership stays off the callback. A resource epoch gates old DSP parameters/automation until reactivation, including reset, repeated loads, and reversion. A block already in flight may finish, but stale endpoint publication cannot overwrite desired state.

Actual-entry resource-backed tests exercise default save, migration, deferred restart, zero-allocation pending callbacks, rejection, save/reopen, and a load racing DSP completion. [Parameters/state](design/parameters-state.md#complete-clap-state-publication) owns the transition and contention contract; [validation](design/validation.md) owns revision-scoped qualification.

Start with reactivation-based resource replacement. Bounded prepared adoption, coalescing, stale-result rejection, deferred reclamation, cancellation, and unload fencing need a concrete client before choosing a queue or executor. No hidden global executor or last-owner `Arc` destruction on the callback.

## Closed — parameter authoring policy

Descriptors now own immutable normalized-control and formatting/parsing policy. Core and actual-entry tests cover nonlinear mapping, units/precision, extreme domains, discrete rounding, stable-choice round trips, configuration drift, and output-buffer rejection. CLAP delegates text without reinterpreting product syntax; its plain automation stays unchanged. A host's generic normalized knob cannot inherit an arbitrary framework taper through CLAP.

[Parameters/state](design/parameters-state.md#control-mapping-and-presentation) owns this contract; [validation](design/validation.md) owns revision-scoped evidence. No macros, new crate, or unit/curve catalog is needed.

## Closed — global additive float modulation

Explicitly opted-in floats now accept global held plain-unit amounts in the same bounded ordered stream as base automation. Cached effective cursors add/clamp without changing base or saved state. Held external signals survive reset, reactivation, and state loads. Custom transitions freeze both consumed base and modulation while retaining desired signals for reactivation; CLAP's separate transient projection preserves accepted modulation even when a newer state load defeats base completion.

Core and actual-entry fixtures exercise mixed trajectories, lifecycle/state exclusion, resource gating, malformed/targeted rejection, and allocation bounds. Per-note parameter and non-float modulation remain unsupported rather than flattened; native note expression is a separate capability. [Parameters/state](design/parameters-state.md#global-additive-float-modulation) owns the contract; [validation](design/validation.md) owns revision-scoped evidence.

## Closed — tail and bypass

Tail now reports a completed conservative bound independently of activation latency. CLAP queries use a shared snapshot, with audio-thread notifications and notification debt preserved across unstarted activations. Bypass is an explicit unique Boolean role in ordinary persistent, sample-timed controls; product DSP owns its transition and audio policy.

The core echo fixture preserves latency and existing tail; the exported gain fixture demonstrates timed unity-gain dry processing with subnormal cleanup in f32/f64, and the delayed export reports its actual delay extent. The boundary review also corrected read-only event-descriptor ownership and typed-event pointer provenance at the existing Clack pin. [Runtime services](design/runtime-services.md) owns the semantic contract; the [source audit](research/clack-pinned-adapter-audit.md) owns the FFI delta and [validation](design/validation.md) owns qualified revisions.

## Closed — complete native notes

The input-only slice was expanded to include native expression and bounded output so the exported instrument can report voice completion honestly. CLAP now projects explicit native note-port identities, preserves actual mixed parameter/note order, and supports all seven native expression dimensions. Caller-owned bounded output includes voice-end feedback to the original input port; essential rejection can terminate processing without completing parameter/modulation endpoints or tail metadata.

The output-only stereo note probe exercises two input ports with distinct host/schema ordering, f32/f64, strike controls captured at onset, expressions, and same-block retrigger lifetime feedback. The existing Clack pin gains narrowly audited expression layout/decoding corrections. The isolated validator also corrects out-of-contract expression ranges, with a regression that fails its original generator. [Events/transport](design/events-transport.md) owns semantics; the [source audit](research/clack-pinned-adapter-audit.md) and [validation](design/validation.md) own boundary review and revision-scoped evidence.

## Closed — MIDI 1/SysEx

Core and CLAP now preserve opaque three-byte messages, borrowed SysEx payload boundaries, concrete ports, defined live/do-not-record annotations, and actual mixed parameter/note/MIDI ordering. Count/aggregate byte limits bound callback admission and output attempts; rejected pushes do not refund payload budget. One caller-owned output sink orders both note and MIDI families. Callback-local payload loans leave only POD metadata in persistent scratch.

The zero-audio MIDI probe routes two inputs to two outputs with deliberate host/schema index differences and a timed Boolean enable control. The same-pin Clack delta adds the required MIDI wrapper layout annotation. [Events/transport](design/events-transport.md) owns semantics, the [source audit](research/clack-pinned-adapter-audit.md) owns the FFI boundary, and [validation](design/validation.md) owns revision-scoped evidence.

## Next — MIDI 2/UMP

Add MIDI 2/UMP with explicit capabilities, bounded payload lifetime, faithful deployment projection, and a focused client. Do not silently convert native identity/expression into a lossy MIDI representation. Per-note parameter modulation and span processing remain separate follow-up.

Speaker layouts and richer I/O policies remain demand-driven. Core proof is not format support, and additional formats/editors/devices/graphs do not justify freezing incomplete semantic surfaces.

## Qualification maintenance

- Keep current Rust/dependency gates and revision-scoped platform evidence distinct.
- After adapter changes, rebuild/package all exported fixtures and run the pinned validator plus bounded fuzz.
- Refresh real-host automation, save/reopen, active save, restart, and PDC when a production DAW is available; the September REAPER record is historical.
- Refresh representative performance when making a comparison or renewing a claim. The old safe-pair benchmark does not measure the new raw alias boundary.

Existing products remain optional integration evidence. No product migration, version milestone, publication, or deployment is implied by this plan.
