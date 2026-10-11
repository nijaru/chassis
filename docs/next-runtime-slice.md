# Current Execution Plan

This file owns the immediate implementation sequence. [Roadmap](roadmap.md) owns deferred framework work; [validation](design/validation.md) owns executed evidence. Neither a plan nor passing historical tests is a compatibility promise.

## Current foundation

The format-independent runtime has one schema/state/lifecycle model exercised by effects, event-only components, a note-driven instrument, multiple outputs, layout/sidechain reactivation, and runtime-owned metadata. It supports f32/f64, sample-accurate parameter automation, transactional semantic state, activation-scoped latency/restart, and bounded `f32` telemetry.

The CLAP adapter is narrower: fixed mapped audio configurations, scalar parameters/automation/state, render mode, and latency. `ClapComponent` now supplies explicit feature metadata rather than inferring stereo-effect semantics. It does not project note ports, event output, modulation, or complete custom semantic state. Core proof does not establish those deployment capabilities.

The package boundary is runtime semantics (`chassis-runtime`) versus format deployment (`chassis-clap`). No other production package is needed yet.

## Next — automation endpoint ownership

Direct runtime processing currently leaves durable parameters unchanged after a block's automation; CLAP separately retains completed endpoints. Define the carryover owner, multi-block ramp behavior, saved-state semantics, and newer-control precedence. Prove two-block continuity through direct runtime and adapter clients before moving completion into one authority.

CLAP now rejects event-port schemas at setup and custom state entries on load, rather than accepting unsupported semantics silently. Complete state/migration/product-validation projection remains unfinished; these rejections are containment, not implementation of that capability.

## Then — parameter authoring contract

Parameter descriptors already validate typed values and map plain/normalized ranges; CLAP already supplies basic numeric and choice text conversion. The missing contract is an author-defined mapping/formatting policy that both DSP and deployments consume, not a second generic normalization layer.

1. Specify domains, units, display precision, parsing, and stepped/choice round-trip rules.
2. Implement the policy at its semantic owner and migrate CLAP conversion to it.
3. Prove nonlinear mapping, invalid input, endpoints, and supported round trips through the public API and actual adapter entry point.

Do not add macros or a new crate to make these declarations shorter before the ordinary API is coherent.

## Following core contracts

Implement one contract at a time with its deployment projection and focused fixture:

- sample-accurate modulation, separate from base state and automation, with explicit combination policy;
- tail reporting distinct from latency;
- host bypass identity/timing and product DSP policy, without treating bypass as dry/wet;
- note-port/input CLAP projection and a real non-effect export using `ClapComponent` with explicit descriptor features;
- bounded output events, expression, MIDI 1/SysEx, and MIDI 2/UMP with explicit rejection rather than destructive translation.

Resolve cross-format semantic questions at each boundary rather than declaring all core APIs closed before an adapter exercises them. Speaker layouts and richer I/O policies remain demand-driven; current heterogeneous fixtures do not show a need for a policy language.

## Communication and background work

Fixed-width coherent DSP-to-control telemetry exists. Prepared control-to-DSP objects, deferred reclamation, and background tasks do not.

A resource-backed fixture must establish ownership, replacement/coalescing, generation/stale-result rejection, cancellation, shutdown/unload, and offline determinism before choosing an executor or queue. No hidden global executor or general lock-free container library.

## Qualification maintenance

- Run current Rust/dependency gates; keep local and hosted compiler/tool evidence distinct.
- Review the published Clack 0.2.0 source against the audited pin before deciding whether to adopt its registry release. A source change requires renewed adapter conformance.
- Rebuild/package both CLAP fixtures and run the pinned validator plus bounded fuzz after adapter changes.
- Refresh real-host automation, state/save-reopen, active-save, and PDC evidence when a production DAW is available. The September REAPER record remains a historical baseline.
- Refresh representative performance only when making a comparison or renewing a performance claim.

Existing products remain optional integration evidence. Tonal EQ parity or redesign is not a Chassis prerequisite, and no product repository is migrated implicitly.
