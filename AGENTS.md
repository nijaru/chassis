# AGENTS.md

## Project purpose

Chassis is a convention-first Rust framework for professional realtime audio components. Effects are the first production clients; instruments, standalone deployment, immersive layouts, and optional host/application layers are planned without making the core depend on any one of them.

Read `docs/architecture.md`, `docs/roadmap.md`, `docs/licensing.md`, and `docs/dependencies.md` before architectural or dependency changes. Relevant contracts live under `docs/design/`; update the owning document when a decision changes rather than allowing implementation and documentation to diverge.

## Rust baseline

- Inspect `rust-toolchain.toml`, workspace/package manifests, targets, features, and current validation commands before changing Rust requirements.
- Development follows the named `stable` toolchain and Edition 2024. Do not pin a compiler or declare/raise an MSRV without an explicit compatibility or reproducibility reason.
- Prefer strong domain newtypes/enums over raw strings or integers once data crosses into the semantic core.
- Borrow at API boundaries when ownership is not needed; do not clone merely to satisfy the borrow checker.
- Prefer typed library errors. Add error/dependency crates only when they materially simplify the design.
- Treat every `pub` item and `pub use` as deliberate API design. These pre-alpha crates are unpublished; use that freedom to change weak abstractions before release.
- Avoid recoverable `.unwrap()` in library code. `expect` is for a proved programmer invariant and should state the reason.

## Design rules

- Design from common audio-component requirements, not by cloning or reacting to another framework's scope.
- Prefer convention over configuration for behavior most products want; preserve explicit escape hatches.
- Do not overfit APIs to any Nijaru plugin.
- Do not assume stereo in the core data model. Stereo main I/O plus optional stereo sidechain is only the default effect convention.
- Do not assume audio input exists; instruments/generators and event-only processors must fit.
- Do not assume a plugin host exists; standalone and embedded deployment must remain possible.
- Keep DAW/application product semantics outside core: timelines, arrangements, projects, media libraries, mixer UX, mastering revisions/QC, delivery workflows, etc.
- Keep CLAP/VST3/AU/Clack/clap-wrapper/toolkit/platform types out of product-facing core APIs.
- Keep stable product/parameter/port identities independent from Rust names, UI labels, runtime dense indices, and backend IDs.
- Treat adapter translation as semantic mapping: map faithfully or reject; never silently change meaning to satisfy a host.
- Extract optional utilities when their semantics are broadly reusable or proven by repeated clients, not merely because they are common words in audio software.

## Ownership and lifecycle

For every mutable guarantee/resource, name one authority and its lifecycle: creation, mutation, observation, replacement/cancellation, and teardown.

- Framework instance runtime owns canonical parameter/control state and lifecycle coordination.
- `Processor` owns mutable realtime DSP state while active.
- `MainThread` owns only optional product non-realtime state/orchestration.
- `Shared` exposes explicitly synchronized projections/immutable snapshots; it is not a second authority.
- Editor/background work never receives unrestricted mutable access to `Processor`.
- Replacement/snapshot/task APIs must prevent stale work from publishing into a new generation and must define where old resources are destroyed.
- Do not add process-global mutable state or a process-global worker runtime without an explicit unload/shutdown/lifetime design. Use `OnceLock` only when one-time process-global lifetime is actually the contract.
- `chassis-core::Processor` is not globally `Send`; deployment adapters add thread-transfer bounds only where their host/runtime contract requires them. CLAP currently requires the concrete processor to be `Send` because Clack models the host as free to move it between audio threads.

## Realtime rules

These strict rules apply to the audio callback and other explicitly deterministic hot paths. Do not impose them indiscriminately on control/editor/tooling code.

- No heap allocation after activation.
- No filesystem/network I/O or blocking calls.
- No contended/unbounded locks.
- Queue, scratch, retry, and work bounds must follow from activation/product requirements; do not invent arbitrary caps merely to claim boundedness.
- Bounded queues require explicit overflow/drop/coalescing policy.
- Keep mutable DSP state audio-thread-owned unless a specific shared design is justified.
- Large replaced objects must not be accidentally destroyed on the audio thread; define reclamation ownership.
- Controlled copies are acceptable when semantics require them and measurement does not justify more complexity. Zero-copy is not a goal by itself.
- Do not add cache padding/alignment, lock-free algorithms, custom allocators, `no_std`, SIMD, or unsafe code without a concrete requirement and evidence.
- Audit every dependency used on the realtime path for allocation, locking, unsafe invariants, maintenance quality, and overhead.
- Do not generalize the first fixed-stereo CLAP proof by allocating `Vec<ChannelBuffer>` per callback. Let real adapter evidence drive the general multibus borrowing model.

## Validation and failure

- Validate host/FFI input, persisted bytes, IDs, lengths, and configuration where they become authoritative.
- Expected invalid input/corrupt state returns or records a typed failure and must not partially publish state.
- Assert non-obvious internal invariants after validation when violating them would imply a framework bug.
- Never unwind across FFI.
- State/product/parameter/port IDs are compatibility contracts once released.
- Preserve sample-accurate ordering/offset semantics where the source format provides them.
- Test negative space: malformed state, exhaustion, lifecycle replacement/cancellation, unusual blocks, state load/save, automation, editor teardown, and unsupported layouts.
- Maintain a conformance component and differential cross-format tests; do not rely on product DSP or one successful host load as framework proof.
- Separate correctness evidence from performance and production-readiness claims.
- `examples/clap-conformance` is the first exported adapter probe. Do not treat it as product support until local Rust validation, CLAP-native validation, and host testing pass.

## Unsafe/FFI rules

- Format-independent crates deny unsafe code.
- Adapter crates may use unsafe only where the FFI/host boundary requires it.
- Every unsafe block requires a `// SAFETY:` comment describing the discharged invariants.
- Edition 2024 `extern` blocks must use `unsafe extern` where required.
- `unsafe_op_in_unsafe_fn` remains denied; unsafe functions state preconditions and explicit unsafe blocks discharge them.
- Run Miri on Rust portions before promoting unsafe adapter changes; use sanitizers/host stress where Miri cannot model native boundaries.
- Subtle atomic/memory-ordering primitives require model/property testing (for example Loom) before becoming shared framework infrastructure.
- The first `chassis-clap` slice intentionally contains no Chassis-owned unsafe code; Clack constructs the safe `ChannelPair` views. Keep that property until a concrete missing capability justifies owning additional unsafe surface.

## Licensing and dependencies

- Chassis is AGPL-3.0-or-later with an intended commercial dual-license path.
- Prefer dependencies that can legally ship in proprietary commercial-license builds (MIT/Apache/BSD/ISC/public-domain or similarly permissive terms).
- `deny.toml` records the dependency-license/source policy. Until automated runners are intentionally restored, run it locally; do not describe GitHub Actions as an enforcement gate.
- Do not widen license policy merely to make a dependency check pass; review and document new license families first.
- Do not import third-party strong-copyleft code into the commercially relicensable framework without an explicit decision.
- Keep SDK-specific constraints, especially AAX/Avid/PACE, isolated from format-independent crates.
- Do not accept substantive external code contributions until contributor/relicensing terms are established.
- The first adopted external runtime dependencies are exact published Clack 0.1.1 crates; upgrades are deliberate compatibility/audit events, not automatic version drift. Do not switch to unreleased 0.2 git sources merely because upstream `main` has bumped its development version.

## Validation commands

The repository currently has no authoritative hosted CI. Before claiming a code slice is validated, run the applicable local commands on a supported development machine and report exactly what was run:

```text
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-features --all-targets -- -D warnings
cargo deny check
cargo machete
cargo miri test      # before promoting unsafe Rust where Miri applies
```

`cargo machete` now applies because `chassis-clap` has third-party dependencies. Format-native validators, host tests, fuzzing, sanitizers, and benchmarks become additional gates as the relevant adapters exist.

## Current implementation priority

1. Locally validate the newly added `chassis-clap` + `examples/clap-conformance` slice, regenerate/review `Cargo.lock`, and fix fmt/test/clippy/deny/machete findings without weakening the design.
2. Build/package the conformance export as an actual `.clap`, then run CLAP-native validator/lifecycle/buffer stress and a real-host smoke test.
3. Use that evidence to decide the general multibus/sidechain buffer-access shape; do not force arbitrary channel layouts through the current fixed `[ChannelBuffer; 2]` proof.
4. Add CLAP render/offline semantics and f64 only when their format mapping/advertisement are explicit.
5. Then implement parameters/automation/state/events through the same conformance path.
6. Add VST3/AU projection and editor integration only after native CLAP semantics are proven.
7. Use real FX clients, then standalone/instruments, to graduate broadly reusable conveniences.

Do not create empty crates or roadmap abstractions merely to make the repository look complete.
