# Common Runtime Services

Status: design direction; public API not frozen.

## Goal

Provide common realtime/control infrastructure when the same semantics recur across effects and instruments, without making every supported service part of the initial core or hiding lifecycle costs.

Likely common areas are latency/tail metadata, bypass integration, realtime telemetry/control, and eventually background preparation. Each service needs one owner, explicit lifetime, and a product that does not use it should pay essentially no runtime cost.

## Latency

Latency belongs to an accepted/active processing configuration, not to an unconstrained mutable integer on the audio thread.

Use samples as the canonical semantic unit:

```text
LatencySamples(u32)
```

Adapters convert where a backend requires another representation.

If a change requires different host-visible latency, Chassis coordinates the adapter-specific reconfiguration/restart path before the new latency becomes authoritative. Product DSP must not call arbitrary host APIs from `process`.

Prefer stable/worst-case latency across ordinary realtime parameter changes when that is sonically/CPU practical; dynamic latency is a capability, not a convention to encourage.

## Tail

Tail is distinct from latency. A likely common semantic model is:

```text
Tail::None
Tail::FiniteSamples(u64)
Tail::Infinite
```

Tail reporting is metadata about potential output after input stops. It does not replace the product's own silence/activity logic.

## Bypass

Normalize host-visible bypass identity/timing where formats support it, but do not force one bypass DSP algorithm.

Product policy may require transparent copy, latency preservation, tail preservation, a crossfade, or custom processing. Reusable helpers can graduate into Chassis after repeated products demonstrate stable semantics.

Host bypass is not the same thing as a product A/B switch, module bypass, or dry/wet control.

## Processing activity hints

Do not make CLAP-style scheduling return values part of the first core `Processor` contract merely because CLAP exposes them. VST3/AU do not share the same model.

Initially, a processor can conservatively remain active. If real products and adapters benefit from a portable activity semantic, define the smallest cross-format abstraction then and let adapters degrade conservatively where unsupported.

## Realtime telemetry

Meters, gain reduction, scopes, analyzers, and diagnostics commonly need audio -> UI/control communication.

Two reusable classes are likely:

### Latest-value publication

For observations where intermediate samples may be dropped intentionally, such as peak/RMS, gain reduction, voice counts, or counters. Use atomics or another measured constant-time representation whose ownership is explicit.

### Ordered stream publication

For scopes, analyzer frames, or event traces. Use a bounded queue with a policy declared by the producer/consumer contract. Display telemetry commonly drops/coalesces when full; the audio thread never waits for the UI.

Capacity is not an arbitrary magic framework constant. It is selected from the update rate, consumer cadence, payload size, and acceptable loss/latency, then allocated before realtime processing.

## Control -> realtime publication

Structural DSP changes may need prepared non-RT data installed at a block boundary: FIR kernels, impulse responses, wavetables, sample maps, or compiled processing structures.

The preferred shape is immutable prepared data plus a typed publication mechanism. Preparation/allocation occurs off the audio thread; adoption on the processor is bounded.

The design must name:

- who owns the currently active object;
- who owns a pending replacement;
- what happens if another replacement arrives first;
- how an instance generation prevents stale work from publishing after reset/reload/destruction;
- where the replaced object is finally destroyed.

Never let `Arc`/box/reference-count release accidentally perform an unbounded destructor on the audio thread. Deferred reclamation is part of the abstraction, not an afterthought.

Avoid an untyped `Box<dyn Any>` queue as the primary mechanism.

## Background preparation

Background work is useful for some products—IR/sample decoding, expensive kernel construction, analysis, resource indexing—but it is **not required by most simple effects**, so it is not a v0.1 core prerequisite.

When Chassis adds a task abstraction, its lifetime must be designed before its executor implementation.

### No implicit process-global executor

Do not create a process-global fallback thread pool merely because sharing threads is efficient. In a plugin dynamic library, process/module unload, last-instance teardown, callbacks into unloaded code, and worker joining become correctness concerns.

Acceptable future directions include:

- use a host-provided worker/thread-pool capability when its lifecycle contract is sufficient;
- a Chassis module/runtime object whose lifetime is explicitly tied to loaded plugin code and which shuts down only after all instances/tasks are gone;
- an instance-owned worker for a product that explicitly accepts that cost;
- standalone/application-owned execution supplied by the outer application.

The product-facing task API should not require knowing which executor backs it, but Chassis must not hide an executor whose ownership cannot be stated.

### Task generations and cancellation

Every task belongs to an instance/generation. Replacement, state load, deactivation, or destruction defines whether the task remains valid.

A completion must prove it still targets the current generation before publication. Cancellation is not equivalent to guaranteed immediate termination; teardown must remain safe even if a task notices cancellation late.

No task calls directly into freed editor/MainThread/Processor state. Results publish through owned handles/snapshots/messages whose receiver may reject stale generations.

Offline rendering cannot depend on wall-clock races. Data required for deterministic output must be prepared before rendering or behind an explicit non-RT barrier.

## Main-thread scheduling

Adapters may expose a semantic main-thread scheduling capability where the host/platform supports it. The adapter owns the definition of the host's legal main/control thread; product code must not guess from OS thread IDs.

Main-thread scheduling is useful for host notifications, editor/control updates, and accepting completed background work into canonical state.

## Diagnostics

Non-RT code can use ordinary structured logging. Realtime diagnostics use preallocated/bounded counters or events and defer formatting/I/O.

Do not synchronously format/log from the audio callback.

FFI adapters need containment for panics or invalid host input so unwinding never crosses a foreign ABI. The exact fallback—silence/discard/status/diagnostic—belongs to the format contract and must be tested.

## Denormals / floating-point environment

Denormal handling may become an optional process guard after cross-platform measurement. It changes execution environment and should not be enabled by folklore.

Likewise, do not use new algebraic/fast-math-style floating point operations merely because current Rust exposes them. Audio DSP that depends on numerical reproducibility or exact transfer behavior needs explicit evidence before allowing reassociation.

## Initial implementation boundary

The first conformance processing path needs only:

- fixed latency/tail metadata if required by the adapter proof;
- one minimal latest-value telemetry primitive if a test needs audio -> control observation;
- one bounded typed control/publication primitive if a test needs control -> audio transfer.

Background executors, dynamic host notifications, bypass DSP helpers, richer diagnostics, denormal guards, analyzer streams, and generalized reclamation should be added when a concrete client exercises their lifecycle.
