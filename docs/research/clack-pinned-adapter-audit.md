# Pinned Clack Adapter Audit

Status: current dependency/trust-boundary audit for the native CLAP adapter; not a DAW production-qualification record.

## Selected source

Chassis currently pins Clack at:

```text
repository: https://github.com/prokopyl/clack
revision:   c5975f9f89f0953b00768680357985d46178078a
```

The pinned source includes plugin/host main-thread reentrancy and adjacent safety/lifetime work. Its relevant crates use Edition 2024, declare Rust 1.85 MSRV, and are `MIT OR Apache-2.0`.

Release metadata refreshed 2026-10-11: registry `clack-plugin` 0.2.0 was published 2026-09-12, and GitHub `v0.2` now resolves to `27ca28391b57a61bad4bb5099288a7bf9247350d`. The old assertion that this tag identifies Chassis's pin is not current. See [dependency policy](../dependencies.md) for primary release metadata. Review the source delta and rerun adapter conformance before changing source/revision. Downgrading to the affected registry 0.1.1 line remains unacceptable.

The workspace manifest uses a full revision rather than a branch, and `deny.toml` allowlists only the Clack repository as the current git-source exception.

## Why Chassis does not use published 0.1.1

Clack issue #89, "UB when hosts call CLAP API re-entrantly," documented that the plugin-side API incorrectly assumed CLAP callbacks could not be re-entered. Affected handlers received exclusive `&mut self`; synchronous host re-entry could therefore create overlapping mutable references and undefined behavior. Bitwig and `clap-wrapper` were cited as real-world reentrant callers.

PR #90 / commit `e8c956ae5c2ebabfee4abf28200fc66b484c47b3` corrected the plugin-side model by changing affected handler access from exclusive `&mut` to shared `&self` and requiring explicit interior mutability where mutation is genuinely needed. The `v0.2` release also includes the corresponding host-side re-entrancy work.

This matters directly to Chassis because:

- real hosts can synchronously re-enter CLAP callbacks;
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
- the development 0.2.0 API/version changes.

Inclusion by revision is not proof by itself. Chassis tests the adapter paths it relies on and still treats a Clack revision/release change as an audit + conformance event.

## Thread and re-entrancy model fit

The pinned Clack API models:

- audio processor as `Send`, processed exclusively through `&mut`;
- main-thread state as neither `Send` nor `Sync`, with reentrant callbacks exposed through shared `&self`;
- shared state as `Send + Sync`.

This maps to Chassis as follows:

- `chassis_runtime::runtime::Processor` is not globally `Send`, because same-thread embedded runtimes remain valid;
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

Chassis still relies on Clack for the raw C ABI and pointer/alias validation. `chassis-runtime` remains `#![forbid(unsafe_code)]`; current adapter production code adds no Chassis-owned unsafe block.

## Current qualification boundary

The configured Linux/macOS/Windows workflows package both CLAP fixtures and run pinned `clap-validator` 0.4.1 plus bounded fuzz. The [validation guide](../design/validation.md) owns exact revision-scoped results; configured gates are not proof that an arbitrary later revision passed.

The REAPER baseline (2026-09-06) records macOS-arm64 coverage of scan/instantiate, deterministic automation render, state round-trip, active save, and PDC alignment; see `docs/design/validation.md` for the recorded matrix.

A manual adapter-overhead benchmark harness exists for representative local hardware; CI may compile it, but hosted-runner timing is not promoted as performance evidence.

## Refresh triggers

Re-audit when Clack changes revision/source form or a new extension path relies on previously unreviewed lifetime/unsafe behavior. Registry 0.2.0 is now a candidate, not an already-audited replacement.

Keep Rust, portable-host, validator, and fuzz gates green for the revision under review. Refresh the dated real-host/performance evidence when making corresponding current claims; neither the old REAPER run nor its benchmark automatically qualifies later changes. Additional real-host reentrancy evidence remains useful when available. Headless f64 semantics and host wire-precision preference are separate questions.
