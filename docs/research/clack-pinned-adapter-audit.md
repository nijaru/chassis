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

The workspace manifest uses a full revision rather than a branch, and `deny.toml` allowlists only the Clack repository as the current git-source exception. A workspace Cargo patch selects `vendor/clack/plugin` for `clack-plugin`; common, host, and extensions retain the exact upstream pin. The vendored crate is excluded from Chassis workspace membership, not from the resolved dependency graph.

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

## Buffer boundary — reviewed 2026-10-13

Pinned Clack's `plugin/src/process/audio/pair.rs` constructs shared/mutable sample references before comparing their pointers. Exact-in-place behavior failed a default Miri probe while Tree Borrows passed; this is model-dependent evidence, not a demonstrated native crash. That API also does not establish whole callback partial/cross-channel/cross-port alias legality. The earlier attribution of complete pointer/alias validation to Clack was too strong.

Chassis now bypasses that sample-pair constructor. Clack's `Audio::raw_buffers()` supplies descriptor borrows; `chassis-clap/src/raw_audio.rs` reads pointer tables without constructing table/sample slices, validates complete mapped aliases, then constructs:


- `ChannelPair::InputOnly`;
- `ChannelPair::OutputOnly`;
- `ChannelPair::InputOutput` for disjoint input/output;
- `ChannelPair::InPlace` for exact aliasing represented by one mutable slice.

The adapter permits shared inputs and exact declared reciprocal in-place pairs; all other overlap involving writable samples is rejected before sample references. Chassis-owned unsafe code is isolated in `raw_audio`, with activation-owned scratch, O(C) storage, O(C²) alias validation, and lazy views borrowing the live source. Its scratch `Send` transfers no sample ownership. `chassis-runtime` still forbids unsafe code.

The host must supply valid live descriptor arrays and readable pointer tables, correctly initialized sample allocations of sufficient extent, metadata separate from samples, and exclusive writable access. Null/alignment/address/alias checks cannot prove pointer provenance, allocation length, initialization, or concurrency. Miri and native tests support the modeled paths, not arbitrary invalid C pointers.

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
CLAP process f32/f64           -> raw alias qualification -> safe views -> InstanceRuntime
CLAP params                    -> typed metadata + bounded mixed base/modulation controls
CLAP state                     -> bounded CHSS save/load + coherent parameter/custom publication
CLAP render                    -> ProcessMode::Realtime / ProcessMode::Offline
CLAP latency                   -> activation snapshot + host change notification
CLAP transport                 -> bounded borrowed transport snapshot
```

The deterministic conformance export has audible sample-accurate `trim` automation, typed float/choice state, f32/f64 processing, and zero declared latency. A separate in-process delayed probe proves nonzero reported latency matches actual delayed audio sample-for-sample.

Note/MIDI/event ports, GUI/editor APIs, and broad application/host infrastructure remain intentionally deferred until real clients require them.

## Global modulation boundary — reviewed 2026-10-24

The existing pin's `common/src/events/event_types/param_value.rs` supplies `ParamModEvent`, its repr-C wrapper, and the existing unsafe `Event` implementation. `UnknownEvent::as_event_for_space` in `common/src/events.rs` checks space, type, and exact struct size before borrowing a typed event. Chassis recognizes value/modulation headers explicitly, rejecting wrong known sizes rather than treating a failed typed borrow as an unknown event. Host event pointers still require valid aligned storage covering the declared extent; these checks cannot establish provenance or host concurrency.

`common/src/events/pckn.rs::Match::from_raw` treats every negative integer as wildcard. The adapter instead checks each raw note/port/channel/key field equals exactly `-1`; `-2` must not become a global update. Correctly sized unknown parameter IDs are ignored before targeting/value validation. Known unsupported targeting or invalid values reject process before DSP.

`extensions/src/params/plugin.rs::flush` borrows the input list and dispatches to the active audio processor or inactive main-thread handler. Because it returns void, Chassis validates the bounded immutable recognized batch before applying any changes; malformed batches are ignored without logging. Timing is lost in flush, but list order is preserved. No new Chassis unsafe production path or Clack source delta is needed.

The reviewed [CLAP event contract](https://github.com/free-audio/clap/blob/a47f6badb49d948fd009998f28309cdab78979c9/include/clap/events.h#L100-L109) defines modulation as a held amount set and effective base-plus-amount, not incremental accumulation. [Flush/threading](https://github.com/free-audio/clap/blob/a47f6badb49d948fd009998f28309cdab78979c9/include/clap/ext/params.h#L295-L306) serializes process and flush; activation is outside processing. Independent fixed-size scalar atomics therefore retain external amounts across adapter runtime lifetimes without coupling them to base/custom state publication. Relaxed operations carry no associated payload; save/load never access these scalars. Accepted modulation survives rejected base-state completion.

[Reset](https://github.com/free-audio/clap/blob/a47f6badb49d948fd009998f28309cdab78979c9/include/clap/plugin.h#L84-L90), activation, and state contracts do not specify a modulation-clear/resend protocol. Chassis preserves held input across these boundaries explicitly. This is not a universal state-load rule: [NIH-plug](https://github.com/robbert-vdh/nih-plug/blob/de421011f41a6d10fc8c7a6084e4f4dee0143683/src/wrapper/state.rs#L174-L238) preserves modulation while restoring base state, whereas [CLAP example parameter deserialization](https://github.com/free-audio/clap-plugins/blob/9fdaf3d667c25bb49ff7a04cd877491c48b56524/plugins/parameters.hh#L46-L60) invokes a reset that clears it. A custom-resource mismatch freezes consumed base and modulation before synchronization; desired amounts remain available for reactivation without retargeting old resources.

## Parameter text boundary — reviewed 2026-10-24

At the same pin, `extensions/src/params/plugin.rs::ParamDisplayWriter` exposes remaining capacity excluding the terminator, but `write_str` copies only the available byte prefix and returns success. This can truncate within a UTF-8 code point; embedded NULs are not rejected.

The adapter's `parameters/control.rs::HostTextWriter` rejects either condition before forwarding a chunk. The core semantic formatter tracks writer failures even if author policy ignores `fmt::Error`, so the callback cannot report a successful truncated conversion. Actual-entry tests cover multibyte boundary cuts, NULs, and ignored writer errors. No Clack source patch is needed for this control-thread wrapper.

Parameter text delegates to descriptor policy without trimming custom syntax. Finite choice admission validates actual policy text against stable identity rather than banning raw labels independently. CLAP's stepped plain conversion still truncates before presentation; normalized framework controls do not redefine host plain automation. [Parameters/state](../design/parameters-state.md#control-mapping-and-presentation) owns these semantics.

## Failure containment and realtime evidence

The adapter rejects or contains invalid inputs before product DSP for the validated boundaries currently represented by core/Clack, including invalid activation bounds, unsupported audio mappings/sample representations, malformed callback dimensions, and invalid/beyond-budget parameter event streams.

Current mechanical evidence includes:

- failed product activation leaves the CLAP instance reusable;
- create/init/destroy without activation is repeatable;
- reactivation recomputes activation-scoped latency and can change sample-rate/block bounds;
- processor ownership can transfer to another audio thread without concurrent mutation;
- current main + sidechain f32/f64 callback paths allocate/deallocate zero times at the configured event bound in the test harness;
- frame-bound extremes are accepted and over-bound process blocks rejected;
- active state save linearizes to one completed parameter/custom generation;
- nonzero latency metadata matches an actual delayed impulse;
- state-rescan and latency host callbacks tolerate immediate plugin extension re-entry.

Clack remains the ABI/lifecycle/panic-containment owner, not the sample alias-validation owner. The reviewed Chassis adapter supplies that sample boundary.

### Realtime process rejection — local source delta reviewed 2026-10-23

Upstream `plugin/src/plugin/logging.rs` formats/allocates for host logging or writes stderr on error. The safe `ProcessStatus` API has no nonreporting error variant. Comparing the audited pin with release commit `27ca28391b57a61bad4bb5099288a7bf9247350d` confirmed that plugin process/error handling is unchanged; upgrading alone cannot close this gap. The release's other changes (event gesture decoding, descriptor/features, GUI/extension wrappers, and host wrapper bounds) are not adopted by this fix.

Only `clack-plugin` is vendored from `c5975f9`, with its original source, tests, READMEs, and MIT/Apache license texts. Its manifest replaces upstream workspace/path inheritance with explicit same-pin dependencies and preserves upstream lints. Examples are not copied, and three upstream trailing-whitespace lines are normalized. The only behavioral Rust source changes are:

- `extensions/wrapper.rs`: a crate-private `handle_process` uses the existing pointer/lifecycle lookup and panic containment, returning `None` without calling `plugin_log`;
- `plugin/instance.rs`: only the process ABI callback uses that helper. Its existing `None -> CLAP_PROCESS_ERROR` mapping is unchanged.

All other callbacks retain normal diagnostics and the existing lifecycle/reentrancy model. Cargo resolves one plugin crate shared by Chassis, extensions, and the test host. The patch introduces no new ABI callback, reference construction, dependency version, or public API.

Chassis deterministic process errors carry static messages, so dropping them does not allocate/deallocate. Boxed/custom errors and arbitrary DSP panics are not covered: catch-unwind remains, but panic hooks, payloads, and arbitrary destructors need not be realtime-safe. Process failure terminates the run; this patch promises no post-panic reuse.

The actual-entry partial-alias rejection regression failed before this patch with one allocation. It now measures acceptance and rejection with zero allocations/deallocations and no host-log calls. No-host-log rejection was also measured locally; the source path cannot fall back to stderr. Packaged validator/fuzz and lifecycle/panic checks are recorded in [validation](../design/validation.md).

Keep this delta narrow. Remove the vendored override only when a reviewed upstream replacement preserves nonlogging process rejection and passes adapter qualification.

## Current qualification boundary

The configured Linux/macOS/Windows workflows package both CLAP fixtures and run pinned `clap-validator` 0.4.1 plus bounded fuzz. The [validation guide](../design/validation.md) owns exact revision-scoped results; configured gates are not proof that an arbitrary later revision passed.

The REAPER baseline (2026-09-06) records macOS-arm64 coverage of scan/instantiate, deterministic automation render, state round-trip, active save, and PDC alignment; see `docs/design/validation.md` for the recorded matrix.

A manual adapter-overhead benchmark harness exists for representative local hardware; CI may compile it, but hosted-runner timing is not promoted as performance evidence.

## Refresh triggers

Re-audit when Clack changes revision/source form, the vendored patch changes, or a new extension path relies on previously unreviewed lifetime/unsafe behavior. Registry 0.2.0 is not a drop-in replacement for the local process-error fix.

Keep Rust, portable-host, validator, and fuzz gates green for the revision under review. Refresh the dated real-host/performance evidence when making corresponding current claims; neither the old REAPER run nor its benchmark automatically qualifies later changes. Additional real-host reentrancy evidence remains useful when available. Headless f64 semantics and host wire-precision preference are separate questions.
