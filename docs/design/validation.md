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

1. build the release conformance and delayed-probe artifacts;
2. package the platform-native `.clap` forms;
3. build `clap-validator` from pinned revision `b2f1d9b79b1d264a5747f46707d72b1aa40a02ef`;
4. run the validator on both artifacts;
5. run a bounded five-second two-worker fuzz pass on both artifacts.

The recorded validator baseline is **35 passed, 0 failed, 9 intentional skips** for the conformance artifact. The matrix is configured in CI; successful revision-scoped runs and local checks below establish actual coverage. The exported delayed probe (`examples/clap-delayed-probe`) validates at **21 passed, 0 failed, 23 intentional skips** — the skips are parameter/state cases the no-parameter latency probe does not declare — and passes the same bounded fuzz after flushing subnormal delay-line output to zero (initially caught by the fuzzer as subnormal pass-through).

These gates do not establish production DAW behavior or representative performance.

Hosted workflows and local results are separate evidence. Before publication, hosted jobs were rejected and local checks supplied the interim record. Consult the actual workflow run for the revision under review before claiming Linux/macOS/Windows coverage. The dated validator, host, and benchmark results below are baselines, not automatic qualification of later commits.

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

Custom-state deployment remains unsupported. The core transition intentionally discards parameter automation until reactivation; it does not qualify arbitrary product DSP or a future complete-state bridge. No production DAW, representative performance benchmark, or new Miri run was performed; raw reference construction and CHSS bytes are unchanged.

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

- `host_conformance.rs`: f32/f64 callback dispatch, stereo main + stereo sidechain, configured 64-event bound, accepted/rejected-callback allocation/deallocation counting and absence of realtime host logging, stepped conversion through display/flush/process, output constant masks, legal raw in-place audio, illegal partial-alias rejection, reactivation, latency callbacks, audio-thread transfer, repeated inactive instances;
- `exported_probe_reset.rs`: actual exported delay fixture reset clears history/positions without allocation and retains the activation delay;
- `host_frame_bounds.rs`: activation frame extremes and over-bound rejection;
- `host_activation_failure.rs`: product activation failure and same-instance retry, explicit descriptor feature preservation, and rejection of schemas whose event ports CLAP cannot yet project;
- `host_latency_audio.rs`: nonzero reported latency matching actual delayed audio;
- `host_restart.rs`: one processor restart request per activation, immediate successful-state-load restart notification before processing, rejected state without restart, unchanged active latency, and reactivation using published state;
- `host_active_state_save.rs`: coherent CLAP state save while processing is active;
- `host_state_policy.rs`: component-owned scalar migrations/validation, atomic load rejection, schema drift, and custom-default rejection across factory/main-thread definitions;
- `host_reentrancy.rs`: plugin -> host -> plugin main-thread re-entry through parameter-rescan and latency-change callbacks;
- `host_panic_containment.rs`: activation/process panic containment at the Clack FFI boundary.

The full adapter allocation test covers 1,000 f32 and 1,000 f64 callbacks with stereo main + stereo sidechain and exactly the configured 64 parameter events, plus actual-entry exact-in-place acceptance and partial-alias rejection, with zero measured allocation/deallocation or host-log calls on the callback thread.

## Core conformance

The format-independent proof is split by concern while exercising the same public runtime model:

- `crates/chassis-runtime/tests/conformance.rs`: schema rejection, lifecycle, buffer relationships, process bounds/context, base parameters, sample-accurate automation, invalid events, generic non-flat buffer-source traversal, and rejection of structurally valid but unsupported audio configurations;
- `crates/chassis-runtime/tests/automation_runtime.rs`: final typed endpoints retained after successful DSP, equal-offset last-value order, event-free carryover, block-local ramp continuation, canonical encoded state, subsequent control precedence, reactivation, and invalid-batch failure atomicity;
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

## Active state-save contract

Implemented CLAP contract:

- audio processing never waits for state save;
- save obtains one coherent completed scalar generation;
- a save racing a process block may linearize immediately before or after final automation-endpoint publication;
- save never emits a mixed cross-parameter generation;
- the publication token describes the whole store synchronized before DSP, not a generation reacquired afterward;
- newer control/state publication prevents stale realtime completion from overwriting it;
- failed publication invalidates the local generation cache, including at terminal exhaustion; coherent end sync restores the authoritative store or retries next block after contention;
- state load is transactional and requests host value rescan after publication.

Evidence exists at three levels:

1. direct publication unit tests;
2. Loom atomic models;
3. in-process CLAP host test that pauses an active callback before endpoint publication, saves the old complete generation, releases processing, then saves both new endpoints together.

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
| Clack in-process host harness | Linux/macOS/Windows CI | passed at `7620284`; same revision macOS local | accepted/rejected process allocation and host logging, f32/f64, component state policy, lifecycle, active state save, latency, re-entrancy, panic containment |
| `clap-validator` 0.4.1 | Linux/macOS/Windows CI | passed at `7620284`; same revision macOS local | conformance 35 pass / 0 fail / 9 skip; delayed probe 21 / 0 / 23; both bounded fuzz runs |
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
