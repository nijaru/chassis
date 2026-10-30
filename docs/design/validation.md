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

The zero-audio MIDI processor (`examples/clap-midi-probe`) validates at **34 passed, 0 failed, 10 skipped**. Its actual-entry tests supply SysEx coverage absent from the validator's generated event set.

These gates do not establish production DAW behavior or representative performance.

Hosted workflows and local results are separate evidence. Before publication, hosted jobs were rejected and local checks supplied the interim record. Consult the actual workflow run for the revision under review before claiming Linux/macOS/Windows coverage. The dated validator, host, and benchmark results below are baselines, not automatic qualification of later commits.

### MIDI 1/SysEx and shared output — 2026-10-30

At code revision `b833b02989fb0d3317e119775f9e89b4bb1e8f97` on macOS 26.6.2 arm64:

- 254 workspace tests passed on stable Rust 1.99.1 and CI-baseline Rust 1.98.0. Formatting, strict all-feature/all-target Clippy on both compilers, warning-denied rustdoc, release workspace build, shell syntax, local Markdown paths, and diff checks passed.
- Local `cargo-deny` 0.19.0 passed with unused-license-allowance warnings; `cargo-machete` 0.9.1 found no unused dependencies. Hosted policy tooling used the configured newer pins.
- Seven actual exported MIDI ABI tests covered opaque fixed bytes and SysEx boundaries, independent port maps, both defined flags, equal-time controls, cross-block completion, malformed/over-budget rejection before DSP, and fatal first/later output backpressure. The host deep-copied payloads during push and retained their contents after source mutation/destruction. Maximum accepted/rejected callbacks allocated and deallocated nothing, including 1,024 MIDI records, 64 KiB SysEx, 512 controls, and 512 ignored records in one accepted 2,048-record batch.
- Default-Stacked-Borrows Miri `0.1.0 (16d1ffac17 2026-10-26)` passed six focused actual ABI cases, two private MIDI projection cases, and the zero-audio regression. The maximum-count stress case ran natively on both compilers, not under Miri. The focused over-budget case retained Miri protection for scalar rejection before even a nonnull invalid payload loan.
- The zero-audio regression failed the original sample-precision discovery. Empty topology now preserves the actual callback frame count without requiring dummy sample storage; unexpected audio descriptors still reject. An independent source review found no actionable core/adapter correctness findings; this is not a whole-Clack safety qualification.
- All four packaged artifacts passed the reviewed pinned validator and separate five-second, two-worker fuzz runs: conformance **35 passed / 0 failed / 9 skipped**, delayed probe **21 / 0 / 23**, note probe **37 / 0 / 7**, MIDI probe **34 / 0 / 10**. SysEx lifetime/ownership evidence comes from the actual ABI/Miri cases, not the validator's generator.

Hosted [Rust CI](https://github.com/nijaru/chassis/actions/runs/41015851376) and [CLAP Conformance](https://github.com/nijaru/chassis/actions/runs/41015851298) completed successfully at this exact code revision. Every one of seven Rust jobs and all three Linux/macOS/Windows CLAP matrix jobs was checked individually. The latter packaged, validated, and fuzzed all four exports.

The [MIDI boundary audit](../research/clack-pinned-adapter-audit.md#midi-1sysex-boundary--reviewed-2026-10-30) records the same-pin MIDI layout correction, scoped payload loans, C provider obligations, and unknown-flag rejection. MIDI 2/UMP remains unsupported. No production DAW or representative performance run was repeated; this record does not renew those older claims.

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

Native-note qualification exposed a second generator defect: tuning was sampled from `-128..=128` instead of CLAP's `-120..=120`, and volume admitted zero despite `0 < x <= 4`. The [native-note range patch](../../vendor/clap-validator/native-note-ranges.patch) corrects only these bounds and adds a fixed-seed 20,000-event domain regression. It fails the original code on tuning `127.85912075812644` and passes the correction. Strict plugin validation remains unchanged; no validator test is excluded or weakened. Both patches retain upstream MIT terms and the original license notice; the tool does not ship in plugin artifacts.

[The installer](../../.github/scripts/install-clap-validator.sh) fetches the exact workflow pin, requires both patches to apply, runs all binary unit regressions on Rust 1.98.0, and installs into `target/clap-validator/bin` without replacing the user's CLI. All three platform jobs call that same executable for validation and fuzzing. Re-audit and remove each patch when a pinned upstream replacement corrects its generator; historical results above/below must be read with their recorded source delta.

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

Tests under `crates/chassis-clap/tests/` exercise the actual exported Chassis CLAP entry through pinned Clack host APIs rather than a synthetic adapter-only seam.

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
- `host_custom_state.rs`: resource-backed complete state through default save, migration/rejection, deferred restart/reset/reversion, active flush, save/reopen, zero-allocation pending callbacks, and a load racing old DSP completion whose accepted modulation survives rejected base completion;
- `host_reentrancy.rs`: plugin -> host -> plugin main-thread re-entry through parameter-rescan and latency-change callbacks;
- `host_panic_containment.rs`: activation/process panic containment at the Clack FFI boundary.
- `exported_notes.rs`: native port metadata, f32/f64 mixed input order, onset expressions, exact raw wildcard admission, zero-velocity on, original input-port voice feedback including cross-block debt, host output rejection, and callback allocation/deallocation.

The full adapter allocation test covers 1,000 f32 and 1,000 f64 callbacks for each of all-base and mixed base/modulation streams, with stereo main + stereo sidechain and exactly the configured 64 parameter controls, plus actual-entry exact-in-place acceptance and partial-alias rejection, with zero measured allocation/deallocation or host-log calls on the callback thread.

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
- `crates/chassis-runtime/tests/realtime_alloc.rs`: repeated typed automation completion with callback-thread allocation/deallocation counting, including choice replacement after control edits and decoded state loading;
- `crates/chassis-runtime/tests/latency.rs`: activation-scoped latency snapshot/stability/removal/recomputation.

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

A manual adapter-overhead harness exists at `crates/chassis-clap/benches/adapter_overhead.rs` and runs with:

```text
cargo bench -p chassis-clap --bench adapter_overhead
```

It exercises 32/64/128/512-frame blocks, f32/f64, zero/max event load, and stereo main + sidechain. Run it on stable representative hardware and record CPU/architecture, OS, Rust version, release profile, sample rate, topology, and event load. Shared CI VM timing is not production performance evidence.

Representative measurement (first recording, 2026-09-03, Mac15,9 / Apple M3 Max / arm64, macOS 26.6.2, rustc 1.98.0 (88d9e12ae 2026-08-18), cargo bench release profile, 48 kHz, stereo main + stereo sidechain, trivial no-op DSP):

- no-event load: median 49–53 ns/callback across every block size and precision, ~19–20 M callbacks/s;
- configured 64-event load: median 477–499 ns/callback, ~2.0 M callbacks/s;
- overhead is flat in frame count and f32/f64; event normalization dominates at ~7 ns/event;
- two consecutive runs agreed within ~4% on medians; p99 stayed within ~6% of median except single-cell OS noise.

Interpretation: adapter-only overhead is well under a single sample period at 48 kHz (20.8 µs); there is no material adapter cost to optimize before real-client work. This is a distribution measurement on one machine, not a universal constant.

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
- production product restart/reactivation behavior when configuration changes required latency;
- cross-format latency parity once VST3/AU exist.

## State/adversarial tests

Treat host state as untrusted bytes. Keep coverage for malformed/truncated/oversized input, invalid types/numerics, wrong product/schema, unknown parameters, migration chains, resource exhaustion, and failed loads leaving live state unchanged.

`InstanceRuntime` complete-state export/load derives identity/version from its `ComponentSchema`; construction and loads use component-owned defaults/validation/migrations. Schema-only construction and per-load policy overrides are removed. The resource-backed state fixture processes while custom resources are stale, proving that frozen DSP parameters/withheld trajectories cannot overwrite saveable desired state. Lower parameter-store serialization helpers do not constitute a second component identity authority.

Retain golden fixtures for every released schema. The current CHSS v1 envelope remains provisional until its compatibility policy and deployment/cross-format state gates are complete.

## Host matrix

| Host / harness | Platform | Status | Evidence |
| --- | --- | --- | --- |
| Clack in-process host harness | Linux/macOS/Windows CI | passed at `b833b02`; same revision macOS local | native notes/expression and MIDI 1/SysEx, mixed input/shared output order, voice feedback, payload ownership and backpressure, zero-audio dispatch, accepted/rejected callback allocation, f32/f64 audio, complete state/resource transitions, lifecycle, latency/tail/bypass, re-entrancy, panic containment |
| `clap-validator` 0.4.1 + reviewed generator corrections | Linux/macOS/Windows CI | passed at `b833b02`; same revision macOS local | conformance 35 pass / 0 fail / 9 skip; delayed probe 21 / 0 / 23; note probe 37 / 0 / 7; MIDI probe 34 / 0 / 10; all four bounded fuzz runs |
| REAPER 7.79 | macOS arm64 (M3 Max, 48 kHz) | 2026-09-06 baseline | 2026-09-06 headless `-renderproject` qualification: scan + instantiate (FX browser, 5 params enumerated), square `trim` automation render sample-exact against the deterministic reference (value changes land at exact sample offsets 24000/36000), injected-CHSS state load render sample-exact (trim 0.25/0.75; blobs byte-identical to a REAPER-saved blob at 0.5 and validated by the real `StateDocument` decoder), project save while the transport rolled through automation with coherent base state (trim=0.5, not torn) and sample-exact automation replay after reload, no-FX baseline render bit-identical to the source fixture, and PDC alignment: a wet track carrying the exported 64-sample delayed probe (`examples/clap-delayed-probe`) mixes against a dry track as exactly `2*x[i]` for all 48,000 frames — REAPER honored the CLAP latency extension and aligned the delayed DSP sample-exact. Host-quirk recorded: `TrackFX_SetParamNormalized` on an inactive CLAP reports success but delivers no change; the differential shows FabFilter Pro-Q 4 (CLAP) behaving identically while ReaEQ (VST) applies immediately, so this is REAPER's inactive-CLAP delivery behavior, not an adapter defect — the automation path (process events) is sample-exact. |
| Bitwig | — | not run | production-host re-entrancy confirmation remains useful when available |

When a production DAW is available, refresh current-head scan/save-reopen/automation/PDC evidence rather than treating the headless harnesses as equivalent.

## Cross-format differential gate

Once VST3/AU projections exist, render the same deterministic product state + input + automation/events through each format and compare audio, state, identities, port configuration, automation timing, latency/tail, and editor-visible parameter semantics. Use bit-exact comparison for framework-only deterministic behavior when possible and explicit tolerances only where semantics require them.

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
