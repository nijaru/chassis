# Open Questions and API Freeze Gates

This document records unresolved design choices that are intentionally **not** compatibility promises yet. Resolve them through the conformance runtime, adapter work, and representative products rather than by adding abstraction speculatively.

## Blocks the first explicit runtime API

### Component / Processor construction

Need the smallest explicit trait/type surface that proves:

- immutable product schema/identity versus per-instance runtime state;
- framework-owned canonical parameter/state authority;
- exclusive active `Processor` ownership;
- optional product `MainThread`/`Shared` without boilerplate;
- activation/deactivation that can map faithfully to CLAP and later VST3/AU;
- cleanup of partial/failed activation.

Do not add derives/macros until this manual API is used by the conformance component.

### Safe process buffer view

Need an implementation that can expose:

- exact in-place channels with one mutable Rust reference;
- disjoint input/output channels;
- input-only/output-only ports;
- multiple buses;
- validated frame/channel bounds;
- `f32` and eventual `f64` without duplicating product architecture.

Adapters must prove aliasing before constructing references. Benchmark any copy/conversion path before adding unsafe/ownership complexity to remove it.

### Process context

Need the per-call type that carries at least:

- actual frame count;
- `ProcessMode` (`Realtime`, `BufferedRealtime`, `Offline` where supplied);
- block-start transport snapshot;
- parameter trajectories/events;
- narrow realtime-safe output/host capabilities.

Do not encode CLAP process status or VST3 classes directly into this type.

## Blocks the first state/automation API

### Canonical parameter-store mechanics

The semantic authority is decided; the implementation is not.

Need to prove:

- how UI/host edits update base state;
- how per-block automation trajectories are constructed without polling every parameter/sample;
- how the post-automation current base value becomes observable;
- how a multi-parameter state load publishes as one generation;
- how state save while active obtains its documented cross-parameter consistency without blocking realtime;
- memory-ordering/ownership/reclamation if atomics or snapshots are used.

Do not default to “one atomic per parameter” until the state-save/load contract is demonstrated.

### State wire format v1

The current Chassis-owned envelope is a prototype.

Freeze only after:

- bounded parser/encoder implementation;
- wrong-product rejection;
- deterministic golden fixtures;
- fuzz/property/corruption/exhaustion tests;
- migration fixture;
- partial stream I/O tests;
- cross-format state round trips.

Changing the format before v1 is expected.

## Blocks CLAP production proof

### Clack dependency/version surface

Audit the exact Clack release used, including:

- unsafe/lifetime boundary;
- allocation/locks on process path;
- extension coverage needed by Chassis;
- thread-domain model;
- transitive dependency/license/source tree.

Keep Clack types confined to `chassis-clap`.

### CLAP lifecycle mapping

Prove create/init/activate/start/process/stop/deactivate/destroy and invalid/partial sequences against the Chassis runtime state machine.

Need explicit containment for adapter panics/invalid host input and no unwind across ABI.

### Event/parameter translation

Prove CLAP's sample-sorted event stream, parameter values/modulation, note/event ports, state streams, and optional render mode without forcing CLAP-specific semantics into core.

## Blocks VST3/AU claims

### `clap-wrapper` qualification

Treat VST3/AU projection as provisional until the same conformance component passes:

- native validators;
- state/identity fixtures;
- buffer aliasing/layout tests;
- automation trajectory tests;
- editor lifetime/resize tests;
- real host matrix.

Replace/patch/own a native adapter only when a concrete wrapper limitation justifies it.

### Export identity manifest

Finalize manifest syntax and generate-once/freeze behavior before any product ships. Import paths for legacy IDs and byte-order golden fixtures are required.

## GUI freeze gates

Do not make Iced or another toolkit part of the core contract prematurely.

Before a first-class adapter:

- prove parent window attach/detach across target hosts;
- resize/scale/high-DPI lifecycle;
- editor recreation/teardown;
- parameter gestures and observations;
- realtime telemetry ownership;
- accessibility path where practical;
- custom rendering/control flexibility sufficient for commercial plugin UIs.

## Background execution / snapshots

No hidden global executor yet.

Before adding a framework task service, specify:

- instance/module/executor lifetime;
- plugin dylib unload behavior;
- task generation and stale-completion rejection;
- cancellation versus join/final shutdown;
- ownership of large result/replaced-object destruction;
- deterministic offline behavior.

Typed immutable control->DSP snapshot publication may be implemented separately if a concrete FX needs it before a general task service.

## Instruments

Core must remain compatible now, but instrument helpers wait for a real instrument/conformance extension.

Before stable instrument claims, prove note identity, expression, MIDI fallback, multiple event/audio outputs, output-event capacity/rejection, and standalone behavior.

## Surround / ambisonics / immersive

Do not freeze a closed speaker enum. Before stable multichannel APIs, define extensible speaker positions, ambisonic ordering/normalization, mapping fixtures, and whole-layout negotiation.

Channel-based immersive beds are distinct from Dolby Atmos object/metadata workflows.

## Standalone / host / application layers

Standalone should run the same component/processor/state implementation, but device/window ownership is a later layer.

General plugin hosting, device runtime, and processing graph crates are conditional on a real application. Do not let their hypothetical needs complicate plugin v0.1 unless the core would otherwise make them impossible.

## Licensing/governance before public contributions/commercial licenses

Before accepting substantive outside code:

- establish contributor/relicensing terms (likely a concise CLA);
- decide commercial license terms;
- review notices/attribution generation;
- verify all distributed dependencies remain compatible with both AGPL and proprietary licensing.

This does not block private framework development.
