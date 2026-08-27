# Roadmap

This roadmap is ordered by architectural proof, not release promises. Features move forward when real product requirements and validation justify them.

## Phase 0 — foundation

- Define the format-independent audio-component model.
- Establish explicit realtime/main/shared ownership boundaries.
- Define audio-port and extensible channel-layout metadata.
- Establish repository architecture, dependency, testing, and licensing policies.
- Build a tiny conformance component used to stress framework semantics rather than DSP quality.

Exit condition: `chassis-core` is small, compilable, and does not encode FX-only, stereo-only, plugin-only, or backend-specific assumptions.

## Phase 1 — first complete plugin path

Primary client: a simple effect.

- CLAP adapter using Clack.
- Stereo main input/output convention with optional stereo sidechain.
- Typed parameters and stable IDs.
- Sample-accurate automation/modulation delivery.
- Host parameter gestures and plugin-to-host changes.
- Versioned state and migrations.
- Process/transport context.
- Latency, tail, bypass, and host restart/change notifications.
- Realtime-safe telemetry/control primitives.
- Headless processing and state tests.
- Realtime allocation assertions.
- CLAP validation and lifecycle torture tests.

Exit condition: the conformance effect is robust enough to begin using Chassis from a real effect product without product-specific framework patches.

## Phase 2 — desktop product formats and GUI

- VST3 projection and validation.
- AUv2/AUv3 projection and validation on macOS.
- Cross-platform packaging on macOS, Windows, and Linux where the target format applies.
- Editor lifecycle, resize/scaling, and native parent-window contract.
- First production GUI adapter, likely Iced/wgpu after a focused evaluation.
- Generic parameter editor for framework testing/debugging.
- Pluginval, Steinberg validator, AUVal, and clap-validator gates where applicable.
- Signing/notarization hooks and release packaging.

Initial wrapper strategy may use `clap-wrapper`; wrapper behavior remains subject to Chassis's own host and validator matrix.

## Phase 3 — standalone deployment and common utilities

- Product-grade standalone runtime.
- Audio device configuration.
- MIDI device configuration.
- Shared editor/state path with plugin deployments.
- Standard parameter smoothing helpers with opt-out/custom semantics.
- Common preset serialization/storage primitives.
- Standard meter/telemetry channels.
- Optional analyzer/FFT transport helpers where repeated clients establish common requirements.

Standalone must run the same product processor/state implementation used by plugin deployments.

## Phase 4 — instruments as first-class clients

The core must be instrument-compatible earlier; this phase proves it with real products.

- Note/MIDI input as a primary processing source.
- No-audio-input generators/instruments.
- Multiple audio output buses.
- Note expression/MPE support as host/format capability allows.
- MIDI/event output.
- Instrument-oriented standalone behavior.
- Stress tests for high event counts and dynamic voice-driven processing behavior.

Potential optional helpers such as voice allocation belong outside the core unless multiple real instruments demonstrate a stable common abstraction.

## Phase 5 — advanced routing and immersive audio

- Generalized multi-bus routing validation.
- Surround channel layouts.
- Ambisonic layouts.
- Immersive/Atmos-style layout mapping where plugin formats and hosts expose the necessary semantics.
- Layout negotiation and dynamic layout changes where formats support them.
- Expanded offline-render and high-channel-count performance tests.

The initial stereo API must not require redesign to reach this phase.

## Phase 6 — hosting and audio-application runtime

Only pursue this if real application use justifies maintaining it.

Potential crates:

- `chassis-host`: plugin discovery/loading/hosting, likely using `clack-host` for CLAP.
- `chassis-device`: reusable audio/MIDI device runtime.
- `chassis-graph`: realtime processing graph and scheduling primitives.

These layers could support modular hosts, live processors, mastering software, or DAW-like applications. Chassis will still not define timeline, project, arrangement, media-library, mixer UX, mastering revision/QC, or delivery semantics.

## Later / conditional

- AAX distribution when product demand justifies Avid/PACE integration and licensing.
- LV2 if Linux ecosystem demand warrants it.
- MIDI 2.0 when host/platform support and Rust dependencies are sufficiently production-ready.
- Sandboxed/out-of-process plugin hosting if Chassis develops a host/application layer.
- Additional GUI adapters based on actual user demand.

## Product proof sequence

Once the framework reaches the appropriate phase, use increasingly demanding real clients:

1. Conformance effect — framework semantics only.
2. Mastering limiter — latency, automation, offline parity, realtime safety.
3. Tonal EQ — many parameters, state, polished custom GUI.
4. Compressor/dynamics processor — sidechain, metering, routing.
5. Instrument — notes, expression, multi-output, standalone.

Framework features should graduate from product-local code only when their semantics are broadly reusable.
