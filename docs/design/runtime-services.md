# Common Runtime Services

Status: design direction; public API not frozen.

## Goal

Provide common realtime/control infrastructure that a large fraction of effects and instruments need, while keeping product DSP semantics owned by the product.

This includes host-visible latency/tail state, bypass integration, process activity status, bounded realtime communication, background work, and diagnostics hooks.

## Latency

Latency is a property of the active processing configuration, not an arbitrary value the audio thread mutates without coordination.

Use samples as the canonical Chassis unit:

```text
LatencySamples(u32)
```

Adapters convert to target units such as Audio Unit seconds where necessary.

A processor receives the resolved latency in its activation configuration. If a parameter/state change requires a different latency, the framework should coordinate the host-specific restart/reactivation path before the new latency becomes active.

Products should prefer a stable/worst-case latency across ordinary realtime parameter changes where practical. Apple explicitly notes that variable latency is difficult for hosts to compensate safely. Chassis should not encourage dynamic latency merely because one format can signal it.

Common product patterns:

- zero latency;
- fixed latency derived from sample rate/configuration;
- fixed maximum/worst-case latency while a lookahead/quality parameter changes internally;
- latency change requiring host restart/reactivation.

The framework can provide a control-domain request such as `request_reconfigure`/`latency_changed`, but the audio processor must not directly call arbitrary host APIs.

## Tail

Tail semantics are separate from latency.

Canonical model should support at least:

```text
Tail::None
Tail::FiniteSamples(u64)
Tail::Infinite
```

Adapters translate to seconds/samples/format-specific infinite values.

A reverb or delay may change tail duration with parameters. The framework should provide a non-RT host notification path where the target format supports tail changes.

Tail reporting is metadata about potential output after input goes silent; it does not replace product DSP's own silence/activity detection.

## Process activity/status

The semantic status returned from `Processor::process` should be small and portable.

Likely core statuses:

```text
Continue
ContinueIfNotQuiet
Tail
Sleep
```

These map naturally to CLAP. Formats that do not expose equivalent scheduling hints can safely map them to successful processing while using latency/tail metadata where appropriate.

`Sleep` is an optimization hint/capability, not permission to discard required future output. Products that do not implement correct silence/tail detection can always return `Continue`.

A process failure should not be the normal control-flow mechanism. Activation/configuration validation should make most failures impossible once realtime processing starts. Adapter boundaries still need a defined fail-safe for panic/invalid host data, typically discard/silence output and report diagnostics without unwinding across FFI.

## Bypass

Bypass exists in multiple host APIs and is common enough for Chassis to normalize, but Chassis should not force one audio crossfade/DSP bypass algorithm.

A product can declare a canonical bypass control/capability. Adapters expose it through the target format's preferred mechanism (for example a bypass-marked parameter or Audio Unit bypass property) where possible.

The processor receives bypass changes with the same timing guarantees as other process-time control changes when the host provides them.

Product policy decides what bypass means sonically:

- transparent copy/input routing;
- latency-preserving bypass;
- tail-preserving bypass;
- wet processing suspended after transition;
- custom crossfade to avoid discontinuities.

Chassis should provide reusable latency-preserving/crossfade helpers later if repeated real products show a stable common implementation, but bypass DSP is not hard-coded into core.

Host bypass must not be conflated with a product's separate A/B, module bypass, or dry/wet controls.

## Realtime telemetry

Meters, gain reduction, scopes, analyzers, voice counts, and diagnostics often need audio-thread -> UI/control communication.

Chassis should provide standard bounded primitives rather than each product creating ad hoc atomics/ring buffers.

Two common classes:

### Latest-value telemetry

For values where only the newest observation matters:

- peak/RMS meter values;
- gain reduction;
- playhead-derived display values;
- CPU/voice counters.

Use atomic/latest-value publication where the type can be represented safely and cheaply. The consumer can miss intermediate values by design.

### Stream telemetry

For ordered sample/block data:

- waveform/scope points;
- analyzer input frames;
- event traces.

Use a bounded single-producer/single-consumer or otherwise explicitly modeled queue. Queue-full behavior is part of the API, typically dropping/coalescing display-only telemetry rather than ever blocking the audio thread.

Do not pretend display telemetry is lossless realtime transport unless the product explicitly needs and provisions that guarantee.

## Control messages to realtime

Non-parameter structural DSP changes may need control/main/background -> processor delivery.

Examples:

- swap a newly built FIR kernel;
- install a decoded impulse response;
- replace a wavetable/sample map snapshot;
- switch a compiled processing graph that does not require host reactivation.

The conventional model should publish immutable/prepared data across a bounded block-boundary mechanism. Preparation/allocation happens off-thread; the processor performs only bounded pointer/handle adoption at a defined safe point.

Avoid a generic `Box<dyn Any>` message queue that hides allocation, type mismatches, or unbounded ownership destruction on the realtime thread.

Likely implementation directions include typed bounded mailboxes and/or immutable snapshot handles. Exact primitives should be selected after the conformance component exercises realistic swaps.

Destruction of a large replaced object must not accidentally occur on the audio thread. A reclamation/deferred-drop path is part of any snapshot-swap abstraction.

## Background work

Background work is common enough for a framework convention:

- file/sample/IR decoding;
- expensive coefficient/filter/kernel construction;
- analysis;
- preset/resource indexing;
- non-realtime network/licensing operations in optional product layers;
- deferred destruction/reclamation.

The initial abstraction should expose a bounded task service to `MainThread`/control code and, for carefully defined fixed task types, a realtime-safe dispatch path.

Implementation strategy may vary by deployment:

```text
CLAP plugin
  -> host thread-pool extension when suitable/available
  -> Chassis fallback executor otherwise

VST3/AU projection
  -> Chassis process-wide fallback executor unless wrapper/native host service exists

standalone/application
  -> Chassis-owned executor/runtime
```

The product API should not depend on whether a particular host supplied the workers.

### Executor scope

Do not create an unbounded dedicated thread pool per plugin instance by convention. A DAW may instantiate hundreds of plugins.

A process-wide/shared Chassis executor with per-instance bounded ownership/cancellation is the preferred fallback direction. Products needing a dedicated realtime-adjacent thread can opt into an explicit specialized service later.

### Cancellation/lifetime

Tasks belong to an instance/domain and must not call into destroyed plugin state.

Framework task handles should support:

- cancellation at teardown/state replacement;
- completion publication through safe handles;
- no callback into freed `MainThread`/editor objects;
- deterministic behavior for offline rendering where asynchronous completion would otherwise make output depend on wall-clock scheduling.

Offline product DSP must not assume an async task happens to finish before a future block. Required render data must be prepared before processing or use a deterministic synchronous/non-RT preparation barrier.

## Main-thread scheduling

Adapters should expose a semantic `schedule_main`/callback facility where the host/platform can provide it. This is needed for:

- editor/control updates;
- host notifications that are main-thread-only;
- completing background operations that change canonical state.

If a target host has unusual "logical main thread" behavior through a wrapper, the adapter owns that compatibility issue. Product code should not inspect OS thread IDs to guess host legality.

## Logging and diagnostics

Chassis should eventually provide structured diagnostics with two paths:

- ordinary non-RT logging through Rust/application/host logging integrations;
- realtime-safe diagnostics via bounded/preallocated events/counters.

Never route arbitrary formatted logging synchronously from the audio callback. Formatting/IO may allocate or block.

Format adapters can bridge non-RT diagnostics to host logging facilities such as CLAP's log extension where useful.

The validation build may enable stricter diagnostics and invariant counters than production release builds.

## Timers / file-descriptor callbacks

CLAP exposes host timer and POSIX-FD support, but these are not universal plugin concepts and should not be mandatory core services.

A later capability layer can expose timers/event-loop integration where a product or GUI needs them, with adapters mapping when available and falling back to toolkit/platform mechanisms when appropriate.

## Denormals / floating-point environment

Denormal handling is a broadly useful DSP concern but modifies execution environment rather than product semantics.

Chassis should evaluate an optional/default process-call guard that disables costly denormal behavior on CPU architectures where that is conventional and safe, restoring host state afterward if required.

Do not freeze this behavior without cross-host/architecture testing. Products remain responsible for algorithms that need unusual floating-point environment semantics.

## Initial implementation boundary

For the first conformance effect, implement only what is needed to prove the contracts:

- fixed latency/tail reporting;
- process status;
- one latest-value telemetry primitive;
- one bounded control/telemetry queue path;
- a simple background task -> safe completion/snapshot path.

Dynamic host notifications, bypass helpers, richer logging, denormal guards, host thread-pool integration, and advanced snapshot reclamation can follow once the base lifecycle is validated.
