# Clack 0.1.1 Initial Adapter Audit

Status: implementation-time audit for the first CLAP vertical slice; not a production qualification.

## Version and source

The first Chassis CLAP adapter targets the published crates.io releases:

- `clack-plugin = 0.1.1`;
- `clack-extensions = 0.1.1`.

They are exact requirements while the adapter is pre-alpha and being qualified.

The crates.io index records both 0.1.1 releases as published on 2026-07-29 with Rust 1.85 MSRV. Clack's 0.1.1 source at commit `82498dbfde2f8a16c7bb0e34389826089dbee5cf` uses Edition 2024 and `MIT OR Apache-2.0`. The dependency graph uses `clack-common 0.1.1`, `clap-sys ^0.5.0`, and `bitflags ^2.11.0` for the extension/common layer.

Clack's repository bumped development versions to 0.2.0 on 2026-08-05, but 0.2.0 is not currently published in the crates.io index. Chassis deliberately uses 0.1.1 rather than introducing a git dependency solely for unreleased API changes.

Before production qualification, inspect the exact registry source selected by `Cargo.lock`; this document does not substitute for reviewing the downloaded crate source/checksum.

## Relevant 0.1.1 versus unreleased 0.2 difference

The July 30 Clack reentrancy work after 0.1.1 changed several plugin main-thread accesses from exclusive to shared references. In published 0.1.1:

- `PluginAudioProcessor::activate` receives `&mut MainThread`;
- `PluginAudioProcessor::deactivate` receives `&mut MainThread`;
- `PluginAudioPortsImpl::{count,get}` receive `&mut self`.

Chassis's adapter follows the published 0.1.1 signatures. The Chassis component itself is only read during these callbacks, so moving to a later shared-reference Clack API should not require a Chassis semantic change.

## Why Clack fits

Clack 0.1.1 explicitly models the CLAP thread domains:

- audio processor is `Send` but processed exclusively through `&mut`;
- main-thread state need not be `Send`/`Sync`;
- shared state is `Send + Sync`.

This maps cleanly to the Chassis rule that `Processor` itself is not globally `Send`, while the CLAP deployment boundary requires the concrete processor to be `Send` because a host may move it between audio threads.

The first adapter therefore places `Send` on `C::Processor` in `chassis-clap`, not on `chassis-core::Processor`.

## Buffer boundary

Clack's safe audio API differentiates:

- `ChannelPair::InputOnly`;
- `ChannelPair::OutputOnly`;
- `ChannelPair::InputOutput` for disjoint input/output;
- `ChannelPair::InPlace` for exact aliasing represented by one mutable slice.

That maps directly to Chassis `ChannelBuffer` relationships without Chassis handling raw host pointers in this slice.

The first adapter intentionally supports only one required stereo main input/output pair. It turns the two Clack `ChannelPair<f32>` values into a fixed `[ChannelBuffer; 2]` on the stack and calls the existing Chassis runtime. There is no explicit process-time heap allocation in Chassis's translation code.

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

Clack owns the CLAP create/init/start/stop/destroy scaffolding around these implementations. Chassis currently uses Clack's default no-op start/stop because the core has not established a distinct start/stop semantic requirement.

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
2. compare the locked 0.1.1 registry sources/checksums with this audit;
3. compile the `chassis-clap-conformance` rlib/cdylib;
4. package a real `.clap` artifact for the current platform;
5. run `clap-validator` plus lifecycle/buffer stress;
6. smoke-test in at least one real CLAP host;
7. decide the general port/channel borrowing model before adding arbitrary multibus support;
8. add render-mode semantics and f64 only when their mappings are explicit.
