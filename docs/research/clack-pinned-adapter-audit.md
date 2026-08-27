# Pinned Clack Adapter Audit

Status: implementation-time audit for the first CLAP vertical slice; not a production qualification.

## Selected source

Chassis currently pins the Clack repository at:

```text
https://github.com/prokopyl/clack
c5975f9f89f0953b00768680357985d46178078a
```

That commit is the 2026-08-05 development-version bump to 0.2.0 after the plugin-side reentrancy fix and the immediately following safety/lifetime changes. The relevant Clack workspace is Edition 2024, declares Rust 1.85 MSRV for its publishable crates, and uses `MIT OR Apache-2.0` licensing.

The manifest uses a full `rev`, not a branch/tag, so Cargo resolves one source commit. `deny.toml` allowlists only the Clack repository as the project's current git-source exception.

## Why Chassis does not use published 0.1.1

The latest published crates.io release available during this audit is Clack 0.1.1 from 2026-07-29.

Clack issue #89, "UB when hosts call CLAP API re-entrantly," documents that the plugin-side API incorrectly assumed CLAP callbacks could not be re-entered. Because handlers received exclusive `&mut self`, synchronous host re-entry could create overlapping mutable references and undefined behavior. The issue explicitly cites Bitwig and `clap-wrapper` as real-world reentrant callers.

This is directly relevant to Chassis:

- Bitwig is an intended CLAP qualification host;
- `clap-wrapper` is the preferred initial VST3/AU projection path;
- a known aliasing/UB defect in the trust boundary conflicts with Chassis's safety criteria.

PR #90 / commit `e8c956ae5c2ebabfee4abf28200fc66b484c47b3` fixed the plugin-side model by changing affected handler access from exclusive `&mut` to shared `&self` and relying on explicit interior mutability where mutation is actually needed.

Therefore Chassis accepts the supply-chain cost of one fully pinned git dependency rather than knowingly using 0.1.1. This is safety-driven, not a desire to track unreleased features.

## Additional post-0.1.1 safety work included by the pin

The selected revision also comes after several immediately subsequent hardening changes, including:

- plugin-side reentrancy fix;
- host-side reentrancy work;
- APIs such as cookie setters/SysEx construction being marked unsafe where required;
- GUI/window-handle lifetime tightening;
- development version bump to 0.2.0.

These changes still require source review where Chassis depends on the affected paths; inclusion by revision is not itself proof of correctness.

## Thread model fit

The pinned Clack API models:

- audio processor as `Send`, processed exclusively through `&mut`;
- main-thread state as neither `Send` nor `Sync`;
- shared state as `Send + Sync`.

This maps to Chassis's deployment-boundary rule:

- `chassis-core::Processor` is not globally `Send` because same-thread embedded runtimes are valid;
- `chassis-clap` requires the concrete `C::Processor: Send` because CLAP may move the audio processor among host audio threads;
- processing remains exclusive; Chassis does not require `Sync` for product DSP state.

The post-reentrancy Clack API gives shared references to main-thread handlers that may be re-entered. Chassis's current adapter only reads its `ChassisMainThread<C>` component during audio-port queries/activation, so this is a clean match.

## Buffer boundary

The pinned Clack safe audio API differentiates:

- `ChannelPair::InputOnly`;
- `ChannelPair::OutputOnly`;
- `ChannelPair::InputOutput` for disjoint input/output;
- `ChannelPair::InPlace` for exact aliasing represented by one mutable slice.

That maps directly to Chassis `ChannelBuffer` relationships without Chassis handling raw CLAP pointers in the current slice.

The first adapter intentionally supports only one required stereo main input/output pair. It turns two Clack `ChannelPair<f32>` values into a fixed `[ChannelBuffer; 2]` on the stack and calls the existing Chassis runtime. There is no explicit process-time heap allocation in Chassis's translation code.

This is a proof, not the general multibus solution. Arbitrary ports/channels must not be implemented by allocating a `Vec<ChannelBuffer>` every callback or by introducing lifetime-erasing unsafe scratch just to preserve the current `ProcessBlock` shape. Adapter evidence should determine whether that core borrowing shape needs refinement.

## Current semantic mapping

Implemented in the first slice:

```text
CLAP factory/init            -> construct Chassis component on main thread
CLAP activate                -> ProcessConfig + Chassis activate
CLAP process (f32 stereo)    -> ChannelPair -> ChannelBuffer -> Activated::process
CLAP reset                   -> Activated::reset
CLAP deactivate              -> Activated::deactivate
CLAP audio-ports extension   -> one stereo main input + one stereo main output
```

Clack owns CLAP create/init/start/stop/destroy scaffolding. Chassis currently uses Clack's default no-op start/stop because core has not established a separate semantic start/stop requirement.

Not implemented yet:

- sidechain/aux/multiple buses;
- configurable audio ports/layout negotiation;
- f64 processing/advertisement;
- render/offline extension;
- parameters, automation, modulation, events, MIDI/notes, transport;
- state;
- latency/tail/sleep/bypass semantics;
- GUI;
- bundle/install tooling;
- CLAP validator or real-host qualification.

The adapter returns `ProcessStatus::Continue` conservatively and maps each call to Chassis `ProcessMode::Realtime`. These are explicit temporary semantics.

## Failure containment

The Chassis adapter rejects before product DSP when:

- CLAP activation frame bounds are zero, exceed the CLAP signed 32-bit limit, or fail Chassis validation;
- process data does not expose exactly one input and one output port for this proof;
- the main port is not exactly stereo;
- f32 buffers are unavailable;
- a required main channel is input-only or output-only;
- Chassis callback bounds reject the block.

Clack handles the raw C ABI and constructs safe audio views. Chassis must still audit Clack's internal unsafe implementation and panic/FFI containment before calling the backend production-qualified.

## Promotion gates

Before this slice becomes the basis for real product exports:

1. let Cargo resolve the pinned git source and regenerate/review `Cargo.lock`;
2. run fmt/test/clippy/deny/machete locally;
3. inspect the exact resolved Clack/transitive tree and source revision;
4. compile the `chassis-clap-conformance` rlib/cdylib;
5. package a real `.clap` artifact for the current platform;
6. run `clap-validator` plus lifecycle/buffer stress;
7. smoke-test at least Bitwig or another real CLAP host, with reentrant behavior in mind;
8. decide the general port/channel borrowing model before arbitrary multibus support;
9. add render-mode semantics and f64 only when their mappings are explicit;
10. migrate back to a crates.io Clack release when a suitable safety-fixed release is published and validated.
