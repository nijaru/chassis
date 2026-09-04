# Pinned Clack Adapter Audit

Status: current dependency/trust-boundary audit for the native CLAP adapter; not a DAW production-qualification record.

## Selected source

Chassis currently pins Clack at:

```text
repository: https://github.com/prokopyl/clack
revision:   c5975f9f89f0953b00768680357985d46178078a
```

The revision is also the exact commit referenced by Clack's `v0.2` GitHub prerelease tag. That release was published on 2026-08-05 and explicitly centers plugin/host main-thread re-entrancy support plus the adjacent safety/lifetime changes. The relevant Clack crates use Edition 2024, declare Rust 1.85 MSRV, and are `MIT OR Apache-2.0`.

As of 2026-09-04, crates.io/docs.rs still expose 0.1.1 as the latest published registry release for the Clack crates. Chassis therefore keeps the exact git `rev`; using the tag instead would not improve reproducibility, and downgrading to registry 0.1.1 would reintroduce the known re-entrancy defect.

The workspace manifest uses a full revision rather than a branch, and `deny.toml` allowlists only the Clack repository as the current git-source exception.

## Why Chassis does not use published 0.1.1

Clack issue #89, "UB when hosts call CLAP API re-entrantly," documented that the plugin-side API incorrectly assumed CLAP callbacks could not be re-entered. Affected handlers received exclusive `&mut self`; synchronous host re-entry could therefore create overlapping mutable references and undefined behavior. Bitwig and `clap-wrapper` were cited as real-world reentrant callers.

PR #90 / commit `e8c956ae5c2ebabfee4abf28200fc66b484c47b3` corrected the plugin-side model by changing affected handler access from exclusive `&mut` to shared `&self` and requiring explicit interior mutability where mutation is genuinely needed. The `v0.2` release also includes the corresponding host-side re-entrancy work.

This matters directly to Chassis because:

- Bitwig remains an intended CLAP qualification host;
- `clap-wrapper` remains the preferred first VST3/AU projection path;
- Chassis state load can synchronously call host parameter-rescan hooks;
- Chassis activation can synchronously call the host latency-change hook;
- a known aliasing/UB defect in that trust boundary is unacceptable for the framework's safety criteria.

The git dependency is therefore a safety/reproducibility exception, not a request to track Clack development head.

## Additional safety work included by the pin

The selected revision includes:

- plugin-side main-thread re-entrancy support;
- host-side main-thread re-entrancy support;
- cookie setter/SysEx APIs marked unsafe where required;
- GUI/window-handle lifetime tightening;
- the development version bump represented by the `v0.2` prerelease tag.

Inclusion by revision is not proof by itself. Chassis tests the adapter paths it relies on and still treats a Clack revision/release change as an audit + conformance event.

## Thread and re-entrancy model fit

The pinned Clack API models:

- audio processor as `Send`, processed exclusively through `&mut`;
- main-thread state as neither `Send` nor `Sync`, with reentrant callbacks exposed through shared `&self`;
- shared state as `Send + Sync`.

This maps to Chassis as follows:

- `chassis-core::Processor` is not globally `Send`, because same-thread embedded runtimes remain valid;
- `chassis-clap` requires concrete processors to be `Send`, because CLAP may transfer the processor between host audio threads;
- processing remains exclusive and does not require `Sync` DSP state;
- `ChassisMainThread<C>` uses shared references for CLAP extension callbacks; mutable cross-domain projections use atomics/interior synchronization rather than exclusive main-thread borrows;
- state load and activation may call host extensions synchronously, so those code paths must remain safe if the host immediately re-enters plugin extension discovery.

The in-process host suite now exercises plugin -> host -> plugin re-entry through parameter-rescan and latency-change callbacks in addition to the pinned dependency's own re-entrancy tests.

## Buffer boundary

The pinned Clack safe audio API distinguishes:

- `ChannelPair::InputOnly`;
- `ChannelPair::OutputOnly`;
- `ChannelPair::InputOutput` for disjoint input/output;
- `ChannelPair::InPlace` for exact aliasing represented by one mutable slice.

Chassis preserves those safe relationships through setup-built mapped process slots and a lazy generic `ProcessBufferSource`. It does not construct a callback-owned channel `Vec` or erase lifetimes with Chassis-owned unsafe code.

Current in-process tests cover mapped main + sidechain topology, f32/f64 paths, minimum/maximum frame bounds, configured maximum parameter-event load, and allocation/deallocation across the complete adapter callback path.

## Current semantic mapping

Current native CLAP projection is:

```text
CLAP factory/init              -> construct/validate Chassis component + projections
CLAP activate                  -> InstanceRuntime activation + activation-scoped latency
CLAP start/process/stop        -> Clack lifecycle + Chassis process callback
CLAP reset                     -> InstanceRuntime::reset
CLAP deactivate                -> InstanceRuntime::deactivate
CLAP audio ports               -> setup-built stable mapped ports/process slots
CLAP process f32/f64           -> safe ChannelPair views -> ProcessBufferSource -> InstanceRuntime
CLAP params                    -> typed metadata + bounded value-event normalization/publication
CLAP state                     -> bounded CHSS save/load + transactional parameter publication
CLAP render                    -> ProcessMode::Realtime / ProcessMode::Offline
CLAP latency                   -> activation snapshot + host change notification
CLAP transport                 -> bounded borrowed transport snapshot
```

The deterministic conformance export has audible sample-accurate `trim` automation, typed float/choice state, f32/f64 processing, and zero declared latency. A separate in-process delayed probe proves nonzero reported latency matches actual delayed audio sample-for-sample.

Note/MIDI/event ports, GUI/editor APIs, and broad application/host infrastructure remain intentionally deferred until real clients require them.

## Failure containment and realtime evidence

The adapter rejects or contains invalid inputs before product DSP for the validated boundaries currently represented by core/Clack, including invalid activation bounds, unsupported audio mappings/sample representations, malformed callback dimensions, and invalid/beyond-budget parameter event streams.

Current mechanical evidence includes:

- failed product activation leaves the CLAP instance reusable;
- create/init/destroy without activation is repeatable;
- reactivation recomputes activation-scoped latency and can change sample-rate/block bounds;
- processor ownership can transfer to another audio thread without concurrent mutation;
- current main + sidechain f32/f64 callback paths allocate/deallocate zero times at the configured event bound in the test harness;
- frame-bound extremes are accepted and over-bound process blocks rejected;
- active state save linearizes to one completed scalar generation;
- nonzero latency metadata matches an actual delayed impulse;
- state-rescan and latency host callbacks tolerate immediate plugin extension re-entry.

Chassis still relies on Clack for the raw C ABI and pointer/alias validation. `chassis-core` remains `#![forbid(unsafe_code)]`; current adapter production code adds no Chassis-owned unsafe block.

## Current qualification boundary

Current Linux CI packages the conformance `.clap` and runs the pinned `clap-validator` 0.4.1 source plus bounded two-worker fuzz. The current result is 35 passed, 0 failed, 9 intentional skips, with the bounded fuzz run clean.

The most recent REAPER evidence is historical macOS-arm64 coverage from before the latest automation/latency work. It must not be described as current-head DAW qualification.

A manual adapter-overhead benchmark harness exists for representative local hardware; CI may compile it, but hosted-runner timing is not promoted as performance evidence.

## Remaining promotion gates

Before describing Chassis CLAP as production-qualified:

1. keep Rust fmt/test/Clippy/Loom and current CLAP validator/fuzz gates green;
2. retain portable macOS/Windows compilation/test coverage alongside Linux;
3. run the adapter-overhead benchmark on stable representative hardware and record CPU/OS/toolchain/workload;
4. requalify current head in real DAWs: scan/instantiate, save/reopen, deterministic automation render, active save, and PDC alignment;
5. exercise a host-selected native f64 path when an available DAW can select it;
6. add Bitwig or equivalent real-world re-entrancy coverage when available;
7. qualify the first real Chassis product before freezing higher-level authoring conveniences;
8. re-audit and requalify if Clack changes revision/source form;
9. migrate to crates.io only when a suitable safety-fixed registry release exists and passes the same evidence gates.
