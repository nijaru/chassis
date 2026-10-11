# Open Design Decisions

This file owns unresolved decisions, not implemented inventories or a second roadmap. [Execution plan](../next-runtime-slice.md) owns next work; [validation](validation.md) owns evidence; [roadmap](../roadmap.md) owns deferred scope. All APIs and CHSS bytes remain pre-alpha.

## Parameter authoring policy

Plain/normalized linear mapping and basic CLAP text conversion already exist. What semantic policy should own nonlinear mapping, units, formatting/parsing, display precision, stepped rounding, and meaningful round trips? DSP, editors, and deployment adapters must not maintain competing mappings. Keep stable choice identity independent of display names and backend indices.

## Modulation and event ordering

Define modulation domain, timing, addressing, combination, and reset semantics separately from automation and persistent base state. Which capabilities preserve global/per-note expression and MIDI 2 information? Equal timestamps do not establish a cross-family causal order when the source has separate streams. An output sink must report target rejection rather than silently discard essential events.

See [events/transport](events-transport.md) for the accepted ordering constraints and current implemented subset.

## Complete deployment state

Core owns complete transactional parameter/custom state and adjacent migrations. CLAP currently persists parameters only and rejects custom fields; it has no complete-state migration/product-validation projection. Define durable inactive/active ownership across adapter lifetimes before extending publication to prepared objects. State save must capture one coherent completed semantic generation, not merely independent scalar and custom snapshots.

When should the provisional CHSS envelope become a compatibility promise? Released-schema fixtures, cross-format round trips, resource limits, and an explicit compatibility policy must justify that decision; the existing binary fixture alone does not.

## Tail, bypass, and activity

Define finite/infinite tail reporting independently of latency. Determine host-bypass identity/timing, latency/tail preservation, and product DSP policy without equating bypass to dry/wet or forcing one crossfade algorithm. Introduce portable activity/sleep hints only if actual deployment behavior benefits and conservative fallback is explicit.

## Prepared data and tasks

Who owns active and pending immutable resources, coalesced replacements, and deferred reclamation? What invalidates task generations on load/reset/deactivation/unload? Cancellation may be late; teardown must remain safe, and offline output cannot depend on wall-clock races. Establish these from a resource-backed fixture before choosing an executor.

## Outer-layer pressure

The heterogeneous core fixtures establish useful evidence for the current schema/lifecycle model, not immunity from refactoring. Reopen it for a concrete mismatch, not speculative generality.

Remaining decisions belong to actual clients:

- I/O-policy families or bus helpers when explicit configurations become materially unwieldy;
- speaker/ambisonic identity and ordering when mapped layout support is implemented;
- editor lifetime/threading, gesture echoes, scaling, and toolkit ownership;
- wrapper versus native VST3/AU adapters after semantic/lifecycle qualification;
- device clock/reconfiguration/recovery and graph topology/PDC publication;
- hosted-plugin failure/isolation policy and media/random-access boundaries.

Keep application documents and workflows outside these contracts. Do not settle a future crate graph as a substitute for answering the ownership question.

## Release decisions

Before any stable support claim, specify the supported surfaces, platform/format/feature matrix, MSRV/version/state policy, production-host evidence, and known limits. Broader roadmap ambition is not evidence for an unimplemented capability. Version milestones, publication, and commercial/contribution terms require separate explicit decisions.
