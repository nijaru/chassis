# Current Execution Plan

This file tracks the next implementation order on `main`. It is not an API compatibility promise.

## Qualified checkpoint

The durable format-independent runtime owns parameter/custom semantic state through `InstanceRuntime<P>`. CLAP uses setup-built mapped audio slots, bounded parameter-event scratch, generation-checked scalar publication, optional f64 processing, render-mode projection, and explicit parameter/state identity.

Current native evidence for the parametered conformance artifact:

- local Rust fmt/test/Clippy/deny/machete/release gate recorded green for the qualified checkpoint;
- `clap-validator` 0.4.1: 35 passed, 0 failed, 9 intentional skips;
- five-second two-worker validator fuzz: clean;
- REAPER 7.79/macOS-arm64: scan, instantiate, two-parameter state round trip, deterministic f32 render through a saved project;
- native `data64` host dispatch remains unexercised because REAPER selected f32 for the advertised capability set.

Historical narrower validator counts are superseded by this checkpoint.

## Slice 1 — repository truth and API cleanup

Do first because stale docs currently point agents at completed milestones.

- reconcile README, roadmap, AGENTS, validation, authoring, and open-question docs;
- define GitHub Actions as portable Rust regression signal rather than production plugin qualification;
- close obsolete validation-only PRs/branches when no longer useful;
- migrate remaining conformance callers from activation-local `Activated<P>` to `InstanceRuntime`;
- remove `Activated<P>` and the temporary free `runtime::activate()` compatibility path once no caller remains;
- rename temporary adapter APIs when their names encode obsolete proof limitations.

Exit: one lifecycle/state authority is visible in code and documentation.

## Slice 2 — audible exported automation

The core conformance processor already proves sample-accurate float trajectories. The exported CLAP artifact must now prove the same semantics end-to-end.

Implement:

1. `trim` remains a float parameter in `[0, 1]` with default `0.5`;
2. exported f32/f64 DSP reads the canonical base `trim` plus block automation trajectory;
3. each main-channel sample is multiplied by the effective `trim` value at that sample offset;
4. `mode` remains process-inert unless a later conformance case needs stepped DSP behavior;
5. deterministic tests cover base trim and a multi-event trajectory for f32 and f64;
6. re-run validator + a host render with actual automation and compare against the reference trajectory.

Exit: CLAP parameter events are proven to affect rendered samples at the intended offsets, not merely transport/state plumbing.

## Slice 3 — active state-save consistency

Current scalar publication already prevents stale realtime completion from overwriting a newer control/state generation. Formalize state save on that mechanism.

Contract:

- the audio thread never blocks for state save;
- save serializes one coherent **completed** scalar publication generation;
- if save races a process block, it may linearize immediately before or immediately after that block's endpoint publication, but never observe a mixed cross-parameter generation;
- if save races a newer control edit/state load, generation ordering determines the winner and stale realtime publication is discarded;
- state load remains transactional and requests host value rescan after publication.

Add executable tests for:

- save after one automation block contains final automation endpoints;
- save racing publication returns a coherent old-or-new generation, never mixed values;
- a newer control/state publication wins over a stale realtime snapshot;
- deactivate/reactivate synchronizes the saved current generation into the fresh runtime.

Then qualify active save during automation in a real CLAP host where the host exposes a reproducible scenario.

Exit: `docs/design/parameters-state.md` can state an implemented and tested active-save contract rather than a future requirement.

## Slice 4 — realtime evidence

Add mechanical evidence for guarantees already designed into the path.

- allocation/deallocation detector around process after activation;
- worst-case configured parameter-event count;
- representative mapped main + sidechain/aux topology;
- frame-count extremes including zero where legal;
- adapter-only overhead benchmark separate from DSP;
- document workload, block sizes, channel count, CPU/architecture, build mode, and toolchain.

Do not turn this into speculative micro-optimization. The goal is to verify allocation/work claims and establish a baseline.

## Slice 5 — lifecycle negative space

Close remaining deployment edge cases:

- activation failure cleanup;
- create/init/destroy without activation;
- repeated activate/deactivate and sample-rate/block-size changes;
- processor movement between host audio threads without simultaneous mutation;
- repeated instance creation/destruction;
- reentrant host callbacks allowed by the pinned Clack model;
- module unload once future callback/task services exist.

Bitwig is useful additional reentrancy coverage when available.

## Slice 6 — first real FX client: mastering limiter

Start the real client before implementing broad note/MIDI or generic background infrastructure.

Initial limiter integration should force concrete answers for:

- latency and lookahead declaration;
- offline/realtime parity;
- activation-time buffers/oversampling preparation;
- automation and smoothing policy;
- meter/telemetry publication;
- state/preset behavior;
- deterministic host renders.

Framework helpers graduate only when the limiter or subsequent EQ/dynamics clients prove common semantics.

## Slice 7 — capability expansion driven by clients

Likely limiter-era CLAP work:

1. latency metadata/change behavior;
2. product-originated gesture/edit path needed by the editor;
3. meter/telemetry transport;
4. tail/status only if product semantics require them.

Defer note/MIDI/event ports until an instrument or event processor becomes a real client.

## Slice 8 — VST3/AU and editor

After native CLAP + limiter evidence is coherent:

- qualify a pinned/reviewed `clap-wrapper` revision/release;
- run VST3/AU native validators and real-host matrices;
- add cross-format differential state/audio/automation tests;
- prove editor attach/detach, resize/scaling, recreation, parameter gestures, and telemetry;
- then add production packaging/signing/notarization.
