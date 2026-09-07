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

Current head carries REAPER 7.79/macOS-arm64 real-DAW evidence (2026-09-06, M3 Max, 48 kHz) for scan/instantiate, sample-exact automated `trim` rendering, injected-state save/reload rendering, active-save during automated playback, and PDC alignment with the exported delayed probe. Native f64 dispatch remains open (see Slice 2).

## Slice 1 — representative performance evidence

**Complete (2026-09-03).** Recorded in `docs/design/validation.md` under "Realtime allocation and performance": adapter-only median 49–53 ns/callback at zero event load and 477–499 ns at the configured 64-event load (~7 ns/event), flat across 32–512 frames and f32/f64, on Apple M3 Max / macOS 26.6.2 / rustc 1.98.0. No material adapter cost appeared, so no optimization work is warranted.

One-time fix during this slice: `benches/adapter_overhead.rs` is a `main()`-based manual harness, but the bench target was auto-discovered with the libtest harness, so `cargo bench` reported "0 tests" and never ran. `crates/chassis-clap/Cargo.toml` now declares `[[bench]] name = "adapter_overhead" harness = false`.

The harness details are retained for reproducibility:

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

## Slice 2 — real-DAW qualification when available

**Complete (2026-09-06): REAPER 7.79/macOS-arm64, M3 Max, 48 kHz, headless `-renderproject`.** All DAW-verifiable items passed with sample-exact evidence:

1. ~~scan and instantiate~~ — FX browser lists "CLAPi: Chassis Conformance"; instantiation enumerates 5 parameters (Mode/Trim/Bypass/Wet/Delta);
2. ~~save/reopen state~~ — injected CHSS blobs (trim 0.25/0.75, validated by the real `StateDocument` decoder and byte-identical to a REAPER-saved blob at 0.5) render sample-exact after reload;
3. ~~render actual `trim` automation~~ — square envelope renders sample-exact: trim changes land at exact sample offsets 24000/36000 against the deterministic reference (48,000/48,000 frames);
4. ~~save state while automation is active~~ — project saved while the transport rolled through 4-point trim automation; saved base state coherent (trim=0.5, not torn); automation replays sample-exact after reload;
5. ~~verify PDC/alignment with delayed DSP~~ — complete (2026-09-06): the exported 64-sample delayed probe (`examples/clap-delayed-probe`, a workspace member) mixes against a dry track as exactly `2*x[i]` for all 48,000 frames; REAPER honored the CLAP latency extension and aligned the delayed wet path sample-exact.

The sixth candidate item, native host f64 dispatch, is not a DAW-qualification gap: f64 semantics are implemented and qualified through the in-process host matrix and the three-platform validator (both double-precision process cases pass). No available host selects `data64` for this artifact (REAPER chooses f32), so wire-precision preference moves to first-client pressure — the first real client decides whether to advertise `PREFERS_64BITS` from its product semantics.

Recorded host quirk (not an adapter defect): `TrackFX_SetParamNormalized` on an *inactive* CLAP reports success but delivers no parameter change; FabFilter Pro-Q 4 (CLAP) shows the identical stale readback while ReaEQ (VST) applies immediately. The automation path (process events) is sample-exact, which is the semantically meaningful host behavior.

Methodology notes: RPP files must stay CRLF (LF-only makes `-renderproject` stall); 32-bit float render sink is `RENDER_CFG` blob `ZXZhdyAAAQ==`; project rate must be forced with `PROJECT_SRATE_USE=1`; render with `-nosplash -newinst -renderproject <file>`.

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
