# Open Questions and API Freeze Gates

This document records unresolved design choices that are intentionally **not** compatibility promises yet. Resolve them through the conformance runtime, adapter work, and representative products rather than by adding abstraction speculatively.

## First explicit runtime slice — implemented, not frozen

### Component / Processor construction

The first manual API now exists:

- `Component` is the immutable definition/factory;
- framework activation structurally validates I/O before calling product activation;
- `Processor` exclusively owns active DSP/runtime history and reset;
- sample processing is a separate `Process<S>` capability;
- `Activated<P>` owns the processor + immutable activation config and consumes itself on deactivation.

This resolves the basic ownership shape but does **not** freeze trait spelling/signatures.

Before promotion, still prove:

- cleanup of failed/partial activation through real adapter sequences;
- semantic whole-I/O policy beyond structural port presence;
- how the eventual framework `InstanceRuntime` adds canonical parameter/state authority without turning into a giant shared object;
- whether any additional lifecycle capability is genuinely required by CLAP/VST3/AU;
- f32/f64 capability advertisement/dispatch using one processor architecture.

Do not add derives/macros yet. The integration conformance effect uses this explicit API directly; the first CLAP adapter should be the next major ergonomic test.

### Safe process buffer view

The first implementation now exposes:

- exact in-place input/output with one mutable Rust slice;
- disjoint input/output views;
- input-only and output-only views;
- semantic stable port/channel endpoints;
- bounded explicit copy-to-output convenience for ordinary in-place DSP;
- callback slice-length validation without unsafe code in `chassis-core`.

Remaining freeze gates:

- adapter setup representation that resolves stable endpoints to host buffers without per-callback allocation/O(n²) lookup;
- ergonomic higher-level port/bus views for stereo main, sidechain, multiple buses, and non-one-to-one routing;
- target-specific null/inactive/zero-buffer legality;
- f64 host dispatch/advertisement;
- benchmark copy/conversion paths before adding ownership/unsafe complexity to remove them.

Adapters must prove aliasing before constructing safe references.

### Process context

`ProcessBlock<S>` currently carries:

- actual frame count;
- `ProcessMode` (`Realtime`, `BufferedRealtime`, `Offline` where supplied);
- borrowed safe channel views.

Still to add only when the corresponding semantic layer is implemented:

- block-start transport snapshot;
- parameter trajectories/events;
- note/MIDI/event views/sinks;
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

The current `Activated` shell is a semantic proof, not evidence that CLAP lifecycle mapping is already correct.

### Buffer mapping

The CLAP adapter must prove its host-pointer validation and endpoint resolution before constructing `ChannelBuffer` views:

- exact alias versus disjoint ranges;
- no overlapping output references;
- active port/channel counts and frame bounds;
- setup-time dense mapping with no hidden per-callback allocation;
- invalid host data contained before safe Rust references exist.

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

The current input-only/output-only buffer forms are necessary groundwork, not an instrument-support claim.

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
