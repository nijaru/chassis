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

## Next — component-owned complete state

The runtime can now publish valid complete state and request custom-state reactivation, but product defaults, validation, and migration are still supplied by callers rather than one component-owned contract. CLAP still projects scalar state only and rejects custom fields/event-port schemas.

1. Give the component one complete-state defaults/validation/migration contract and one activation preparation hook. Remove the historical activation-hook chain when migrating callers; do not preserve it as a compatibility family.
2. Exercise a resource-backed component whose custom state prepares activation-scoped DSP resources. Couple a parameter to those resources and process between accepted active load and deferred host reactivation, not only after restart. Current core loads make replacement parameters immediately visible while old custom-derived resources remain active: settle the safe transition policy before extending deployment state. Prove default construction, rejected candidates, active-load restart, state save/reopen, and deterministic reactivation.
3. Project one coherent parameter/custom semantic generation across inactive and active CLAP lifetimes. Complete migration and product validation at that boundary; never combine independently sampled scalar/custom snapshots.

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
