# Current Execution Plan

This file tracks the next implementation order on `main`. It is not an API compatibility promise.

## Current code checkpoint

The durable format-independent runtime owns parameter/custom semantic state through `InstanceRuntime<P>`. `Processor` exclusively owns active DSP history and now reports activation-established processing latency, which the runtime snapshots for the active lifetime.

The CLAP adapter uses setup-built mapped audio slots, bounded parameter-event scratch, generation-checked scalar publication, optional f64 processing, render-mode projection, explicit parameter/state identity, audible `trim` automation, and CLAP latency projection.

Rust CI is green through the CLAP latency projection checkpoint (`081caccadaca8c8038efa53f1fbbb41695143321`): fmt, workspace tests, strict Clippy, and Loom all pass.

Completed since the last native CLAP qualification baseline:

- exported `trim` now audibly drives deterministic f32/f64 DSP sample by sample;
- conformance callers use `InstanceRuntime`; the temporary `Activated<P>` / free `runtime::activate()` lifecycle path is removed;
- publication tests cover coherent completed snapshots, in-progress multi-value writes, stale realtime generations, and bounded realtime retry behavior;
- a thread-local allocation probe verifies repeated post-activation core `process` calls perform no allocation/deallocation on the callback test thread;
- activation failure is tested to leave the runtime inactive, state-preserving, and reusable;
- `LatencySamples`, `Processor::latency()`, and `InstanceRuntime::active_latency()` define activation-scoped latency;
- CLAP exposes the latency extension and announces a changed activation latency to supporting hosts.

## Native evidence boundary

The most recent recorded native baseline predates those changes:

- `clap-validator` 0.4.1: 35 passed, 0 failed, 9 intentional skips;
- five-second two-worker validator fuzz: clean;
- REAPER 7.79/macOS-arm64: scan, instantiate, two-parameter state round trip, deterministic f32 render;
- native `data64` host dispatch remained unexercised because REAPER selected f32.

Do not describe that baseline as qualification of current head. The implementation changed in observable automation and extension behavior after it was recorded.

## Slice 1 — requalify current native CLAP head

This is the immediate priority.

1. Build/package the current conformance `.clap` artifact.
2. Run the current `clap-validator` suite and record exact pass/fail/skip counts.
3. Repeat bounded validator fuzzing.
4. Re-open the artifact in REAPER and verify scan + instantiation + state save/reopen.
5. Automate `trim` through a deterministic trajectory and compare rendered samples against the expected reference, including event offsets rather than only block-end state.
6. Exercise state save while automation is running and verify the saved state is one coherent completed generation.
7. Retain validator-qualified f64 coverage; attempt native `data64` dispatch only where a host can actually select it.

Latency note: the current conformance processor reports zero latency. Validator/host requalification can prove extension presence and zero-latency behavior, but **nonzero PDC correctness requires actual delayed DSP**. Exercise that with the limiter or a deliberate delayed conformance case; do not report nonzero latency without delaying audio by the same amount.

Exit: current head, rather than the historical fixed/earlier artifact, owns the recorded CLAP validator and REAPER evidence.

## Slice 2 — finish realtime/work-bound evidence

The core callback allocation probe now provides one mechanical zero-allocation result. Extend evidence to the real adapter path rather than repeating the same core proof.

Cover:

- configured maximum parameter-event count;
- representative mapped main + sidechain/aux topology;
- minimum/maximum/legal zero frame counts where applicable;
- f32 and f64 adapter paths where executable;
- allocation/deallocation across the complete adapter process path;
- adapter-only overhead separate from DSP.

Record workload, block sizes, channel count/layout, event count, CPU/architecture, build mode, and toolchain. Do not turn the exercise into speculative micro-optimization.

Exit: realtime claims have representative mechanical evidence rather than source inspection alone.

## Slice 3 — close deployment negative space

Core activation failure is now covered. Focus remaining work on host/deployment behavior:

- repeated activate/deactivate and sample-rate/block-size changes through CLAP;
- create/init/destroy without activation where the native harness can force it;
- processor movement between host audio threads without simultaneous mutation;
- repeated instance creation/destruction;
- reentrant host callbacks allowed by the pinned Clack model;
- eventual module unload only when future callback/task services make it relevant.

Bitwig is useful additional reentrancy coverage when available.

## Slice 4 — first real FX client: mastering limiter

Start the real client before implementing broad note/MIDI or generic background infrastructure.

The limiter should provide the first product-level proof for:

- actual lookahead/delayed audio matching reported latency;
- restart/reactivation when activation-scoped latency changes;
- offline/realtime parity;
- activation-time lookahead and oversampling resources;
- real automation and explicit smoothing policy;
- meter/telemetry publication;
- state/preset behavior;
- deterministic host renders and reopen tests.

Keep its DSP crate wrapper-agnostic. Framework helpers graduate only when limiter integration demonstrates that the behavior belongs to Chassis rather than the product.

## Slice 5 — editor-facing capabilities driven by the client

Once the limiter needs a production editor:

1. define product-originated begin/change/end gesture semantics and echo suppression;
2. add bounded meter/telemetry publication with explicit ownership and reclamation;
3. prove editor attach/detach, recreation, resize/scaling, and parameter observation;
4. add tail/status or background task infrastructure only if actual product semantics require it.

Defer note/MIDI/event ports until an instrument or event processor becomes a real client.

## Slice 6 — VST3/AU and editor qualification

After native CLAP + limiter evidence is coherent:

- qualify a pinned/reviewed `clap-wrapper` revision or release;
- run VST3/AU native validators and real-host matrices;
- add cross-format differential state/audio/automation/latency tests;
- include Ableton save/reopen behavior in VST3 qualification because wrapper state restoration is a host-sensitive boundary;
- prove editor lifecycle across projected formats;
- then add production packaging/signing/notarization.
