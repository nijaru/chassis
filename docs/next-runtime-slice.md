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

A resource-backed table fixture couples parameter addressing to custom data and processes before restart. The first accepted active custom-state change freezes the pre-load DSP parameter projection. Incoming parameter automation remains validated but is discarded, with no endpoint completion, until deactivation/reactivation. Later loads and control edits affect saveable desired state only. Reset and state reversion cannot unfreeze the old resources. This deliberately sacrifices parameter automation while a host defers restart; retaining it would require a qualified resource-compatibility policy.

CLAP scalar load and activation now use component validation/migrations and reject schema drift. Every factory/main-thread/activation/load admission rejects custom defaults rather than silently losing them. Complete custom-state projection remains unsupported.

## Next — complete CLAP state projection

Project one coherent parameter/custom semantic generation across inactive and active CLAP lifetimes. Never combine independently sampled scalar/custom snapshots or let an old processor publish endpoints into a newly accepted complete generation. Exercise the resource-backed transition through the actual plugin entry, including default save, migrated load, deferred restart, save/reopen, and rejected candidates. The scalar bridge is not yet this contract.

Start with reactivation-based resource replacement. Bounded prepared adoption, coalescing, stale-result rejection, deferred reclamation, cancellation, and unload fencing need a concrete client before choosing a queue or executor. No hidden global executor or last-owner `Arc` destruction on the callback.

## Parameter authoring policy

Descriptors validate typed plain-unit domains. CLAP supplies scalar plain-value conversion and basic numeric/choice text; a shared normalized-control mapping and author-defined formatting/parsing policy are not implemented.

Specify domain mapping, units, precision, parsing, and stepped/choice round trips at the semantic owner. Migrate deployments to that policy and prove nonlinear mapping, endpoints, invalid input, and supported round trips through the actual adapter. Dense callback values remain separate from display/control mapping. Do not add macros or a crate before the ordinary API is coherent.

## Following audio contracts

Deliver each contract with its deployment projection and focused fixture:

- modulation separate from base state/automation, with explicit timing and combination policy;
- tail separate from latency;
- host bypass identity/timing and product DSP policy, not a generic dry/wet control;
- note-port/input CLAP projection and a non-effect export with honest descriptor features;
- bounded output events, expression, MIDI 1/SysEx, and MIDI 2/UMP with explicit rejection rather than destructive translation.

Speaker layouts and richer I/O policies remain demand-driven. Core proof is not format support, and additional formats/editors/devices/graphs do not justify freezing incomplete semantic surfaces.

## Qualification maintenance

- Keep current Rust/dependency gates and revision-scoped platform evidence distinct.
- After adapter changes, rebuild/package both fixtures and run the pinned validator plus bounded fuzz.
- Refresh real-host automation, save/reopen, active save, restart, and PDC when a production DAW is available; the September REAPER record is historical.
- Refresh representative performance when making a comparison or renewing a claim. The old safe-pair benchmark does not measure the new raw alias boundary.

Existing products remain optional integration evidence. No product migration, version milestone, publication, or deployment is implied by this plan.
