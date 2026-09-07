# Current Execution Plan

This file tracks the next implementation order on `main`. It is not an API compatibility promise.

## Current checkpoint

The format-independent runtime and native CLAP adapter are implemented. The [validation guide](design/validation.md) owns the automated checks, dated REAPER results, and representative adapter benchmark. These results are baselines; changes require checks appropriate to the affected guarantees.

## Completed foundation

- **Slice 1 — adapter performance:** measured on Apple M3 Max hardware on 2026-09-03. Re-run `cargo bench -p chassis-clap --bench adapter_overhead` when adapter changes need a performance comparison.
- **Slice 2 — real-DAW qualification:** REAPER 7.79/macOS-arm64 baseline recorded on 2026-09-06 for scan/instantiate, automated rendering, state round-trip, active save, and PDC alignment. Refresh affected host scenarios after relevant changes.

Native f64 processing is covered headlessly. A client's product requirements determine whether to advertise a preference for native f64 wire dispatch.

## Slice 3 — first real FX client

The first client is opted in: the **Tonal EQ** port from Truce `audio-plugins` (decision recorded 2026-09-06, `audio-plugins` `docs/migration.md` commit `92c2b52`). Port order is fixed on the client side: DSP parity against the frozen JUCE four-band oracle (`plugins-juce/eq`, 28 parameters) first, then the five-slot product changes as separate verified changes. Do not duplicate or silently migrate the existing Truce implementation.

The EQ exercises the framework at parameter scale the conformance artifact does not reach — a 28-parameter surface, per-band shape choice params — while needing no lookahead, oversampling, or telemetry for the parity pass. Use it as the primary API pressure: promote framework helpers only after the port code demonstrates recurring framework-owned behavior.

The client should drive:

- real parameter-count scale (28 parameters, choice params) through descriptor publication, dense-index mapping, and state save/load;
- real product latency policy (zero for the parity pass) and restart semantics;
- offline/realtime parity;
- automation and explicit smoothing policy at product scale;
- state/preset behavior under product use;
- deterministic host renders and reopen tests (the REAPER methodology from Slice 2 applies directly).

The originally intended mastering-limiter pressure (activation-time lookahead and oversampling resources, bounded meter/gain-reduction telemetry) moves to the Invisibull opt-in. The decision trail is recorded here so the delayed client does not silently drop it.

## Slice 4 — editor-facing capabilities driven by the client

When the first Chassis client actually needs a production editor:

1. define product-originated begin/change/end gesture semantics and echo suppression;
2. add bounded meter/telemetry publication with explicit ownership and reclamation;
3. prove editor attach/detach, recreation, resize/scaling, and parameter observation;
4. add tail/status or background task infrastructure only if product semantics require it.

Defer note/MIDI/event ports until an instrument or event processor becomes a real client.

## Slice 5 — VST3/AU and editor qualification

After native CLAP semantics are coherent under a real client:

- qualify a pinned/reviewed `clap-wrapper` revision or release;
- run VST3/AU native validators and real-host matrices;
- add cross-format differential state/audio/automation/latency tests;
- include Ableton save/reopen behavior in VST3 qualification because wrapper state restoration is host-sensitive;
- prove editor lifecycle across projected formats;
- then add production packaging/signing/notarization.

## Explicitly deferred

- speculative note/MIDI infrastructure;
- generic task/executor services;
- custom allocator/SIMD/zero-copy work without measurements;
- API convenience macros before real-client repetition exists;
- replacing Truce in existing products without an explicit migration decision.
