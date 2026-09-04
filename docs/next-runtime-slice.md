# Current Execution Plan

This file tracks the next implementation order on `main`. It is not an API compatibility promise.

## Current checkpoint

Current Rust checkpoint: `c6aa54f2022d456e89329fd969ba993eb8d76e52`.

Rust CI is green for fmt, workspace tests, strict Clippy, and Loom. The Linux CLAP conformance workflow builds/packages the current artifact, runs pinned `clap-validator` 0.4.1, and runs bounded fuzzing.

Current executable evidence includes:

- current Linux `clap-validator`: 35 passed, 0 failed, 9 intentional skips;
- five-second two-worker validator fuzz: clean;
- f32/f64 exported sample-accurate `trim` DSP;
- full CLAP adapter callback allocation/deallocation probe at the configured 64-event bound, stereo main + stereo sidechain, 1,000 f32 and 1,000 f64 callbacks: zero measured allocation/deallocation on the callback thread;
- CLAP minimum/maximum frame-count processing plus over-bound rejection;
- product activation failure leaves the same CLAP instance inactive and reusable;
- repeated inactive instance construction/destruction;
- sample-rate/block-size reactivation and audio-thread transfer;
- CLAP latency-change callback on activation;
- deliberate delayed probe: reported 64-sample latency matches an impulse delayed by exactly 64 samples;
- active CLAP state save during a paused audio callback returns the coherent pre-publication generation; after callback completion it returns both new automation endpoints.

The historical REAPER 7.79/macOS-arm64 baseline predates current observable behavior and remains regression history only.

## Slice 1 — real-DAW qualification when available

This is externally blocked until a suitable DAW is available; do not stall code work on it.

When available, verify current head in a production host:

1. scan and instantiate;
2. save/reopen state;
3. render actual `trim` automation and compare sample offsets against the deterministic reference;
4. save state while automation is active;
5. verify PDC/alignment with delayed DSP;
6. exercise native f64 dispatch if the host can select it.

The in-process host tests already cover the semantic contracts; this slice establishes production-host behavior.

## Slice 2 — representative performance evidence

The zero-allocation callback claim now has both core and full-adapter executable evidence. The remaining performance question is quantitative adapter overhead.

Measure on stable representative hardware, not a shared CI runner:

- adapter-only processing with trivial DSP;
- representative block sizes such as 32/64/128/512;
- no-event and maximum configured event load;
- f32 and f64;
- main-only and main + auxiliary/sidechain topology;
- release build, fixed toolchain, recorded CPU/OS.

Report distribution/throughput rather than one noisy timing. Do not optimize before measurements identify a material cost.

## Slice 3 — first real FX client

Once a product explicitly opts into Chassis, use it as the primary API pressure. A mastering-limiter class of client is the intended first case because it exercises the right missing capabilities. Do not duplicate or silently migrate the existing Truce `audio-plugins` implementation.

The client should drive:

- activation-time lookahead and oversampling resources;
- real product latency/restart behavior;
- offline/realtime parity;
- automation and explicit smoothing policy;
- state/preset behavior;
- bounded meter/gain-reduction telemetry;
- deterministic host renders and reopen tests.

Promote framework helpers only after client code demonstrates recurring framework-owned behavior.

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
