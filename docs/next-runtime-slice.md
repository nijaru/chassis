# Current Execution Plan

This file tracks the next implementation order on `main`. It is not an API compatibility promise.

## Current checkpoint

The format-independent runtime/state/parameter model and current native CLAP semantics are executable under the automated headless matrix.

Current automated evidence includes:

- Rust fmt, Linux workspace tests, strict Clippy, and Loom;
- full workspace tests plus all-target/all-feature checks on macOS and Windows;
- `cargo-deny` and `cargo-machete` policy gates;
- packaged CLAP validation and five-second two-worker fuzzing on Linux, macOS, and Windows with pinned `clap-validator` 0.4.1;
- validator result: 35 passed, 0 failed, 9 intentional skips;
- f32/f64 exported sample-accurate `trim` DSP;
- full CLAP adapter callback allocation/deallocation probe at the configured 64-event bound, stereo main + stereo sidechain, 1,000 f32 and 1,000 f64 callbacks: zero measured callback-thread allocation/deallocation;
- CLAP minimum/maximum frame-count processing plus over-bound rejection;
- result-based activation failure and panic-based activation failure both leave the same CLAP instance reusable;
- process panic is caught at the Clack FFI boundary and appears to the host as processing failure;
- repeated inactive instance construction/destruction, sample-rate/block-size reactivation, and audio-thread transfer;
- plugin -> host -> plugin main-thread re-entry through parameter-rescan and latency-change callbacks;
- deliberate delayed probe: reported 64-sample latency matches an impulse delayed by exactly 64 samples;
- active CLAP state save during processing linearizes to a coherent completed generation.

The historical REAPER 7.79/macOS-arm64 baseline predates current observable behavior and remains regression history only.

## Slice 1 — representative performance evidence

This is the remaining code-side Phase-2 measurement that can be performed without a DAW, but it requires stable representative hardware rather than a shared CI runner.

Use:

```text
cargo bench -p chassis-clap --bench adapter_overhead
```

The harness exercises the actual in-process CLAP adapter with:

- 32/64/128/512-frame blocks;
- no-event and configured 64-event load;
- f32 and f64;
- stereo main + stereo sidechain;
- trivial product DSP to expose adapter cost.

Record CPU, OS/architecture, Rust version, release profile, sample rate, topology, event load, and benchmark output. Treat the result as a distribution/throughput measurement, not a universal constant. Optimize only if a material cost appears.

## Slice 2 — real-DAW qualification when available

This is externally blocked until a suitable production DAW is available.

Verify current head in at least one real host:

1. scan and instantiate;
2. save/reopen state;
3. render actual `trim` automation and compare sample offsets against the deterministic reference;
4. save state while automation is active;
5. verify PDC/alignment with delayed DSP;
6. exercise native f64 dispatch if the host can select it.

The in-process host and three-platform validator matrix establish format semantics; this slice establishes production-host behavior.

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
