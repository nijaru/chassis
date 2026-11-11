# Open Design Decisions

This file owns unresolved decisions, not implemented inventories or a second roadmap. [The execution plan](../next-runtime-slice.md) orders work; [validation](validation.md) owns evidence; [the roadmap](../roadmap.md) owns the initial core boundary, stability gates, and possible later extensions. All APIs and CHSS bytes remain pre-alpha.

## State validity and resource compatibility

How do descriptor-valid independent edits and automation preserve reloadable complete semantic state when a component has cross-field constraints? Which values are automation-safe, which changes require a validated coupled transaction, and which mismatches mean only that current resources are stale? Resolve the [known validity gap](parameters-state.md#state-validity-closure) before treating complete persistence as stable.

## Control trajectories and origins

Which representation preserves source interpolation domains and boundary anchors without callback use of non-realtime presentation policy? [Trajectory portability](parameters-state.md#trajectory-portability-before-stabilization) records the VST3 normalized-ramp counterexample that the initial core proof must address.

How do typed gestures and origin-aware changes preserve host notification ordering, observation, echo suppression, and state/resource authority while processing is stopped or a replacement is pending? These are core control questions; they do not require an editor toolkit.

## Prepared data and background-work lifetime

Who owns active and pending immutable resources, coalesced replacements, and deferred reclamation? Which transitions permit compatible adoption, and which require reactivation? What invalidates results on load/reset/deactivation/unload? Cancellation can be late; teardown must remain safe, and offline output cannot depend on wall-clock races.

This is committed core closure work. Establish the [resource lifecycle](runtime-services.md#control---realtime-publication) with an in-repository workload before choosing a mechanism. An executor implementation or general task framework is a separate decision.

## Bounds and failure policy

What happens at each supported capacity boundary: event admission/output, control publication, resource supersession and observations? Essential events and replaceable observations need not have the same loss/failure policy. Make outcomes visible without realtime logging, blocking, or hidden allocation. Clarify which failures end a run and which permit continued valid processing.

Additional event targeting, portable activity/sleep hints, larger observation streams, and richer I/O policies are not automatic core gates. The supported subset must remain explicit; no silent flattening of note identity, MIDI information, source ordering, or labeled layouts.

## Compatibility and extension decisions

When should the provisional API and CHSS envelope become compatibility promises, and what is their supported scope? Define initial versioned fixtures, migration/resource limits and the compatibility policy before release; retain released-schema fixtures thereafter. Cross-format round trips qualify a later selected adapter, not an obligation to implement another format before core stability.

After the core gates, which narrow extension is worth maintaining? Editor attachment/toolkits, devices/standalone, more plugin formats, graphs/PDC, hosting, and media/offline job infrastructure are candidates, not accepted obligations. Their boundaries must preserve the existing semantic authority, but the core need not implement every subsystem surrounding its consumers.

Before a stable support claim, settle the supported API/platform/format/feature matrix, compiler/version/state policy, production-host evidence and known limits. Version milestones, publication, and commercial/contribution terms remain separate explicit decisions. Do not choose a future crate graph as a substitute for a scope and ownership decision.
