# Validation and Conformance Strategy

## Goal

Framework correctness must be observable and reproducible. Keep separate evidence for:

- Rust/type correctness;
- format/API conformance;
- realtime allocation/work guarantees;
- lifecycle/ownership correctness;
- state/identity compatibility;
- cross-format semantic parity;
- performance;
- production host qualification.

A stronger claim requires matching evidence. One build, validator pass, or successful DAW load does not imply the others.

## Automated headless gates

### Rust CI

Linux:

- `cargo fmt --all -- --check`;
- `cargo test --workspace --locked`;
- `cargo clippy --workspace --all-features --all-targets --locked -- -D warnings`;
- Loom publication model;
- `cargo deny check`;
- `cargo machete`.

macOS and Windows:

- `cargo test --workspace --locked`;
- `cargo check --workspace --all-features --all-targets --locked`.

Dependency-policy tooling is pinned in CI to `cargo-deny` 0.20.2 and `cargo-machete` 0.9.2.

### CLAP Conformance

On Linux, macOS, and Windows:

1. build the release conformance, delayed-probe, note-probe, and MIDI-probe artifacts;
2. package the platform-native `.clap` forms;
3. build `clap-validator` from pinned revision `b2f1d9b79b1d264a5747f46707d72b1aa40a02ef`;
4. run the validator on all four artifacts;
5. run a bounded five-second two-worker fuzz pass on each artifact.

The recorded validator baseline is **35 passed, 0 failed, 9 intentional skips** for the conformance artifact. The matrix is configured in CI; successful revision-scoped runs and local checks below establish actual coverage. The exported delayed probe (`examples/clap-delayed-probe`) validates at **21 passed, 0 failed, 23 intentional skips** — the skips are parameter/state cases the no-parameter latency probe does not declare — and passes the same bounded fuzz after flushing subnormal delay-line output to zero (initially caught by the fuzzer as subnormal pass-through).

The native-note instrument (`examples/clap-note-probe`) validates at **37 passed, 0 failed, 7 skipped** and passes the same bounded fuzz. Packaging is shared through [the native packaging script](../../.github/scripts/package-clap.sh); the isolated validator includes the reviewed corrections described below.

The zero-audio MIDI processor (`examples/clap-midi-probe`) validates at **34 passed, 0 failed, 10 skipped**. Its actual-entry tests supply SysEx and MIDI 2/UMP coverage absent from the validator's generated event set.

These gates do not establish production DAW behavior or representative performance.

Hosted workflows and local results are separate evidence. Before publication, hosted jobs were rejected and local checks supplied the interim record. Consult the actual workflow run for the revision under review before claiming Linux/macOS/Windows coverage. The dated validator, host, and benchmark results below are baselines, not automatic qualification of later commits.

### Compact timed normalized trajectories — 2026-11-12

At code revision `ddea6d96de1cff202a4c55f129d5a4ba17ae9bd9` on macOS 26.7.2 arm64:

- 311 workspace tests, including doctests, passed on stable Rust 1.99.1 and CI-baseline Rust 1.98.0, with no failures or ignored tests. Formatting and strict all-feature/all-target Clippy passed on both compilers; warning-denied rustdoc, first-party local Markdown paths and diff checks passed. Local `cargo-deny` 0.19.0 passed with unused-license-allowance warnings; `cargo-machete` 0.9.1 found no unused dependencies. Package versions, dependencies, audited Clack source, validator pin and CHSS bytes are unchanged.
- Production `NormalizedSet` / `NormalizedLinear` points share the existing bounded ordered stream with plain controls and modulation. Borrowed float cursors interpolate coordinates before forward mapping, then compose plain modulation/clamping. Canonical provenance or an explicit normalized set supplies the anchor; plain points invalidate it even at equal values. No inverse supplies source history. Endpoint preflight stages fixed-width completion pairs before DSP; successful tail completion commits them without an author mapper call. Normalized admission is structurally `O(P + E)` with setup-allocated `O(P)` scratch; plain-only batches avoid a scratch sweep. This is a work/storage bound, not a timing result.
- Ten new cases exercise public-runtime audio across nonlinear and flat mappings, mixed domains, same-offset incoming ramps/overwrites, identical-load reseeding, admission-versus-completion ownership, cursor cloning/query errors, plain-only cursor rejection, multi-parameter/discrete completion, and clipping/partition invariance. Invalid overwritten endpoints reject before DSP; valid endpoints with NaN/out-of-domain interiors fail when queried and retain the prior canonical pair. Product errors and post-DSP tail panic publish neither field; damaged processors are abandoned. Frozen-resource DSP keeps its original pair while desired-source events are checked but withheld. A completion-probe mapper explicitly fails if invoked after DSP.
- The nine prior source-contract cases now compare independent source mathematics and bounded dense expansion with compact **production** mapped cursors. The dense reference remains explicitly bounded, not the default normalized path. A further 1,000 compact timed callbacks run alongside the existing 1,000 normalized scalar callbacks, including an exact eight-event batch mixing normalized points, plain overrides and all four typed value families. The serial TLS probe records **zero allocations/deallocations**, including rejected mapped endpoint admission; entering the non-realtime inverse would fail. Cursor cloning borrows metadata and never clones/releases an owning mapper.
- Strict-provenance, default-Stacked-Borrows Miri `0.1.0 (9c5648833e 2026-11-10)` passed all 25 functional cases in the allocation-test binary: ten new timed cases, six scalar cases and nine source-reference cases. The long parent allocation test ran natively, not under Miri. Independent design and code review found no material unresolved issue. Neither admission probes, tests nor reviews certify arbitrary author mappers' realtime bounds or whole-domain validity; consumed outputs remain checked.

Hosted [Rust CI](https://github.com/nijaru/chassis/actions/runs/43245608410) and [CLAP Conformance](https://github.com/nijaru/chassis/actions/runs/43245609098) succeeded at this exact source revision. All seven Rust jobs and all three CLAP jobs were inspected, including build, native packaging, pinned-validator, validation and bounded fuzz steps. Every platform rebuilt and validated all four exports and separately fuzzed each for five seconds with two workers. All twelve validator runs reported zero failures and zero warnings; Windows retained its additional platform-inapplicable `scan-rtld-now` skip. A warning-free shared-runner result does not establish representative denormal cost or close the existing [floating-point environment qualification](runtime-services.md#denormals--floating-point-environment). These checks qualify the existing CLAP deployment, not a normalized-source adapter.

**M2 remains open.** Timed normalized handling and explicit reseeding are implemented; typed control origins, gestures, observation and echo-safe publication are next in [the execution plan](../next-runtime-slice.md#2-finish-portable-control-and-processing-semantics). No new timing or production-DAW evidence was collected; historical performance does not qualify these changed paths. No normalized-source format adapter is claimed. This record qualifies the source revision above, not its later evidence-only documentation commit.

### Prepared float mapping and normalized scalar provenance — 2026-11-12

At code revision `5f8febdc89e1ce72d96715e8cec53e1931ebaafe` on macOS 26.7.2 arm64:

- 301 workspace tests, including doctests, passed on stable Rust 1.99.1 and CI-baseline Rust 1.98.0, with no failures or ignored tests. Formatting and strict all-feature/all-target Clippy passed on both compilers; warning-denied rustdoc, first-party local Markdown paths and diff checks passed. Local `cargo-deny` 0.19.0 passed with unused-license-allowance warnings; `cargo-machete` 0.9.1 found no unused dependencies. Package versions, dependencies, audited Clack source, validator pin and CHSS bytes are unchanged.
- `FloatMapping` now owns both float-normalization directions, separately from non-realtime text policy. All normalization-policy callers were migrated without shims, including the conformance product's exact square/square-root taper. The robust default linear and built-in discrete arithmetic moved without changing its formulas; existing extreme/subnormal/constant-domain and full-width integer rounding tests remain passing. Forward realtime bounds and whole-domain validity are author obligations, not properties certified by admission probes. Deliberately invalid interior outputs demonstrate per-use rejection without canonical mutation.
- Six new public-runtime cases protect exact normalized scalar provenance, non-injective mappings, representative presentation without cache filling, every plain mutation route including equal-value edits, failed input/output admission, identical and invalid complete-state loads, frozen-resource projection, reset/reactivation, mapper configuration drift and successful versus failed endpoint completion. Desired edits and repeated loads never retarget the first frozen base/coordinate pair. Process error and post-DSP tail panic retain the prior pair; damaged DSP is abandoned, not resumed.
- Another 1,000 native callbacks include dense normalized edits through custom plateau and default linear mappings, actual public-runtime DSP, and repeated invalid coordinate/index/type/mapped-output rejection. The serial TLS probe records **zero allocations/deallocations** throughout the tracked work. The non-realtime inverse would fail if entered on that thread. No owning mapper clone/drop occurs in dense mutation; metadata remains retained by the instance schema/store.
- Strict-provenance, default-Stacked-Borrows Miri `0.1.0 (9c5648833e 2026-11-10)` passed all fifteen functional cases in the allocation-test binary: six new scalar cases and nine migrated source-contract cases. The long parent allocation qualification ran natively, not under Miri. Independent code review found no material scalar ownership or lifecycle issue; neither review nor tests certify arbitrary author mappings.

Hosted [Rust CI](https://github.com/nijaru/chassis/actions/runs/43237807336) and [CLAP Conformance](https://github.com/nijaru/chassis/actions/runs/43237806753) succeeded at this exact source revision. All seven Rust jobs and all three CLAP jobs were inspected, including successful build, native packaging, pinned-validator, validation and bounded fuzz steps. Each platform rebuilt and validated all four exports and separately fuzzed each for five seconds with two workers. All validator runs had zero failures; macOS and Windows had zero warnings. Linux retained denormal-input warnings in conformance and delayed processing, measured at 5.57× and 6.48× normal-input time on shared runners. These observations do not establish a causal regression or representative cost; denormal performance remains unqualified under the existing [floating-point environment policy](runtime-services.md#denormals--floating-point-environment). Windows retained its additional platform-inapplicable `scan-rtld-now` skip. These format results do not qualify a normalized-source adapter.

**M2 remains open.** Normalized scalar admission immediately establishes canonical base and coordinate; it is not timed source completion. Timed normalized trajectories, explicit anchoring after unknown provenance, source ordering/budgets and completed-coordinate publication are next in [the execution plan](../next-runtime-slice.md#2-finish-portable-control-and-processing-semantics), followed by control origins/gestures. No new timing or production-DAW evidence was collected; historical performance does not qualify these changed paths. This record qualifies the source revision above, not its later evidence-only documentation commit.

### Source-domain trajectory reference proof — 2026-11-12

At code revision `0bebec50f1a406e005d53b4062fdc8a5a83f2d93` on macOS 26.7.2 arm64:

- 295 workspace tests, including doctests, passed on stable Rust 1.99.1 and CI-baseline Rust 1.98.0, with no failures or ignored tests. Formatting and strict all-feature/all-target Clippy passed on both compilers; warning-denied rustdoc, first-party local Markdown paths and diff checks passed. Local `cargo-deny` 0.19.0 passed with unused-license-allowance warnings; `cargo-machete` 0.9.1 found no unused dependencies. Production code, dependencies, audited source, package versions and pins are unchanged.
- The [private source-contract fixture](../../crates/chassis-runtime/tests/support/trajectory_portability.rs) independently implements the normalized queue mathematics cited in the [trajectory contract](parameters-state.md#trajectory-portability-before-stabilization), not the source SDK or ABI. An independent barycentric oracle and analytical goldens distinguish nonlinear mapping from previous-sample anchoring. Compact source-coordinate clipping and dense mapped-plain conversion through the actual public runtime agree across rising/falling curves, unequal block partitions, one-frame blocks, separated interior knots with slope reversal, adjacent-sample jumps and held tails. Independently computed interiors use absolute `1e-12` tolerance; exact binary goldens and retained coordinates use exact comparisons.
- Admission has separate bounds for two queues, eight total source points, sixteen frames and eight supplemental modulation inputs. Literal expansion also checks the actual activated output-event bound, including modulation. Tests cover exact fits, one-over rejection, reversed independent-queue order, malformed sources, runtime schema rejection and product failure after audio mutation. Failed consumption publishes neither source carry nor completed base values; already-written audio is not rolled back and damaged DSP is abandoned. Fresh-runtime state reopen retains mapped plain base, not the source coordinate or held modulation.
- Independent review found that supplemental plain events could override the mapped base while the fixture retained a different source coordinate. The regression failed before narrowing supplemental admission to modulation only. Arbitrary plain edits and state-load coordinate reseeding remain a production design gate, not a claimed fixture capability.
- A further 1,000 callbacks include source validation and literal conversion of two sixteen-frame queues, one plain modulation event and public-runtime DSP: exactly 33 events against the activated bound of 33. Source-limit and 34-event rejection are also measured. The serial TLS allocator probe records **zero callback allocations/deallocations**; presentation-policy hooks would fail if entered on that thread. This qualifies the bounded dense path, not a general prepared mapper or performance.
- Strict-provenance, default-Stacked-Borrows Miri `0.1.0 (9c5648833e 2026-11-10)` passed all nine new functional source-proof cases. The long parent heap-qualification test ran natively, not under Miri; neither result qualifies the whole workspace or a future source adapter.

Hosted [Rust CI](https://github.com/nijaru/chassis/actions/runs/43222256187) and [CLAP Conformance](https://github.com/nijaru/chassis/actions/runs/43222256153) succeeded at this exact source revision. All seven Rust jobs and all three CLAP jobs were inspected, including successful build, packaging, pinned-validator, validation and fuzz steps. Each platform rebuilt and validated the four unchanged exports and separately fuzzed each for five seconds with two workers. There were zero validator failures. Linux reported denormal-input warnings for conformance and delayed processing at 2.82× and 2.45× normal-input time; Windows reported one conformance warning at 2.38×. macOS reported no warnings. Windows retained the extra platform-inapplicable `scan-rtld-now` skip. These shared-VM observations leave denormal performance unqualified under the existing [floating-point environment policy](runtime-services.md#denormals--floating-point-environment); green jobs do not close that limitation. These format checks do not qualify a normalized-source adapter.

**M2 remains open.** This establishes the reference-backed direction, not production normalized automation, a general realtime mapping API, control gestures/origins or another plugin adapter. Prepared mapping admission and consumed-coordinate ownership across plain edits, state loads and resource generations are next in [the execution plan](../next-runtime-slice.md#2-finish-portable-control-and-processing-semantics). No new real-host or timing evidence was collected; the existing M1 performance limitations remain. This record qualifies the source revision above, not its later evidence-only documentation commit.

### Fixed-resource execution costs and M1 exit — 2026-11-12

At code revision `8c683fe87b06d0d6e19e0e69c06dbd07e502e1ca` on macOS 26.7.2 arm64:

- 286 workspace tests, including doctests, passed on stable Rust 1.99.1 and CI-baseline Rust 1.98.0, without failures or ignored tests. Formatting, strict all-feature/all-target Clippy on both compilers, warning-denied rustdoc and diff checks passed. Local `cargo-deny` 0.19.0 passed with unused-license-allowance warnings; `cargo-machete` 0.9.1 found no unused dependencies. This revision changes benchmark/test code only; production code, package versions and dependency pins are unchanged.
- The replacement benchmark consumes gain, wet/dry and linked-duck depth, each with base steps and global modulation, through bare ordinary slices, the public runtime and the actual CLAP process ABI. An independent per-frame rescan oracle checks exact native `f32`/`f64` audio, completed base values and event-free held modulation before timing. The full release matrix passed all 228 path/strategy/precision/workload preflights.
- A preflight regression reproduced the original six-event sparse stream becoming constant after steady-state carryover. The corrected twelve-event stream changes every effective parameter within repeated callbacks; both native regression tests and the benchmark enforce that invariant. The declared bound remains 64 combined base/modulation events.
- Strict-provenance, default-Stacked-Borrows Miri `0.1.0 (9c5648833 2026-11-10)` passed both new precision-specific workload tests, including tiny-block equal-offset events, exact in-place audio and the focused topology cases. They use owned raw CLAP pointer tables, not the pinned host helper's multiport builder. This extends focused boundary evidence, not whole-Clack qualification.
- Two full release timing runs established the workload-specific [cost evidence below](#realtime-allocation-and-performance). Shared frame-major control evaluation avoids the demonstrated repeated per-channel scans using existing APIs; no new helper, event index or scheduler was needed. The preceding lifecycle, state, allocation and capacity proofs remain applicable to unchanged production code.

Hosted [Rust CI](https://github.com/nijaru/chassis/actions/runs/43211194925) and [CLAP Conformance](https://github.com/nijaru/chassis/actions/runs/43211194947) succeeded at this exact revision. All seven Rust jobs and all three CLAP jobs were inspected, including build, packaging, pinned-validator, validation and fuzz steps. Every platform validated and separately fuzzed all four exports for five seconds with two workers. Validator results below are **passed / warnings / skipped**, with **zero failures** throughout:

| Runner | Conformance | Delayed | Notes | MIDI |
| --- | --- | --- | --- | --- |
| macOS | 35 / 0 / 9 | 21 / 0 / 23 | 37 / 0 / 7 | 34 / 0 / 10 |
| Linux | 34 / 1 / 9 | 20 / 1 / 23 | 37 / 0 / 7 | 34 / 0 / 10 |
| Windows | 34 / 0 / 10 | 20 / 0 / 24 | 36 / 0 / 8 | 33 / 0 / 11 |

Linux's warnings report denormal-input processing taking 4.22× and 4.47× the normal-input time in the conformance and delayed fixtures. These shared-VM timing observations do not establish their cause or representative cost; denormal performance is not qualified by a green job. The existing [floating-point environment policy](runtime-services.md#denormals--floating-point-environment) remains unchanged: no automatic process guard is supplied. Recheck this limitation during cross-platform M4 cost qualification before making broader performance claims. Windows additionally skips the platform-inapplicable `scan-rtld-now` test.

**M1's documented fixed-resource subset is usable at this revision**, within the [boundary below](#fixed-resource-subset-boundary). State/control recall, supported lifecycle/failure and capacity behavior, public authoring/composition, and useful-DSP costs have matching evidence. This is not core stability, an API/CHSS compatibility promise, or a release. M2's portable trajectories and headless control origins/gestures are next; compatible resource adoption remains M3, and current real-host, integrated performance and compatibility qualification remain M4. No production DAW scenario or individual callback-tail measurement was repeated. This record qualifies the source revision above, not its later documentation commit.

### Fixed-resource public proof and state-stream ownership — 2026-11-12

At code revision `ac00a47ece851ec4b73fc3e63519d670d0c5b01d` on macOS 26.7.2 arm64, including the fixed-resource proof introduced by `3624d8a311df847a90703aca9e8b108364cb3eeb`:

- 284 workspace tests, including doctests, passed on stable Rust 1.99.1 and CI-baseline Rust 1.98.0, with no failures or ignored tests. Workspace and changed vendor-source formatting, strict all-feature/all-target Clippy on both compilers, warning-denied rustdoc, release workspace build, first-party local Markdown paths and diff checks passed. Local `cargo-deny` 0.19.0 passed with unused-license-allowance warnings; `cargo-machete` 0.9.1 found no unused dependencies. Package versions, dependency versions, the Clack pin, and the validator pin and its three patches are unchanged; the reviewed local Clack stream correction is new.
- Actual-entry activation regressions inject factory, latency-query and tail-query panics. Each preserves byte-identical desired state and held modulation, publishes no active metadata, destroys exactly the processors constructed, and permits same-instance retry and real processing. Core DSP/tail panic regressions preserve completed base/modulation/tail endpoints while retaining already-emitted output; damaged DSP is abandoned, not resumed.
- A public-runtime workload performs linked stereo processing with a real stereo sidechain and two timed typed controls, then composes two independent instances through caller-owned buffers. All 1,000 chained blocks (2,000 callbacks) match independently derived intermediate/final audio with zero measured callback allocations/deallocations and unchanged input. This is separate-buffer `f64` embedding evidence, not a graph, a new CLAP export or timing evidence.
- Deterministic telemetry checks reject busy/exhausted publication without writes, bound reads of a deliberately stalled writer, and preserve the last accepted display snapshot until a coherent new generation succeeds. Failed scratch contents are not accepted as display state.
- Final strict-provenance, default-Stacked-Borrows Miri exposed a real unique-borrow violation in pinned Clack state save: the ABI loans a const descriptor, but the wrapper cast it to `&mut`. Scoped locally owned input/output forwarders now retain the original descriptor/context and return values; mutable raw-field access requires unsafe code. Both state wrappers and the adjacent, unregistered state-context callers are migrated. The [audit](../research/clack-pinned-adapter-audit.md#state-stream-ownership--reviewed-2026-11-12) owns the exact source delta and separate compile-only state-context check.
- Miri `0.1.0 (9c5648833 2026-11-10)` passed five focused tests: three read-only stream-descriptor regressions and actual-entry activation/save and migration/load tests. These cover partial transfers, EOF/errors, absent callbacks, safe wrapper replacement and working original callbacks afterward. Separately, four negative stream API doctests reject lifetime escape and safe raw-field mutation, and the corrected extension-authoring example compiles, on both checked compilers. These vendor doctests are not included in the 284 workspace count; neither these runs nor the previous Miri baselines qualify all Clack or the whole workspace.
- All four rebuilt packaged artifacts passed the unchanged corrected pinned validator and separate five-second, two-worker fuzz runs: conformance **35 passed / 0 failed / 9 skipped**, delayed probe **21 / 0 / 23**, note probe **37 / 0 / 7**, MIDI probe **34 / 0 / 10**. A fresh installer build passed all three validator binary regressions.

Hosted [Rust CI](https://github.com/nijaru/chassis/actions/runs/43192625279) and [CLAP Conformance](https://github.com/nijaru/chassis/actions/runs/43192624334) succeeded at this exact code revision. All seven Rust jobs and all three Linux/macOS/Windows CLAP jobs were checked individually, including their build, packaging, pinned-validator, validation and fuzz steps. Each CLAP job validated and fuzzed all four exports.

At this baseline, M1 remained open for the useful-DSP cost comparisons subsequently recorded above. No production DAW scenario or representative performance run was repeated at this revision. This evidence-only follow-up records the code revisions above, not its own later documentation commit; API and CHSS compatibility remain provisional.

### Fixed-resource capacity and reset failures — 2026-11-12

At code revision `f5841f64866fe41633de25bdea90db5eaa2cf3b1` on macOS 26.7.2 arm64:

- 278 workspace tests, including doctests, passed on stable Rust 1.99.1 and CI-baseline Rust 1.98.0. Formatting, strict all-feature/all-target Clippy on both compilers, warning-denied rustdoc, release workspace build, first-party local Markdown paths, and diff checks passed. Local `cargo-deny` 0.19.0 passed with unused-license-allowance warnings; `cargo-machete` 0.9.1 found no unused dependencies. Package versions, dependencies, audited Clack source, and the corrected validator pin are unchanged.
- Three actual-entry state regressions failed before the fix: unselected oversized choice IDs, custom loads that fit selected values but leave insufficient room for later choices, and migrations expanding beyond the fixed save budget. Admission now reserves the longest UTF-8 ID of every independent choice with each custom candidate before publication. Tests cover per-payload and combined limits, unchanged state on rejection, an exact-limit save after both controls change, and byte-identical reopen. Migration/custom semantic admission is checked independently from deployment capacity rejection.
- Allocation-free codec size preflight shares encoding admission and matches actual written byte lengths for empty, all-value, and multibyte documents. Existing golden bytes remain unchanged. The new admission work runs off-thread; scalar callbacks do not inspect or own custom payloads, and save serializes accepted state unchanged.
- An oversized SysEx attempt reproduced leaving byte capacity available for another payload. It now exhausts the remaining byte budget before returning rejection, never reaches the sink, and does not prevent a short MIDI event while count capacity remains.
- A reset panic reproduced subsequent product DSP entry through the actual process ABI. Both precision wrappers now retain a failure latch through reset or post-reset tail-query unwinds, reject further start/process calls, and skip repeated product reset calls. All four wrapper/panic-stage cases preserve the prior tail, retain processor ownership until deactivation, destroy exactly once, and allow fresh activation and successful reset. This protects failure containment, not realtime-safe panic unwinding or safe continuation of damaged DSP.
- All four rebuilt packaged artifacts passed the unchanged corrected pinned validator and separate five-second, two-worker fuzz runs: conformance **35 passed / 0 failed / 9 skipped**, delayed probe **21 / 0 / 23**, note probe **37 / 0 / 7**, MIDI probe **34 / 0 / 10**. A fresh installer build passed all three validator binary regressions.

Hosted [Rust CI](https://github.com/nijaru/chassis/actions/runs/43178602504) and [CLAP Conformance](https://github.com/nijaru/chassis/actions/runs/43178602515) succeeded at this exact code revision. All seven Rust jobs and all three Linux/macOS/Windows CLAP jobs were checked individually; each CLAP job packaged, validated, and fuzzed all four exports.

M1 remains open for the failure cases and fixed-resource public-API/authoring-cost proof in [the execution plan](../next-runtime-slice.md#1-close-statecontrol-validity-first). No production DAW scenario, representative performance run, or Miri qualification was repeated. This evidence-only follow-up records the code revision above; API and CHSS compatibility remain provisional.

### Independent parameter-domain state validity — 2026-11-12

At code revision `5d15cc5d3386475a6b198d32dd4f2451e6796d03` on macOS 26.7.2 arm64:

- 272 workspace tests, including doctests, passed on stable Rust 1.99.1 and CI-baseline Rust 1.98.0. Formatting, strict all-feature/all-target Clippy on both compilers, warning-denied rustdoc, release workspace build, first-party local Markdown paths, and diff checks passed. Local `cargo-deny` 0.19.0 passed with unused-license-allowance warnings; `cargo-machete` 0.9.1 found no unused dependencies. Dependencies, package versions, audited Clack source, and the validator pin and its three patches are unchanged.
- Direct-control and successful-automation regressions reproduced save succeeding followed by same-policy reload rejection before the fix. An actual CLAP-entry regression independently reproduced inactive flush accepting `0.75` in a declared `0..=1` domain, saving it, then rejecting it on reopen. All now preserve exact desired state through reopen and activation. Direct embedding also checks the selected DSP output after activation.
- `Component::validate_custom_state` replaces complete parameter/custom validation without a shim. Descriptor domains define independently valid parameters; custom policy still rejects missing, unknown, and invalid custom fields before either candidate is published. Borrowed-definition admission, schema drift, adjacent migrations, and failure-atomic rejection remain covered.
- The absolute-selector fixtures now require both addressable table slots. Old and new outputs distinguish leaked controls without invalid addressing or a fallback counter. Reversion, reset, desired-only edits, held modulation, in-flight completion races, and 1,000-block pending-resource allocation/deallocation checks remain protected. The parameter-only fixture also now activates its own valid defaults; that assertion reproduced its former fixture-only panic before correction.
- All four rebuilt packaged artifacts passed the unchanged corrected pinned validator and separate five-second, two-worker fuzz runs: conformance **35 passed / 0 failed / 9 skipped**, delayed probe **21 / 0 / 23**, note probe **37 / 0 / 7**, MIDI probe **34 / 0 / 10**. A fresh installer build passed all three validator binary regressions.

Hosted [Rust CI](https://github.com/nijaru/chassis/actions/runs/43162535324) and [CLAP Conformance](https://github.com/nijaru/chassis/actions/runs/43162535333) succeeded at `13f4c1c763c3ce6f7ee67e9c1e0bab5c1215ba1e`, which differs from the code revision above only by the initial qualification record. All seven Rust jobs and all three Linux/macOS/Windows CLAP jobs were checked individually. Each CLAP job packaged, validated, and fuzzed all four exports.

This closes the semantic state-validity slice, not M1 or the core-stability gates. Lifecycle/failure and capacity qualification remain, including checking fixed deployment state budgets across all admitted choice IDs and custom payloads rather than defaults alone. No production DAW scenario, representative performance run, or Miri qualification was repeated. This evidence-only follow-up records the revisions above, not its own later documentation commit; API and CHSS compatibility remain provisional.

### DSP authoring API redesign — 2026-10-30

At code revision `99fc0c664d3f8b9b3df253a3aa0b399cc8b005cc` on macOS 26.6.2 arm64:

- 268 workspace tests, including doctests, passed on stable Rust 1.99.1 and CI-baseline Rust 1.98.0. Formatting, strict all-feature/all-target Clippy on both compilers, warning-denied rustdoc, release workspace build, and diff checks passed. Local `cargo-deny` 0.19.0 passed with unused-license-allowance warnings; `cargo-machete` 0.9.1 found no unused dependencies. Dependencies, package versions, audited Clack source, and the validator pin and its three existing patches are unchanged.
- Safe channel enum views support simultaneous separate input/output slices. Scoped lazy iterators also permit multiple channel views to coexist for linked DSP without collecting channels or copying input to scratch. A split-source mid/side transform exercises this borrowing contract; a compile-fail doctest prevents sample views escaping the traversal. Complete topology and sample dimensions share one activation-bounded admission pass, without repeated context validation.
- Typed parameter handles resolve kind, float domain, and modulation capability during setup. Tests cover unknown/wrong-type lookup, typed values, discrete timing/source order, and invalid cursor queries. The exported gain fixture consumes typed float/Boolean trajectories, while resource-backed tests exercise frozen consumed base/modulation independently of newer desired controls. Existing f32/f64 callback instrumentation measures zero allocations/deallocations for accepted processing and dimension/topology rejection, including pending-resource callbacks using typed handles.
- Complete instance persistence remains failure-atomic across identity/schema checks, migrations, parameter completeness, custom policy, and deferred reactivation. The CHSS golden fixture retains byte-identical encoding. Parameter-only store persistence is removed rather than retained as an alternate partial-load path.
- Default-Stacked-Borrows Miri `0.1.0 (16d1ffac17 2026-10-26)` passed 13 focused checks: the scoped cross-channel test, both raw f32/f64 topology cases, eight semantic-state/resource-transition tests, and two actual metadata ABI tests covering both precision factories. The latter check every descriptor field, optional defaults, feature termination, and retained C-string ownership. This is focused boundary evidence, not whole-workspace or whole-Clack qualification.
- All four rebuilt packaged artifacts passed the unchanged corrected pinned validator and separate five-second, two-worker fuzz runs: conformance **35 passed / 0 failed / 9 skipped**, delayed probe **21 / 0 / 23**, note probe **37 / 0 / 7**, MIDI probe **34 / 0 / 10**.

Hosted [Rust CI](https://github.com/nijaru/chassis/actions/runs/41112539280) and [CLAP Conformance](https://github.com/nijaru/chassis/actions/runs/41112539279) succeeded at this exact code revision. All seven Rust jobs and all three Linux/macOS/Windows CLAP jobs were checked individually. Each CLAP job packaged, validated, and fuzzed all four exports.

The redesign replaces unpublished APIs without compatibility shims. Typed handles remain schema-local, not portable cross-schema identities. Channel traversal uses scoped dynamic dispatch rather than exposing backend-source generics; no new performance claim is made. No production DAW scenario or representative performance run was repeated. This evidence-only follow-up records the code revision above, not its own later documentation commit.

### Musical block-start transport — 2026-10-30

At code revision `95ad51326028c8a9d9c22c2f42d5927461b5785e` on macOS 26.6.2 arm64:

- 262 workspace tests, including doctests, passed on stable Rust 1.99.1 and CI-baseline Rust 1.98.0. Formatting, strict all-feature/all-target Clippy on both compilers, warning-denied rustdoc, release workspace build, local Markdown paths, and diff checks passed.
- Local `cargo-deny` 0.19.0 passed with unused-license-allowance warnings; `cargo-machete` 0.9.1 found no unused dependencies. Dependencies, package versions, audited Clack source, and the pinned validator and its three existing patches are unchanged.
- Core tests cover independently available signed timelines, known inactive loops, equal endpoints, and rejection of invalid tempo/slope, positions, meter, and loop ranges. Existing runtime callers now borrow validated snapshots rather than copying transport into each context.
- Three actual process-ABI cases cover every projected field, independent flags, negative/stopped positions, seeks, null transport, unavailable poisoned storage, and no fabricated project sample clock. Invalid optional BPM and its dependent slope become unavailable, preserving the adapter's prior recovery contract. Malformed available meter, active loops, or slope with valid BPM reject before DSP; source fixed-point ordering catches reversed endpoints even when floating projection would collapse them. Accepted and rejected callbacks measured zero allocations/deallocations, and rejection preserved untouched output.
- Default-Stacked-Borrows Miri `0.1.0 (16d1ffac17 2026-10-26)` passed all three actual ABI cases, including the final fixture lifecycle cleanup. An optional native source-order mutation probe was blocked before `main` in macOS `dyld`, including with `--list`; no failing-before result is claimed. Source was restored before final Miri and Clippy checks.
- All four rebuilt packaged artifacts passed the unchanged corrected pinned validator and separate five-second, two-worker fuzz runs: conformance **35 passed / 0 failed / 9 skipped**, delayed probe **21 / 0 / 23**, note probe **37 / 0 / 7**, MIDI probe **34 / 0 / 10**.

Qualification corrected loop availability at the adapter: inactive CLAP endpoints are unspecified and deliberately poisoned by the validator. Fuzzing also exposed advertised negative BPM after decreasing ramps; CLAP's header does not specify a positive bound. Retaining the existing optional-BPM recovery, with direct adversarial ABI coverage and strict core validation, resolved this without changing the generator or removing decreasing-tempo coverage.

Hosted [Rust CI](https://github.com/nijaru/chassis/actions/runs/41048341806) and [CLAP Conformance](https://github.com/nijaru/chassis/actions/runs/41048341782) succeeded at this exact code revision. All seven Rust jobs and all three Linux/macOS/Windows CLAP jobs were checked individually. The final fixture ran natively in hosted workspace tests; each CLAP job packaged, validated, and fuzzed all four exports with the unchanged corrected validator.

This qualifies block-start observations, not timestamped `CLAP_EVENT_TRANSPORT` updates, a transport clock engine, or a new format. No production DAW scenario or representative performance run was repeated. This evidence-only follow-up records the code revision above; it does not qualify its own later documentation commit.

### MIDI 2/UMP transport — 2026-10-30

At code revision `88c884b8e6974de7db433a8759fcc65e3f4b0ff7` on macOS 26.6.2 arm64:

- 258 workspace tests passed on stable Rust 1.99.1 and CI-baseline Rust 1.98.0. Formatting, strict all-feature/all-target Clippy on both compilers, warning-denied rustdoc, release workspace build, shell syntax, local Markdown paths, and diff checks passed. The initial hosted run caught a missing rustdoc backtick; the final revision corrects it without changing runtime behavior.
- Local `cargo-deny` 0.19.0 passed with unused-license-allowance warnings; `cargo-machete` 0.9.1 found no unused dependencies. Dependency versions, package versions, and the audited Clack revision are unchanged.
- Eight actual exported MIDI ABI cases cover the prior MIDI 1/SysEx path plus UMP. All 16 message-type nibbles round-trip arbitrary four-word storage, including unused words, through two independently mapped routes with explicit timestamps and both defined annotations. Mixed short/SysEx/UMP/control traffic preserves equal-offset list order and cross-block control completion.
- Exact-size, flags, timing, unknown-port, and incompatible-dialect cases reject the whole batch before DSP. Fatal host rejection preserves prior output without completing controls. Core tests cover per-message input/output capabilities, shared note/MIDI ordering, UMP surviving resource-gated parameter withholding, and essential failure retaining neither base/modulation endpoints nor new tail metadata. A MIDI 2-only setup admits UMP with zero SysEx capacity but rejects missing or excessive event capacity.
- Accepted 1,024-UMP and mixed short/SysEx/UMP callbacks, independent overflows, and the existing simultaneous 2,048-record maximum batch allocated and deallocated nothing. Fixed UMP words use the common MIDI count bound, not SysEx bytes. The direct-runtime mixed callback probe includes UMP across 1,000 successful callbacks and essential-output failure.
- Default-Stacked-Borrows Miri `0.1.0 (16d1ffac17 2026-10-26)` passed seven focused actual ABI cases, two private MIDI projection cases, MIDI 2-only zero-SysEx setup, and the zero-audio regression. Maximum-size stress ran natively on both compilers, not under Miri; focused invalid-nonnull over-budget payload rejection remains included.
- The isolated validator's MIDI 2-only port-selection regression failed its original generator's 1,024-retry panic and passed the correction. A fresh installer run applied all three reviewed patches and passed all three binary regressions on Rust 1.98.0. The correction adds no UMP or SysEx generation; fidelity evidence comes from the actual ABI/Miri cases, not validator traffic.
- All four rebuilt packaged artifacts passed the corrected pinned validator and separate five-second, two-worker fuzz runs: conformance **35 passed / 0 failed / 9 skipped**, delayed probe **21 / 0 / 23**, note probe **37 / 0 / 7**, MIDI probe **34 / 0 / 10**.

Hosted [Rust CI](https://github.com/nijaru/chassis/actions/runs/41023972175) and [CLAP Conformance](https://github.com/nijaru/chassis/actions/runs/41023972106) completed successfully at this exact code revision. Every one of seven Rust jobs and all three Linux/macOS/Windows CLAP jobs was checked individually; the latter packaged, validated, and fuzzed all four exports with the isolated corrected validator.

The [MIDI 2 boundary audit](../research/clack-pinned-adapter-audit.md#midi-2ump-boundary--reviewed-2026-10-30) records the same-pin `Midi2Event` C-layout correction and opaque numeric-word projection. This qualifies bounded four-word transport, not UMP stream parsing, MIDI 2 device/protocol compliance, or new production DAW/performance support. No production DAW or representative performance run was repeated. This evidence-only follow-up records the code revision above; it does not qualify its own later documentation commit.

### MIDI 1/SysEx and shared output — 2026-10-30

At code revision `b833b02989fb0d3317e119775f9e89b4bb1e8f97` on macOS 26.6.2 arm64:

- 254 workspace tests passed on stable Rust 1.99.1 and CI-baseline Rust 1.98.0. Formatting, strict all-feature/all-target Clippy on both compilers, warning-denied rustdoc, release workspace build, shell syntax, local Markdown paths, and diff checks passed.
- Local `cargo-deny` 0.19.0 passed with unused-license-allowance warnings; `cargo-machete` 0.9.1 found no unused dependencies. Hosted policy tooling used the configured newer pins.
- Seven actual exported MIDI ABI tests covered opaque fixed bytes and SysEx boundaries, independent port maps, both defined flags, equal-time controls, cross-block completion, malformed/over-budget rejection before DSP, and fatal first/later output backpressure. The host deep-copied payloads during push and retained their contents after source mutation/destruction. Maximum accepted/rejected callbacks allocated and deallocated nothing, including 1,024 MIDI records, 64 KiB SysEx, 512 controls, and 512 ignored records in one accepted 2,048-record batch.
- Default-Stacked-Borrows Miri `0.1.0 (16d1ffac17 2026-10-26)` passed six focused actual ABI cases, two private MIDI projection cases, and the zero-audio regression. The maximum-count stress case ran natively on both compilers, not under Miri. The focused over-budget case retained Miri protection for scalar rejection before even a nonnull invalid payload loan.
- The zero-audio regression failed the original sample-precision discovery. Empty topology now preserves the actual callback frame count without requiring dummy sample storage; unexpected audio descriptors still reject. An independent source review found no actionable core/adapter correctness findings; this is not a whole-Clack safety qualification.
- All four packaged artifacts passed the reviewed pinned validator and separate five-second, two-worker fuzz runs: conformance **35 passed / 0 failed / 9 skipped**, delayed probe **21 / 0 / 23**, note probe **37 / 0 / 7**, MIDI probe **34 / 0 / 10**. SysEx lifetime/ownership evidence comes from the actual ABI/Miri cases, not the validator's generator.

Hosted [Rust CI](https://github.com/nijaru/chassis/actions/runs/41015851376) and [CLAP Conformance](https://github.com/nijaru/chassis/actions/runs/41015851298) completed successfully at this exact code revision. Every one of seven Rust jobs and all three Linux/macOS/Windows CLAP matrix jobs was checked individually. The latter packaged, validated, and fuzzed all four exports.

The [MIDI boundary audit](../research/clack-pinned-adapter-audit.md#midi-1sysex-boundary--reviewed-2026-10-30) records the same-pin MIDI layout correction, scoped payload loans, C provider obligations, and unknown-flag rejection. MIDI 2/UMP was outside that revision's support. No production DAW or representative performance run was repeated; this record does not renew those older claims.

### Native notes, expression, and voice feedback — 2026-10-30

At code revision `8996b83b1bc018bfc4e3e2212c23e757f37f1cb4` on macOS 26.6.2 arm64:

- 234 workspace tests passed on stable Rust 1.99.1 and CI-baseline Rust 1.98.0. Formatting, strict all-feature/all-target Clippy on both compilers, warning-denied rustdoc, local Markdown paths, shell syntax, and release workspace build passed;
- `cargo deny check` 0.19.0 passed with unused-license-allowance warnings; `cargo machete` 0.9.1 found no unused dependencies. The new first-party fixture has a package-specific AGPL exception, without widening third-party license policy;
- core tests cover explicit mixed source order versus independent streams, withheld trajectories retaining note order, seven expression domains, bounded caller-owned output, direction/dialect checks, backpressure, and product errors committing neither endpoints nor new tail metadata;
- actual exported instrument tests cover native metadata and reversed host/schema port order, output-only f32/f64 audio, onset-sensitive mixed controls/expressions, zero-velocity on, exact raw wildcard admission, malformed records and overflow before DSP, original input-port feedback, and essential output rejection. Maximum 1,024-note, 512-parameter/512-note, and 1,024-expression callbacks in both precisions measured zero allocations/deallocations, as did malformed/over-bound rejection;
- the cross-block voice-end regression failed the earlier per-block debt clearing. Deferred wildcard feedback now survives until a matching live retrigger finishes. The expression round-trip regression failed without Clack's expression-ID decoding correction;
- default Stacked-Borrows Miri 0.1.0 (`16d1ffac17`, 2026-10-26) passed the native egress cast/round-trip test and seven selected actual-entry ABI cases, including both precisions, strict admission, deferred feedback, and output rejection. Maximum-load tests ran natively, not under Miri; these are focused checks, not whole-workspace or whole-Clack qualification;
- rebuilt packaged validator results were conformance 35 passed / 0 failed / 9 skipped, delayed probe 21 / 0 / 23, and note probe 37 / 0 / 7. All three five-second, two-worker fuzz runs passed. The unchanged validator pin includes both reviewed generator corrections; its native-expression domain regression failed the original ranges before correction.

Hosted [Rust CI](https://github.com/nijaru/chassis/actions/runs/41000008563) and [CLAP Conformance](https://github.com/nijaru/chassis/actions/runs/41000008593) succeeded at this exact source revision. All seven Rust jobs passed, including Loom and both portable-host jobs. Every Linux/macOS/Windows CLAP matrix job built the isolated corrected validator, packaged all three exports, and passed validation and bounded fuzz.

The [Clack audit](../research/clack-pinned-adapter-audit.md#native-note-boundary--reviewed-2026-10-30) records the same-pin expression layout/decoding delta and unused host-side note-port vtable limitation. No production DAW scenario or representative performance benchmark was rerun. MIDI dialects, per-note parameter modulation, scheduling/activity hints, and prepared resource adoption remain outside this qualification.

### Tail and timed bypass — 2026-10-30

At code revision `5d03a0f62f33af67b255a7abf9c537dca0b41555` on macOS 26.6.2 arm64:

- 215 workspace tests passed on stable Rust 1.99.1 and CI-baseline Rust 1.98.0. Formatting, strict all-feature/all-target Clippy on both compilers, warning-denied rustdoc, local Markdown paths, and release workspace build passed;
- `cargo deny check` 0.19.0 passed with unused-license-allowance warnings; `cargo machete` 0.9.1 found no unused dependencies. Hosted CI passed its separately pinned tools;
- core tests cover explicit unique Boolean bypass identity, stepped timing/source order, completed tail snapshots, failed validation and tail-hook panic atomicity, and an echo product preserving dry latency/existing wet tail through bypass and deferred custom-resource replacement;
- actual exported gain tests cover role flags, inactive/active flush, timed bypass, state/reset, mixed 512-event controls, overflow rejection, and 1,000 maximum-event callbacks in each precision with zero measured allocations/deallocations;
- tail ABI tests cover initialized inactive queries, activation publication before reentrant latency notification, main-thread queries during mutable DSP, synchronous notification re-entry, projected-value changes, reset, and 1,000 allocation/deallocation-free dynamic callbacks. The unstarted-activation notification regression failed before instance-owned debt replaced processor-local debt;
- default Stacked-Borrows Miri 0.1.0 (`16d1ffac17`, 2026-10-26) passed the full `host_tail` test and `exported_bypass::readonly_output_tables_preserve_host_ownership`. The latter checks original callback/table/context identity, success/failure propagation, safe local descriptor replacement, and actual inactive/active raw flush;
- rebuilt packaged conformance: 35 passed / 0 failed / 9 skipped; delayed probe: 21 passed / 0 failed / 23 skipped. Both five-second, two-worker fuzz runs passed with the unchanged pinned validator and reviewed generator correction.

Fuzzing exposed subnormal pass-through in bypass. The fixture now applies its normal subnormal cleanup to the unity-gain dry path; f32/f64 regression inputs and both originally failing seeds passed after correction. This is product policy, not a framework bypass algorithm.

The [Clack audit](../research/clack-pinned-adapter-audit.md) records the independently reviewed read-only descriptor, typed-event provenance, and shared tail-getter boundaries. The Clack revision is unchanged; common/plugin/extensions carry the documented local corrections. The Miri tail fixture owns raw audio tables rather than qualifying the pinned host's multiport builder. These are focused Miri results, not a whole-Clack or whole-workspace claim.

Hosted [Rust CI](https://github.com/nijaru/chassis/actions/runs/40990508543) and [CLAP Conformance](https://github.com/nijaru/chassis/actions/runs/40990508501) succeeded at this exact source revision. Every job passed, including all Linux/macOS/Windows workspace and packaged validator/fuzz jobs.

No production DAW scenario or representative performance benchmark was rerun. Note/event-port deployment, scheduling/activity hints, prepared resource adoption, and arbitrary product DSP remain outside this qualification.

### Global additive float modulation — 2026-10-24

At code revision `f7459c6aabc8961302b850e7ea5d96571cd87884` on macOS 26.6.2 arm64:

- 202 workspace tests passed; local stable Rust 1.99.1 and CI-baseline Rust 1.98.0 suites and final focused checks passed. Formatting, strict all-feature/all-target Clippy on both compilers, warning-denied rustdoc, local Markdown links, and release workspace build passed;
- `cargo deny check` 0.19.0 passed with unused-license-allowance warnings; `cargo machete` 0.9.1 found no unused dependencies. Hosted CI additionally passed its pinned tool versions;
- core tests cover explicit Float-only admission, finite unrestricted amounts, independent base ramps and modulation steps, equal-offset source order, effective clamping including finite overflow, held lifetime, state exclusion/load preservation, and mixed-batch failure atomicity;
- actual-entry tests cover global-only flags, audible f32/f64 mixed controls, inactive/active flush, reset/reactivation, unmodulated get/save values, fresh-instance reopening, strict raw targeting including `-2`, known invalid/non-finite/unsupported controls, malformed record sizes, and combined process-event overflow;
- resource-backed core/CLAP tests freeze consumed base and modulation through deferred restart, reset, loads/reversion, and active flush while retaining desired modulation for reactivation. A paused old block's accepted modulation survives a newer state load that rejects its base CAS;
- 1,000 f32 and 1,000 f64 callbacks for each of all-base and maximum-bound mixed controls measured zero allocations/deallocations and realtime host-log calls. The pending-resource probe also measured 1,000 mixed-control callbacks with reset, without allocation/deallocation, resource destruction, or retargeting;
- rebuilt packaged conformance: 35 passed / 0 failed / 9 skipped; delayed probe: 21 passed / 0 failed / 23 skipped. Both five-second, two-worker fuzz runs passed using the exact validator pin plus the reviewed correction described below. Its added generator regression failed the original code and passed the correction.

Deliberate regressions failed when conflating modulation with base points, consuming desired instead of frozen modulation, validating targeting before ignoring unknown IDs, or making held-signal retention depend on successful base completion. All mutations were restored. Independent review also identified Windows line-ending requirements for the new installer/patch; explicit LF attributes passed a `core.autocrlf=true` checkout probe and the actual hosted Windows job.

Hosted [Rust CI](https://github.com/nijaru/chassis/actions/runs/40119880419) and [CLAP Conformance](https://github.com/nijaru/chassis/actions/runs/40119880386) succeeded at this exact source revision, with every Linux/macOS/Windows job passing, including the corrected validator regression, packaged validation, and bounded fuzz. The user-installed validator and the audited Clack pin/local process patch remain unchanged.

No production DAW scenario, representative performance benchmark, or new Miri run was performed. CHSS bytes and raw sample-reference construction are unchanged. This qualifies global additive float modulation, not per-note/expression, other parameter domains, or arbitrary product DSP.

### Validator source and local fix

The qualification tool remains `clap-validator` 0.4.1 at `b2f1d9b79b1d264a5747f46707d72b1aa40a02ef`, with the reviewed [global-modulation patch](../../vendor/clap-validator/global-modulation.patch). The pin's [`ParamFuzzer::generate_event`](https://github.com/free-audio/clap-validator/blob/b2f1d9b79b1d264a5747f46707d72b1aa40a02ef/src/tests/rng.rs#L863-L891) constructs base-value events containing signed modulation amounts. For a `0..1` gain this sends invalid negative base values; strict plugin rejection correctly fails the unmodified `param-fuzz-modulation` test. Do not loosen parameter admission or exclude that test to compensate.

That patch changes only the global modulation branch's event variant, C struct/header, and amount field, plus a focused seeded regression that fails the original generator. It does not change assertions, ranges, test selection, or production code.

Native-note qualification exposed a second generator defect: tuning was sampled from `-128..=128` instead of CLAP's `-120..=120`, and volume admitted zero despite `0 < x <= 4`. The [native-note range patch](../../vendor/clap-validator/native-note-ranges.patch) corrects only these bounds and adds a fixed-seed 20,000-event domain regression. It fails the original code on tuning `127.85912075812644` and passes the correction. Strict plugin validation remains unchanged; no validator test is excluded or weakened.
MIDI 2-only ports exposed a third generator defect: the legacy generator can select a port supporting neither native notes nor MIDI 1 and exhaust its 1,024 retries on inapplicable parameter events. The [port-selection patch](../../vendor/clap-validator/midi2-port-selection.patch) selects only ports for which it can generate note traffic, retaining original host indices. Its seeded regression fails the previous generator's panic and covers MIDI 2-only and interleaved mixed configurations. This adds no UMP or SysEx generation; actual-entry tests must establish that fidelity separately. All three patches retain upstream MIT terms and the original license notice; the tool does not ship in plugin artifacts.

[The installer](../../.github/scripts/install-clap-validator.sh) fetches the exact workflow pin, requires all three patches to apply, runs all binary unit regressions on Rust 1.98.0, and installs into `target/clap-validator/bin` without replacing the user's CLI. All three platform jobs call that same executable for validation and fuzzing. Re-audit and remove each patch when a pinned upstream replacement corrects its generator; historical results above/below must be read with their recorded source delta.

### Parameter control policy — 2026-10-24

At code revision `9f17ac3` on macOS 26.6.2 arm64:

- 196 workspace tests passed with stable Rust 1.99.1 and CI-baseline Rust 1.98.0; formatting, strict all-feature/all-target Clippy on both compilers, warning-denied rustdoc, local Markdown links, and release workspace build passed;
- `cargo deny check` 0.19.0 passed with unused-license-allowance warnings; `cargo machete` 0.9.1 found no unused dependencies. Local tools differ from CI's pins;
- core tests cover nonlinear controls, author-owned units/precision, unchanged plain-unit state, extreme/subnormal/constant float domains, exact wide-integer step rounding, full-width integer text, invalid policy results, sticky writer errors, and configuration-based schema drift;
- actual-entry tests share the exported percent policy, preserve stepped plain conversion, accept disambiguated custom choice syntax, and reject truncated multibyte text, embedded NULs, or swallowed writer failures;
- existing 1,000 f32 and 1,000 f64 bounded-event callbacks still measured zero allocations/deallocations and realtime host-log calls with author control policy attached;
- rebuilt packaged conformance: 35 passed / 0 failed / 9 skipped; delayed probe: 21 passed / 0 failed / 23 skipped. Pinned `clap-validator` 0.4.1 passed both five-second, two-worker fuzz runs.

Regressions failed when restoring adapter whitespace trimming, bypassing text-buffer checks, ignoring policy configuration equality, or replacing subnormal-safe interpolation. Final review also exposed wide integer half ties reversed by converting the span to `f64`; the added regression failed before exact binary-coordinate rounding replaced that calculation. Independent review checked the bounded integer-product proof and boundary cases.

Hosted [Rust CI](https://github.com/nijaru/chassis/actions/runs/40068376120) and [CLAP Conformance](https://github.com/nijaru/chassis/actions/runs/40068376228) succeeded at `9f17ac3`, with every Linux/macOS/Windows job passing, including CI's pinned dependency tools and packaged validator/fuzz matrix.

No production DAW scenario, representative performance benchmark, or new Miri run was performed. CHSS bytes and raw sample-reference construction are unchanged. Framework normalized control policy does not imply a custom CLAP host-knob taper or a new editor integration.

### Complete CLAP custom-state projection — 2026-10-24

At code revision `75202f9` on macOS 26.6.2 arm64 with stable Rust 1.99.1:

- workspace tests: 189 passed; formatting, strict all-feature/all-target Clippy, warning-denied rustdoc, local Markdown links, and release workspace build passed;
- CI-baseline Rust 1.98.0 workspace tests and strict Clippy passed locally;
- `cargo deny check` 0.19.0 passed with unused-license-allowance warnings; `cargo machete` 0.9.1 found no unused dependencies. Local tools differ from CI's pins;
- actual-entry resource-backed state tests cover original custom defaults, component migration/validation, failure-atomic rejection, immediate restart notification, deferred processing, active flush, reset/repeated loads/reversion (including before callback observation), save/reopen, and resource preparation on reactivation;
- 1,000 pending resource-backed f32 callbacks with parameter input and reset measured zero allocations/deallocations, no retargeting or destruction of old DSP resources, and unchanged saveable desired state;
- a load during paused DSP saves the accepted complete replacement, defeats old endpoint publication, and retains the last old block's local parameter baseline until reactivation;
- production-helper tests cover custom-epoch gating, scalar-write epoch preservation, and complete-state rejection at terminal generation exhaustion; Loom models cover coherent scalar/epoch snapshots and existing bounded/CAS ordering;
- deployment admission tests preserve original defaults, reject incompatible retained policy/schema drift, and reject defaults outside the bounded wire budget;
- rebuilt/package-qualified conformance: 35 passed / 0 failed / 9 skipped; delayed probe: 21 passed / 0 failed / 23 skipped. Pinned `clap-validator` 0.4.1 (`b2f1d9b79b1d264a5747f46707d72b1aa40a02ef`) passed both five-second, two-worker fuzz runs.

Actual-entry regressions failed when bypassing the custom-epoch guard, forwarding withheld trajectories, or reacquiring a newer publication token after DSP. Design review exposed the contention case: an unreadable baseline also withholds automation for that block, without permanently latching the resource gate. Custom ownership and reclamation remain off-callback; this does not qualify prepared resource adoption or background tasks.

Hosted [Rust CI](https://github.com/nijaru/chassis/actions/runs/40064059445) and [CLAP Conformance](https://github.com/nijaru/chassis/actions/runs/40064059500) succeeded at `75202f9`, with every Linux/macOS/Windows job passing, including CI's pinned dependency tools and packaged validator/fuzz matrix.

No production DAW scenario, representative performance benchmark, or new Miri run was performed. Raw sample-reference construction and CHSS bytes are unchanged; arbitrary boxed errors and DSP panics remain outside realtime qualification.

### Component-owned complete state — 2026-10-23

At code revision `7620284` on macOS 26.6.2 arm64 with stable Rust 1.99.1:

- workspace tests: 185 passed; formatting, strict all-feature/all-target Clippy, warning-denied rustdoc, and release workspace build passed;
- CI-baseline Rust 1.98.0 workspace tests and strict Clippy passed locally;
- `cargo deny check` 0.19.0 passed with unused-license-allowance warnings; `cargo machete` 0.9.1 found no unused dependencies. Local tool versions differ from CI's pins;
- resource-backed core state proof covers validated defaults without persistence identity, complete replacement/rejection, component migration, schema admission, desired-state save/reopen, deferred-reactivation processing with coupled table addressing, reset/reversion, and deterministic reactivation;
- 1,000 pending resource-backed callbacks with float ramp/choice input and reset measured zero allocations/deallocations, retained the saveable desired generation, and did not retarget old resources;
- actual-entry CLAP state tests cover scalar component migration/validation, atomic rejection, schema drift, and custom defaults differing between temporary factory and retained main-thread definitions;
- packaged conformance: 35 passed / 0 failed / 9 skipped; delayed probe: 21 passed / 0 failed / 23 skipped. Pinned `clap-validator` 0.4.1 (`b2f1d9b79b1d264a5747f46707d72b1aa40a02ef`) passed both five-second, two-worker fuzz runs.

The coupled-resource regression failed when restoring either pre-fix parameter selection or trajectory forwarding. Independent review found the retained-definition custom-default and load-schema admission holes; actual-entry regressions failed when those checks were removed, and both are now enforced by one scalar admission helper. The previous semantic-state fixture was replaced with this resource-backed proof; typed invalid-value/migration-output protection remains in the codec tests, while the new fixture exercises component-owned policy rather than per-load test closures.

Hosted [Rust CI](https://github.com/nijaru/chassis/actions/runs/40030698060) and [CLAP Conformance](https://github.com/nijaru/chassis/actions/runs/40030698084) succeeded at `7620284`, with all Linux/macOS/Windows jobs passing. This includes CI's pinned dependency tools and packaged validator/fuzz matrix.

At this baseline, custom-state deployment remained unsupported. The core transition intentionally discards parameter automation until reactivation; this baseline did not qualify arbitrary product DSP or the later complete-state bridge. No production DAW, representative performance benchmark, or new Miri run was performed; raw reference construction and CHSS bytes are unchanged.

### Realtime rejection and dense projection — 2026-10-23

At code revision `d6e7eff` on macOS 26.6.2 arm64 with stable Rust 1.99.1:

- workspace tests: 182 passed, including dense base-value coherence through control/state/reset/automation, invalid dense-value rejection, stable-ID persistence after choice reordering, and existing lifecycle/panic/concurrency cases;
- formatting, strict all-feature/all-target Clippy, release workspace build, and warning-denied rustdoc: passed;
- CI-baseline Rust 1.98.0 workspace tests and strict Clippy: passed locally;
- `cargo deny check` 0.19.0: passed with unused-license-allowance warnings; `cargo machete` 0.9.1: no unused dependencies. Local tooling differs from CI's pins;
- core allocation instrumentation now also measures dense base-value synchronization and invalid dense-choice rejection over 1,000 callbacks, including after a decoded stable-ID state load, with zero allocations/deallocations;
- actual-entry accepted f32/f64 callbacks and rejected partial-alias callbacks measure zero allocations/deallocations; the host-log-enabled harness observes no realtime log calls. A no-host-log rejection probe also passed while qualifying the local plugin patch;
- packaged conformance: 35 passed / 0 failed / 9 skipped; delayed probe: 21 passed / 0 failed / 23 skipped;
- installed pinned `clap-validator` 0.4.1, revision `b2f1d9b79b1d264a5747f46707d72b1aa40a02ef`: both five-second, two-worker fuzz runs passed.

The actual-entry rejection allocation assertion failed before the fix with one allocation. Independent source reviews found no blocking issue in the narrow vendored plugin delta or dense projection changes. Cargo resolves one patched plugin crate for Chassis, extensions, and the test host. The [Clack audit](../research/clack-pinned-adapter-audit.md) owns source provenance, the reporting delta, and why registry 0.2.0 does not replace it.

Arbitrary boxed errors and DSP panics remain outside realtime qualification. No production DAW scenario, representative performance benchmark, or new Miri run was performed; sample-reference construction is unchanged. Hosted [Rust CI](https://github.com/nijaru/chassis/actions/runs/40022067596) and [CLAP Conformance](https://github.com/nijaru/chassis/actions/runs/40022067685) both succeeded at `d6e7eff` on 2026-10-23. All Linux/macOS/Windows jobs passed, including CI's pinned dependency tools and packaged validator/fuzz matrix.

### Boundary redesign qualification — 2026-10-13

At code revision `b0dce56` on macOS arm64 with stable Rust 1.99.0:

- workspace tests: 181 passed, including whole callback topology, dense-choice atomic rejection/canonical persistence, active custom-state restart/reversion, typed/migrated invalid-state rejection, stepped CLAP conversion, output masks, and exported delay reset;
- formatting, strict all-feature/all-target Clippy, release workspace build, and warning-denied rustdoc: passed;
- `cargo deny check` 0.19.0: passed with unused-license-allowance warnings; `cargo machete` 0.9.1: no unused dependencies. Local tooling differs from CI's pins;
- three focused `raw_audio::tests` passed Miri under default Stacked Borrows and `-Zmiri-tree-borrows`, using nightly 1.101.0 (`080cc2714`, 2026-10-11); a fresh independent source review found no new code-safety blocker;
- accepted actual-entry callbacks, including legal exact-in-place audio, remain allocation/deallocation-free in the measured configurations;
- packaged conformance: 35 passed / 0 failed / 9 skipped; delayed probe: 21 passed / 0 failed / 23 skipped;
- installed pinned `clap-validator` 0.4.1, revision `b2f1d9b79b1d264a5747f46707d72b1aa40a02ef`: both five-second, two-worker fuzz runs passed.

Whole-topology and policy-order regression tests failed at original revision `c936496`; state/adapter regressions also failed before their fixes. The schema-immutability doctest now supplies a valid component definition, with a positive compile probe confirming that forbidden schema replacement is its failure reason. Obsolete callback choice-text validation was removed: dense choice indices now carry that contract, while stable IDs remain validated at setup/persistence boundaries.

**Gap at this baseline (closed by the later local plugin patch):** upstream Clack's process-error reporter allocates/formats for host logging or writes stderr. A temporary warmed actual-entry rejection probe observed one allocation before returning. Valid-callback evidence does not qualify rejected callbacks or arbitrary DSP panics; see the [source audit](../research/clack-pinned-adapter-audit.md). Host pointer provenance/extent, initialization, metadata separation, and concurrency remain obligations.

Local CI-baseline Rust 1.98.0 workspace tests and strict Clippy also passed at test revision `37429d1`, after replacing new float equality assertions with bit-exact checks. Production code is unchanged from `b0dce56`.

Hosted [Rust CI](https://github.com/nijaru/chassis/actions/runs/38460788170) and [CLAP Conformance](https://github.com/nijaru/chassis/actions/runs/38460787972) both succeeded at `37429d1` on 2026-10-13. All Linux/macOS/Windows jobs passed, including CI's pinned dependency tools and packaged validator/fuzz matrix.

No production DAW scenario or representative performance benchmark was rerun. The previous safe-pair benchmark does not measure the new O(C²) raw alias boundary.

### Refresh validation — 2026-10-11

At code revision `e5be210` on macOS arm64 with stable Rust 1.99.0:

- workspace tests: 168 passed, including direct-runtime automation carryover/state/reactivation, invalid-batch atomicity, generation-cache/terminal rejection, and existing Loom/allocation/host cases;
- the core allocation probe now exercises float/integer/boolean/choice completion over 1,000 callbacks, including replacement after a separately allocated control choice and a decoded state load, with zero callback-thread allocations/deallocations;
- formatting, strict all-feature/all-target Clippy, release workspace build, and warning-denied rustdoc: passed;
- CI-baseline Rust 1.98.0 workspace tests and strict Clippy: passed locally;
- `cargo deny check` 0.19.0: passed with unused-license-allowance warnings; `cargo machete` 0.9.1: no unused dependencies. These local versions differ from CI's pinned tools;
- packaged conformance: 35 passed / 0 failed / 9 skipped; delayed probe: 21 passed / 0 failed / 23 skipped;
- installed `clap-validator` 0.4.1 from pinned revision `b2f1d9b79b1d264a5747f46707d72b1aa40a02ef` (verified in Cargo's install metadata): both five-second, two-worker fuzz runs passed.

The direct-runtime carryover test failed before runtime endpoint retention. While implementing retention, the allocation probe caught one last-owner choice deallocation; canonicalizing control/state values to schema-owned IDs fixed it. Earlier custom-field-loss and lossy-integer regressions remain protected. Duplicate serial interleaving models were removed: their writer-serialization/coherent-snapshot protection remains in the stronger Loom weak-memory cases, alongside actual production-helper tests. Callback allocation/work evidence remains the tested configurations, not every possible client. No REAPER scenario or representative performance benchmark was rerun.

Hosted [Rust CI](https://github.com/nijaru/chassis/actions/runs/38122940824) and [CLAP Conformance](https://github.com/nijaru/chassis/actions/runs/38122940822) both succeeded at `e5be210` on 2026-10-11, with all Linux/macOS/Windows jobs successful, including CI's pinned dependency tools and packaged validator/fuzz matrix. Earlier successful runs at `05062c2` and `5bb628f` remain historical evidence.

### Local validation — 2026-09-07

At code revision `bd0a153` on macOS arm64 with Rust 1.98.1:

- workspace tests: 119 passed, including Loom, allocation probes, active-load/automation concurrency, and the schema-replacement compile-fail check;
- formatting, strict all-feature/all-target Clippy, and release workspace build: passed;
- `cargo deny check`: passed (unused license allowances reported as warnings); `cargo machete`: no unused dependencies;
- packaged conformance artifact: 35 passed / 0 failed / 9 skipped;
- packaged delayed probe: 21 passed / 0 failed / 23 skipped;
- pinned `clap-validator` 0.4.1: both five-second, two-worker fuzz runs passed.

The REAPER and performance baselines below were not rerun for this revision.

## Current in-process CLAP evidence

Tests under `crates/chassis-clap/tests/` exercise the actual exported Chassis CLAP entry through pinned Clack host APIs. Narrow raw-descriptor regressions supplement those actual-entry checks at unsafe boundaries.

Current cases include:

- `host_conformance.rs`: f32/f64 callback dispatch, stereo main + stereo sidechain, configured 64-event base/mixed-control bounds, held global modulation and state exclusion, strict targeted/malformed-control rejection, accepted/rejected-callback allocation/deallocation counting and absence of realtime host logging, author-defined percent display/parsing and stepped conversion through display/flush/process, output constant masks, legal raw in-place audio, illegal partial-alias rejection, reactivation, latency callbacks, audio-thread transfer, repeated inactive instances;
- `host_control_text.rs`: custom choice syntax/stable identity, multibyte buffer boundaries, embedded NUL rejection, and formatter errors swallowed by author code;
- `exported_probe_reset.rs`: actual exported delay fixture reset clears history/positions without allocation and retains the activation delay;
- `host_frame_bounds.rs`: activation frame extremes and over-bound rejection;
- `host_activation_failure.rs`: product activation failure and same-instance retry, explicit descriptor feature preservation, and rejection of unsupported event-port schemas;
- `host_latency_audio.rs`: nonzero reported latency matching actual delayed audio;
- `host_restart.rs`: one processor restart request per activation, immediate successful-state-load restart notification before processing, rejected state without restart, unchanged active latency, and reactivation using published state;
- `host_active_state_save.rs`: coherent CLAP state save while processing is active;
- `host_state_policy.rs`: component-owned scalar migrations/validation, atomic load rejection, schema drift, original custom-default preservation, retained-definition policy admission, and deployment wire-budget rejection;
- `stream_descriptor.rs`: read-only input/output stream tables, original descriptor/context forwarding, partial transfers, EOF/errors, absent callbacks, safe wrapper replacement and preserved original callbacks;
- `host_custom_state.rs`: resource-backed complete state through default save, migration/rejection, deferred restart/reset/reversion, active flush, save/reopen, zero-allocation pending callbacks, and a load racing old DSP completion whose accepted modulation survives rejected base completion;
- `host_reentrancy.rs`: plugin -> host -> plugin main-thread re-entry through parameter-rescan and latency-change callbacks;
- `host_panic_containment.rs`: factory/activation-metadata panic containment with preserved controls, byte-identical desired state, exact processor destruction and same-instance retry; reset-failure latching in both precision wrappers; process panic containment and teardown.
- `exported_notes.rs`: native port metadata, f32/f64 mixed input order, onset expressions, exact raw wildcard admission, zero-velocity on, original input-port voice feedback including cross-block debt, host output rejection, and callback allocation/deallocation.
- `exported_midi.rs`: zero-audio MIDI 1/SysEx and four-word MIDI 2/UMP, per-port dialects, independent routes, exact contents/offsets/flags, payload ownership, mixed controls, whole-batch malformed admission, essential output rejection, and allocation-free maximum/overflow bounds.

The full adapter allocation test covers 1,000 f32 and 1,000 f64 callbacks for each of all-base and mixed base/modulation streams, with stereo main + stereo sidechain and exactly the configured 64 parameter controls, plus actual-entry exact-in-place acceptance and partial-alias rejection, with zero measured allocation/deallocation or host-log calls on the callback thread.

## Fixed-resource subset boundary

M1's current proof combines the fixtures below; it does not require each fixture to implement every capability. The dated checks above own revision/platform qualification. [The execution plan](../next-runtime-slice.md#4-consolidate-authoring-and-bounded-costs) carries authoring/cost checks forward into M2–M4.

| Boundary | Qualified behavior and limits | Proof entrypoints |
| --- | --- | --- |
| State/control | Independent parameter domains, custom-state transactions, save/reopen, frozen resource epochs, held external modulation, fixed CLAP save admission and read-only stream-descriptor ownership | `semantic_state.rs`, `host_state_policy.rs`, `host_custom_state.rs`, `stream_descriptor.rs` |
| Lifecycle/failure | Failed activation publishes no child/metadata; contained reset failure disables the activation; failed processing completes no base/modulation/tail endpoints. Already-written audio/history/output is not transactional; abandon damaged DSP | `lifecycle_negative.rs`, `host_panic_containment.rs`, `native_notes.rs`, `tail_bypass.rs` |
| Frames/layouts/buffers | Activation-selected frame and topology bounds, dense endpoint legality, separate/in-place access, accepted exact aliases and rejected partial aliases at the raw boundary | `host_frame_bounds.rs`, `audio_endpoint_runtime.rs`, `layout_sidechain_runtime.rs`, `host_conformance.rs` |
| Events/controls | Declared global controls, native note/expression addressing, opaque MIDI/SysEx/UMP, source-defined mixed order, explicit essential-output rejection retaining prior output. Per-note parameter modulation remains unsupported | `host_conformance.rs`, `exported_notes.rs`, `exported_midi.rs`, `native_notes.rs` |
| Capacity/observations | Independent event/count/byte attempt limits; rejected SysEx charges byte capacity. Telemetry rejects busy/exhausted publication without writes and bounds contended reads; only accepted snapshots replace display state | `exported_midi.rs`, `native_notes.rs`, `telemetry.rs` unit tests, `telemetry_alloc.rs` |
| Public authoring/composition | Effects, instruments, event-only processors and direct embedding. Linked stereo/real-sidechain processing composes two instances through caller-owned audio with timed typed controls and zero measured heap traffic; that workload is separate-buffer `f64`, not a new graph or CLAP fixture | `instrument_runtime.rs`, `exported_notes.rs`, `exported_midi.rs`, `realtime_alloc.rs` and its `support/linked_composition.rs` workload |
| Execution costs | Equivalent useful DSP through bare ordinary slices, direct runtime and actual CLAP; three consumed base/modulation trajectories, event-density and focused topology/exact-alias comparisons. Shared frame-major evaluation avoids repeated channel cursor work. Local batch-average measurements, not callback-tail or arbitrary-scale guarantees | `benches/adapter_overhead.rs`, `tests/benchmark_workloads.rs`; [conditions and results](#realtime-allocation-and-performance) |

This boundary does not qualify portable source-domain ramps/anchors, gestures/control origins, compatible prepared-resource adoption, a DAW, or representative callback-tail latency. M2/M3 and the broader M4 host/compatibility gates remain separate.

## Core conformance

The format-independent proof is split by concern while exercising the same public runtime model:

- `crates/chassis-runtime/tests/conformance.rs`: schema rejection, lifecycle, buffer relationships, process bounds/context, base parameters, sample-accurate automation, invalid events, generic non-flat buffer-source traversal, and rejection of structurally valid but unsupported audio configurations;
- `crates/chassis-runtime/tests/automation_runtime.rs`: final typed endpoints retained after successful DSP, equal-offset last-value order, event-free carryover, block-local ramp continuation, canonical encoded state, subsequent control precedence, reactivation, and invalid-batch failure atomicity;
- `crates/chassis-runtime/tests/modulation.rs`: Float-only explicit opt-in, finite unrestricted amount admission, and clamped finite-overflow composition; `automation_runtime.rs` also covers interleaved base ramps, held lifecycle/state exclusion, and failed mixed-batch atomicity; `semantic_state.rs` covers frozen consumed modulation with desired amounts retained for reactivation;
- `crates/chassis-runtime/tests/parameter_control.rs`: nonlinear mapping, units/precision, extreme and constant numeric domains, distinct discrete/text rounding, wide-integer rounding, full-width integer text, invalid policy outputs, writer failures, and policy-configuration schema drift;
- `crates/chassis-runtime/tests/component_schema_runtime.rs`: one coherent schema snapshot, dense lookup, and schema-drift rejection;
- `crates/chassis-runtime/tests/audio_endpoint_runtime.rs`: active/inactive port, direction, channel-range, and dense endpoint legality before product DSP;
- `crates/chassis-runtime/tests/event_schema_runtime.rs` and `note_runtime.rs`: event-schema authority, dense note-port addressing, wildcard handling, and zero-audio note processing;
- `crates/chassis-runtime/tests/instrument_runtime.rs`: event-driven instrument with no audio input, output-only stereo process buffers, and sample-positioned note on/off behavior;
- `crates/chassis-runtime/tests/multi_output_runtime.rs`: multiple semantically distinct output buses routed by dense `AudioPortIndex` even when callback buffers arrive in a different order;
- `crates/chassis-runtime/tests/layout_sidechain_runtime.rs`: one immutable schema reactivated across accepted mono and stereo+sidechain configurations, activation-derived resources, and policy rejection of a structurally valid unsupported combination;
- `crates/chassis-runtime/tests/dynamic_metadata_runtime.rs`: audio/event keys, names, and capabilities created from runtime-owned metadata, then resolved to dense callback identities without strings in the process path;
- `crates/chassis-runtime/tests/semantic_state.rs`: schema-owned complete semantic state, custom-state validation, transactional publication, and migration;
- `crates/chassis-runtime/tests/state_adversarial.rs`: bounded hostile/truncated input, resource limits, failure atomicity, and current schema-owned complete-state loading;
- `crates/chassis-runtime/tests/lifecycle_negative.rs`: product activation failure, state preservation, inactive post-failure state, successful reuse;
- `crates/chassis-runtime/tests/realtime_alloc.rs`: repeated typed automation completion with callback-thread allocation/deallocation counting, including choice replacement after control edits and decoded state loading; its `support/linked_composition.rs` workload adds real sidechain-driven linked stereo processing, timed typed controls, independently checked composed output, state isolation and off-callback teardown;
- `crates/chassis-runtime/tests/latency.rs`: activation-scoped latency snapshot/stability/removal/recomputation;
- `crates/chassis-runtime/tests/native_notes.rs` and `tail_bypass.rs`: failure-atomic base/modulation/tail completion, including DSP/tail panics after output/history mutation, output attempt bounds and abandoning damaged processors;
- `crates/chassis-runtime/src/telemetry.rs` unit tests and `tests/telemetry_runtime.rs` / `telemetry_alloc.rs`: coherent accepted snapshots, deterministic busy/exhausted/contended outcomes, last-accepted display policy, processor-independent observation lifetime, and callback allocation/deallocation.

Together these fixtures now exercise conventional effects, event-only processors, instruments, multi-output processors, multiple legal layouts/sidechains, dynamic/hosted schema metadata, direct non-plugin runtime use, and CLAP deployment through the same component/runtime ownership model. None required a second lifecycle, richer I/O-policy language, or mandatory helper family.

Grow these public-contract tests as semantics are added; do not create a second conceptual runtime for adapters.

## Component-schema / runtime migration checkpoint — 2026-09-12

The following pre-alpha architecture changes were gated with workspace formatting, full tests, and strict all-target/all-feature Clippy before their semantic commits landed:

- audio stable keys/names migrated to borrowed-or-owned setup metadata while callback endpoints remained dense-only;
- event stable keys/names/dialect lists migrated to borrowed-or-owned setup metadata while note/event callback addressing remained dense-only;
- schema-owned `AudioIoPolicy` added with policy-definition validation, policy drift checks, runtime activation enforcement, and CLAP setup enforcement;
- conventional effect policy restricted to stereo main I/O with optional stereo sidechain rather than silently accepting arbitrary layouts;
- runtime complete-state APIs consolidated on schema-owned identity, deleting runtime explicit-product-ID and parameter-only compatibility surfaces;
- heterogeneous instrument, multi-output, layout/sidechain, and dynamic-metadata fixtures added.

The resulting `ComponentSchema` / `InstanceRuntime` ownership shape is considered a candidate-stable architecture, not a published compatibility promise. Later graph/device/host/deployment clients may still reveal concrete semantic gaps.

## Active state-save evidence

[Parameters/state](parameters-state.md#complete-clap-state-publication) owns the complete-generation save/publication contract, including resource-epoch gating and in-flight DSP completion. Evidence exists at three levels:

1. direct publication unit tests;
2. Loom atomic models;
3. actual-entry CLAP host tests that pause DSP before endpoint publication: scalar save sees both old or both new endpoints, and a complete custom-state load saves its accepted replacement without contamination by old DSP completion.

The dated REAPER active-save scenario below is production-host evidence for that baseline. Refresh it against changed artifacts; it is distinct from the current in-process semantic evidence.

## Realtime allocation and performance

Current mechanical evidence:

- core `InstanceRuntime::process`: zero measured post-activation callback-thread allocation/deallocation;
- full CLAP adapter: zero measured allocation/deallocation under the representative mapped topology/event-bound test;
- frame-count minimum/maximum callbacks succeed and over-bound processing is rejected.

The manual [useful-DSP benchmark](../../crates/chassis-clap/benches/adapter_overhead.rs) shares its [workload](../../crates/chassis-clap/benches/support/workload.rs) with [committed equivalence tests](../../crates/chassis-clap/tests/benchmark_workloads.rs):

```sh
cargo bench -p chassis-clap --bench adapter_overhead --locked -- --verify-only
cargo bench -p chassis-clap --bench adapter_overhead --locked
```

It compares bare trusted kernels, direct runtime and actual CLAP process calls, each with shared frame-major control evaluation or independent per-channel cursor bundles. Useful DSP applies gain, wet/dry and linked ducking with three globally modulated floats. Bare kernels evaluate and retain the same stepped controls inside timing but omit framework admission/publication services; they are not a pre-expanded-curve floor. Every path includes identical main-input refill and rendered-output consumption. Setup and independent audio/state equivalence checks are outside timing.

The primary matrix is stereo main plus stereo sidechain, separate buffers, 32/128/512 frames, `f32`/`f64`, and 0/12/64 combined base/modulation events. Focused 128-frame, zero/bound-event cases add mono, mono sidechain, no sidechain, eight discrete main channels and exact reciprocal in-place pairs. Without a sidechain, the detector uses linked main input. Different detector/topology cases are not isolated adapter-only comparisons. Invalid raw aliases remain rejection tests, not useful-DSP timing workloads.

**Measurement — 2026-11-12, source `8c683fe87b06d0d6e19e0e69c06dbd07e502e1ca`:** Mac15,9 / Apple M3 Max / arm64, 16 logical CPUs, macOS 26.7.2 (25G332), rustc 1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1, `opt-level=3`, thin LTO, 16 codegen units and incremental compilation (inherited host Cargo configuration), no custom Rust flags, 48 kHz, AC power. This was an interactive desktop with background activity, not isolated or frequency-pinned hardware. Shared CI VM timing is not production performance evidence.

Each of two serial full runs produced 228 rows. Each row reports p50/p95/p99 of **24 batch-average callback times**, after 64 warmup callbacks. Iterations per batch are `(262144 / (main_channels * frames)).clamp(64, 2048)`; six path/strategy combinations rotate through every order position four times. With 24 batches, nearest-rank p99 is the maximum batch average, **not individual callback-tail latency**. Across the 228 cells, absolute cross-run p50 differences had median 0.48%, p95 4.12% and maximum 8.94%; these describe repeat variation, not callback percentiles.

Representative 128-frame stereo + stereo-sidechain results, in **µs/callback**. Ranges span both precisions and both runs' batch-average p50s:

| Events | Evaluation | Bare trusted | Direct runtime | CLAP |
| --- | --- | --- | --- | --- |
| 0 | Shared | 0.426–0.428 | 1.511–1.612 | 1.592–1.699 |
| 0 | Independent | 0.544–0.584 | 2.728–2.843 | 2.811–2.942 |
| 12 | Shared | 0.475–0.482 | 2.178–2.190 | 2.383–2.411 |
| 12 | Independent | 0.627–0.653 | 3.995–4.013 | 4.211–4.222 |
| 64 | Shared | 0.553–0.580 | 2.510–2.596 | 3.197–3.309 |
| 64 | Independent | 0.782–0.821 | 4.809–5.030 | 5.498–5.734 |

At 64 events with shared evaluation, the focused 128-frame CLAP p50 ranges were 3.026–3.126 µs for mono + mono sidechain, 3.194–3.238 µs for stereo + mono sidechain, 3.239–3.280 µs without a sidechain, 3.149–3.228 µs for stereo exact in-place, and 3.713–3.773 µs for eight discrete main channels + stereo sidechain. The primary 32- and 512-frame shared CLAP cases spanned 0.535–1.688 µs and 5.828–9.930 µs respectively across event loads, precisions and both runs.

The actionable result is to share effective controls when linked channels use the same trajectories. At 128 frames/64 events, this reduced full `f32` CLAP time by about 1.7× for stereo and 5.3× for eight channels versus independent bundles. Cloning a cursor does not share its mixed-event scan or value evaluation. Independent traversals still need independent cursors; these measurements do not select a universal DSP traversal order or justify a new indexing/span API.

CLAP-minus-runtime time includes raw pointer/alias mapping, event normalization and publication, not an isolated alias-classifier cost. Classification remains bounded O(writable channels × descriptors), quadratic for growing matched I/O; the eight-channel case is not an arbitrary-scale guarantee. These local results establish neither cross-framework superiority, a callback deadline, denormal performance, nor production-host behavior.

**Superseded no-op baseline:** on 2026-09-03, Mac15,9 / M3 Max / arm64, macOS 26.6.2, rustc 1.98.0 (88d9e12ae 2026-08-18), the former stereo + sidechain no-op harness reported 49–53 ns without events and 477–499 ns with 64 events; two runs' medians agreed within roughly 4%. That workload and raw-alias boundary differ from the current implementation. Those numbers cannot establish current control/DSP costs or support the former conclusion that there was no material cost to optimize.

## Lifecycle / negative space

Executable current coverage includes:

- create/init/destroy without activation;
- result-based activation failure + same-instance retry;
- activation panic + same-instance retry;
- activate/start/process/stop/deactivate;
- process panic mapping to host processing failure followed by clean stop/deactivation;
- repeated activation with changed sample rate/block bounds;
- reactivation with a different policy-accepted audio layout;
- rejection of structurally valid but policy-unsupported audio I/O before product activation;
- processor transfer between host audio threads without simultaneous mutation;
- repeated instance creation/destruction;
- callback dimension/event/endpoint rejection before product DSP;
- main-thread synchronous host re-entry during state-rescan and latency-change calls.

Pinned validator/fuzz additionally exercises lifecycle sequencing.

## Panic / FFI boundary

`chassis-runtime` forbids unsafe code. Clack owns raw CLAP ABI lifecycle/descriptor borrowing and panic containment. Chassis owns sample-pointer/alias classification and reference construction in the narrow `chassis-clap/src/raw_audio.rs` boundary; host allocation/concurrency obligations remain explicit. The [audit](../research/clack-pinned-adapter-audit.md) records those invariants and the reviewed local plugin patch that returns process failure without synchronous reporting.

Pinned Clack wraps plugin FFI handlers in `std::panic::catch_unwind`. Chassis executable evidence establishes:

- an activation panic is caught, reported as activation failure, and the plugin remains inactive/reusable;
- a process panic is caught and returned to the host as processing failure;
- the host can then stop and deactivate cleanly.

Do not infer that arbitrary DSP state is valid for continued processing after a panic. Process failure should terminate that processing run.

Test-only allocator instrumentation uses `GlobalAlloc` forwarding with explicit safety comments and does not alter the production unsafe boundary.

Adapter-owned unsafe code is isolated in the raw audio module, with explicit discharged/host invariants and `unsafe_op_in_unsafe_fn` denied. The vendored plugin delta retains Clack's existing FFI access and panic-containment invariants; it adds no sample-reference construction. Focused Miri tests cover f32/f64 exact pairs, disjoint/shared-input topology, partial/cross-channel/cross-port writable rejection, and empty views; native actual-entry tests cover acceptance and rejection. These checks are not a proof for arbitrary invalid C pointers.

## Re-entrancy evidence

The exact Clack pin is required because published 0.1.1 predates the main-thread re-entrancy safety fix. Chassis exercises the relevant behavior directly:

- state load calls the host parameter-rescan extension;
- the host immediately re-enters plugin extension discovery;
- activation calls the host latency-change extension;
- the host immediately re-enters plugin latency extension discovery.

Bitwig remains useful real-world confirmation when available, but main-thread re-entrancy is no longer an untested semantic assumption.

## Latency / PDC evidence

Implemented semantics:

- one processor establishes `LatencySamples` during activation;
- `InstanceRuntime` snapshots it for the active lifetime;
- deactivation removes it and reactivation may compute another value;
- CLAP exposes the snapshot through `PluginLatency` and calls the host latency-change hook when activation changes it.

The deliberate delayed probe reports 64 samples at 48 kHz and its stereo impulse emerges exactly at sample 64.

The recorded REAPER baseline below establishes sample-exact PDC alignment for its delayed-probe artifact. Remaining current/support claims require:

- refreshing that real-host observation/alignment after relevant artifact changes;
- production product restart/reactivation behavior when configuration changes required latency.

Additional-format parity is a gate for a selected adapter, not a prerequisite for the initial core; see [cross-format qualification](#cross-format-differential-gate).

## State/adversarial tests

Treat host state as untrusted bytes. Keep coverage for malformed/truncated/oversized input, invalid types/numerics, wrong product/schema, unknown parameters, migration chains, resource exhaustion, and failed loads leaving live state unchanged.

`InstanceRuntime` complete-state export/load derives identity/version from its `ComponentSchema`; construction and loads use component-owned defaults/validation/migrations. Schema-only construction and per-load policy overrides are removed. The resource-backed state fixture processes while custom resources are stale, proving that frozen DSP parameters/withheld trajectories cannot overwrite saveable desired state. Lower parameter-store serialization helpers do not constitute a second component identity authority.

Retain golden fixtures for every released schema. The current CHSS v1 envelope remains provisional pending its explicit compatibility policy and the [state-format promotion gates](state-format.md#v1-promotion-gates-still-open) for selected supported boundaries. An unselected second format is not an initial-core prerequisite.

## Host matrix

| Host / harness | Platform | Status | Evidence |
| --- | --- | --- | --- |
| Clack in-process host harness | Linux/macOS/Windows CI | passed at `88c884b`; same revision macOS local | native notes/expression, MIDI 1/SysEx and opaque MIDI 2/UMP, mixed input/shared output order, per-message port capabilities, voice feedback, payload ownership and backpressure, zero-audio dispatch, accepted/rejected callback allocation, f32/f64 audio, complete state/resource transitions, lifecycle, latency/tail/bypass, re-entrancy, panic containment |
| `clap-validator` 0.4.1 + reviewed generator corrections | Linux/macOS/Windows CI | passed at `88c884b`; same revision macOS local | conformance 35 pass / 0 fail / 9 skip; delayed probe 21 / 0 / 23; note probe 37 / 0 / 7; MIDI probe 34 / 0 / 10; all four bounded fuzz runs; no generated UMP/SysEx coverage |
| REAPER 7.79 | macOS arm64 (M3 Max, 48 kHz) | 2026-09-06 baseline | 2026-09-06 headless `-renderproject` qualification: scan + instantiate (FX browser, 5 params enumerated), square `trim` automation render sample-exact against the deterministic reference (value changes land at exact sample offsets 24000/36000), injected-CHSS state load render sample-exact (trim 0.25/0.75; blobs byte-identical to a REAPER-saved blob at 0.5 and validated by the real `StateDocument` decoder), project save while the transport rolled through automation with coherent base state (trim=0.5, not torn) and sample-exact automation replay after reload, no-FX baseline render bit-identical to the source fixture, and PDC alignment: a wet track carrying the exported 64-sample delayed probe (`examples/clap-delayed-probe`) mixes against a dry track as exactly `2*x[i]` for all 48,000 frames — REAPER honored the CLAP latency extension and aligned the delayed DSP sample-exact. Host-quirk recorded: `TrackFX_SetParamNormalized` on an inactive CLAP reports success but delivers no change; the differential shows FabFilter Pro-Q 4 (CLAP) behaving identically while ReaEQ (VST) applies immediately, so this is REAPER's inactive-CLAP delivery behavior, not an adapter defect — the automation path (process events) is sample-exact. |
| Bitwig | — | not run | production-host re-entrancy confirmation remains useful when available |

When a production DAW is available, refresh current-head scan/save-reopen/automation/PDC evidence rather than treating the headless harnesses as equivalent.

## Cross-format differential gate

When another adapter is selected for supported deployment, render the same deterministic product state + input + automation/events through the supported formats and compare audio, state, identities, port configuration, automation timing, latency/tail, and editor-visible parameter semantics where implemented. This gate qualifies those adapters; it does not require adding VST3/AU or an editor to stabilize the initial core. Use bit-exact comparison for framework-only deterministic behavior when possible and explicit tolerances only where semantics require them.

## Release evidence

A release advertised for commercial plugin use must record:

- Rust/tool versions;
- supported OS/architectures/formats;
- native validator results;
- tested DAW matrix;
- identity/state fixtures;
- realtime/performance evidence;
- known limitations.

Evidence, not confidence or source availability, earns production status.
