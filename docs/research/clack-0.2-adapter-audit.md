# Clack 0.2 Initial Adapter Audit

Status: implementation-time audit for the first CLAP vertical slice; not a production qualification.

## Version and source

The first Chassis CLAP adapter targets crates.io `clack-plugin = 0.2.0` and `clack-extensions = 0.2.0` with exact version requirements.

The corresponding current Clack workspace declares:

- Edition 2024;
- Rust 1.85 MSRV for the plugin/common/extensions crates;
- `MIT OR Apache-2.0` licensing;
- `clap-sys = 0.5.0`;
- `bitflags = 2.11.0` in the common/extensions layer.

Before production qualification, compare the exact registry source selected by `Cargo.lock` with the source reviewed here rather than assuming a same-version development branch is byte-identical.

## Why Clack still fits

Clack explicitly models the CLAP thread domains:

- audio processor is `Send` but processed exclusively through `&mut`;
- main-thread state need not be `Send`/`Sync`;
- shared state is `Send + Sync`.

This maps cleanly to the Chassis rule that `Processor` itself is not globally `Send`, while the CLAP deployment boundary requires the concrete processor to be `Send` because a host may move it between audio threads.

The first adapter therefore places `Send` on `C::Processor` in `chassis-clap`, not on `chassis-core::Processor`.

## Buffer boundary

Clack's safe audio API already differentiates:

- `ChannelPair::InputOnly`;
- `ChannelPair::OutputOnly`;
- `ChannelPair::InputOutput` for disjoint input/output;
- `ChannelPair::InPlace` for exact aliasing represented by one mutable slice.

That maps directly to Chassis `ChannelBuffer` relationships without Chassis handling raw host pointers in this slice.

The first adapter intentionally supports only one required stereo main input/output pair. It turns the two Clack `ChannelPair<f32>` values into a fixed `[ChannelBuffer; 2]` on the stack and calls the existing Chassis runtime. There is no process-time heap allocation in Chassis's translation code.

This is a proof, not the general multibus solution. Arbitrary ports/channels must not be implemented by allocating a `Vec<ChannelBuffer>` every callback or by introducing lifetime-erasing unsafe scratch merely to preserve the current slice API. The next multibus design pass should use adapter evidence to decide whether core buffer access needs a different borrowing shape.

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

Not implemented yet:

- sidechain/aux/multiple buses;
- configurable audio ports/layout negotiation;
- f64 processing/advertisement;
- render/offline mode extension;
- parameters, automation, modulation, events, MIDI/notes, transport;
- state;
- latency/tail/sleep/bypass semantics;
- GUI;
- bundle/install tooling;
- CLAP validator or real-host qualification.

The adapter currently returns `ProcessStatus::Continue` conservatively and maps every call to Chassis `ProcessMode::Realtime`. Those are explicit temporary semantics, not final framework behavior.

## Failure containment

The adapter rejects before product DSP when:

- CLAP activation frame bounds are zero, exceed the CLAP signed 32-bit limit, or fail Chassis validation;
- the process callback does not contain exactly one input and one output port for this proof;
- the main port is not exactly stereo;
- the host supplies no f32 buffers;
- a required main channel is input-only or output-only;
- Chassis callback bounds reject the block.

Clack handles the raw C ABI and constructs its safe audio views. Chassis should still audit Clack's internal unsafe implementation and panic/FFI containment before calling the backend production-qualified.

## Promotion gates

Before this slice becomes the basis for real product exports:

1. regenerate/review `Cargo.lock` and run fmt/test/clippy/deny/machete locally;
2. compile the `chassis-clap-conformance` rlib/cdylib;
3. package a real `.clap` artifact for the current platform;
4. run `clap-validator` plus lifecycle/buffer stress;
5. smoke-test in at least one real CLAP host;
6. decide the general port/channel borrowing model before adding arbitrary multibus support;
7. add render-mode semantics and f64 only when their mappings are explicit.
