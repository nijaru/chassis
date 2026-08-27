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

The first CLAP adapter now exercises this ownership split directly. Clack requires its audio processor to be `Send`, so `chassis-clap` places `Send` on the concrete processor at the deployment boundary rather than adding it to `chassis-core::Processor` globally.

This resolves the basic ownership shape but does **not** freeze trait spelling/signatures.

Before promotion, still prove:

- cleanup of failed/partial activation through real adapter/host sequences;
- semantic whole-I/O policy beyond structural port presence;
- how the eventual framework `InstanceRuntime` adds canonical parameter/state authority without turning into a giant shared object;
- whether any additional lifecycle capability is genuinely required by CLAP/VST3/AU;
- f32/f64 capability advertisement/dispatch using one processor architecture.

Do not add derives/macros yet. The integration conformance effect and first CLAP export use this explicit API directly.

### Safe process buffer view

The core implementation exposes:

- exact in-place input/output with one mutable Rust slice;
- disjoint input/output views;
- input-only and output-only views;
- semantic stable port/channel endpoints;
- bounded explicit copy-to-output convenience for ordinary in-place DSP;
- callback slice-length validation without unsafe code in `chassis-core`.

The first CLAP proof maps Clack `ChannelPair::InputOutput` and `ChannelPair::InPlace` directly into those safe Chassis relationships. It uses a fixed stack `[ChannelBuffer; 2]` for the one-stereo-main proof and performs no process-time heap allocation in Chassis adapter code.

Remaining freeze gates:

- general adapter setup representation for arbitrary ports/channels without per-callback allocation or lifetime-erasing unsafe scratch;
- ergonomic higher-level port/bus views for stereo main, sidechain, multiple buses, and non-one-to-one routing;
- target-specific null/inactive/zero-buffer legality;
- f64 host dispatch/advertisement;
- benchmark copy/conversion paths before adding ownership/unsafe complexity to remove them.

Do **not** generalize the current proof by allocating `Vec<ChannelBuffer>` on every process call. If the current slice-shaped `ProcessBlock` makes a correct arbitrary-layout adapter awkward, revisit the core borrowing shape using adapter evidence rather than hiding the mismatch.

### Process context

`ProcessBlock<S>` currently carries:

- actual frame count;
- `ProcessMode` (`Realtime`, `BufferedRealtime`, `Offline` where supplied);
- borrowed safe channel views.

The initial CLAP adapter currently emits only `ProcessMode::Realtime` and does not register the CLAP render extension. This is an explicit limitation, not a claim that offline mode maps automatically.

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

## CLAP adapter — implemented proof, not qualified

### Clack dependency/version surface

The first adapter pins one exact safety-fixed Clack git revision:

```text
https://github.com/prokopyl/clack
c5975f9f89f0953b00768680357985d46178078a
```

The latest published release available during the audit, 0.1.1, has an acknowledged plugin-side reentrancy UB issue affecting real hosts including Bitwig and `clap-wrapper`. The fix merged after 0.1.1 and changes affected handler access from exclusive mutable to shared references. Since both Bitwig and `clap-wrapper` are relevant to Chassis, using 0.1.1 solely to stay on crates.io is not acceptable.

The selected revision is after the reentrancy fix and subsequent immediate safety/lifetime work. It is MIT/Apache-2.0 licensed and is allowlisted as the project's only current git-source exception. See `docs/research/clack-pinned-adapter-audit.md`.

Still required before production qualification:

- regenerate/review the exact git + transitive dependency graph in `Cargo.lock`;
- confirm Cargo resolves exactly the pinned revision;
- inspect the exact source used by the lockfile;
- deeper audit of Clack unsafe/lifetime and panic containment around the process/extension path;
- allocation/locking/reclamation audit of the exact process path;
- advisory/source/license checks after Cargo resolves the graph;
- move back to a suitable crates.io Clack release when a safety-fixed release is published and qualified.

Keep Clack types confined to `chassis-clap` and export/test glue.

### CLAP lifecycle mapping

The first implementation maps:

```text
factory/main-thread construction -> Chassis Component
CLAP activate                    -> Chassis activate
CLAP process                     -> Activated::process
CLAP reset                       -> Activated::reset
CLAP deactivate                  -> Activated::deactivate
```

Clack supplies the CLAP create/init/start/stop/destroy machinery around these types. The Chassis audio processor currently uses Clack's default no-op start/stop behavior because core has no separate semantic start/stop state yet.

Need actual evidence for:

- create/init/activate/start/process/stop/deactivate/destroy legal sequences;
- activation failure cleanup;
- repeated activation/deactivation;
- invalid/partial host sequences and panic containment;
- processor movement between CLAP audio threads (`Send`) without simultaneous mutation;
- reentrant host callbacks against the pinned Clack model;
- module unload once future callbacks/tasks exist.

The adapter existing in source is not evidence these sequences are qualified.

### Buffer mapping

The first proof intentionally supports exactly:

- one stereo main input;
- one stereo main output;
- f32;
- exact in-place or disjoint paired channels.

It rejects missing/asymmetric required main channels, wrong port counts, non-stereo main data, non-f32 data, invalid frame bounds, and Chassis callback-bound failures before product DSP proceeds.

Still required:

- sidechain/aux/multiple buses;
- stable setup-time endpoint mapping for arbitrary configurations;
- CLAP audio-port activation/configuration negotiation;
- f64;
- silence/constant-mask semantics if useful;
- synthetic malformed-buffer tests at the lowest safe boundary Clack exposes;
- proof that generalization retains bounded no-allocation callback work.

### Export/package/host qualification

`examples/clap-conformance` is now an rlib/cdylib export probe. It is not yet a packaged/validated `.clap` product artifact.

Before calling native CLAP support usable:

- local fmt/test/clippy/deny/machete all green with updated lockfile;
- build the export on a supported platform;
- package it according to CLAP platform conventions;
- run current `clap-validator` and lifecycle/buffer stress;
- smoke-test at least one real CLAP host, preferably including Bitwig while qualifying reentrancy-sensitive behavior;
- record exact validator/host/toolchain versions.

### Event/parameter translation

Still unimplemented. Prove CLAP's sample-sorted event stream, parameter values/modulation, note/event ports, state streams, and optional render mode without forcing CLAP-specific semantics into core.

## Blocks VST3/AU claims

### `clap-wrapper` qualification

Treat VST3/AU projection as provisional until the same conformance component passes:

- native validators;
- state/identity fixtures;
- buffer aliasing/layout tests;
- automation trajectory tests;
- editor lifetime/resize tests;
- real host matrix.

The known Clack 0.1.1 reentrancy issue explicitly cited `clap-wrapper`; keep the pinned safety-fixed Clack source (or a future qualified release) as a prerequisite for this projection path.

Replace/patch/own a native adapter only when a concrete wrapper limitation justifies it.

### Export identity manifest

Finalize manifest syntax and generate-once/freeze behavior before any product ships. Import paths for legacy IDs and byte-order golden fixtures are required.

The current `ClapStereoEffect::{CLAP_ID, CLAP_NAME}` contract is temporary adapter-proof metadata and must not become the long-term duplicated identity source merely because it exists first.

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

The current input-only/output-only buffer forms are necessary groundwork, not an instrument-support claim. The fixed stereo CLAP proof is deliberately not an instrument API.

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
