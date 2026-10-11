# Chassis

## Navigation and checks

- `docs/architecture.md` owns scope and architectural constraints; `docs/design/` owns individual contracts. Read the affected contract before changing it and update its owner when the decision changes.
- `docs/roadmap.md` owns deferred framework work; `docs/next-runtime-slice.md` owns the immediate implementation sequence; `docs/design/validation.md` owns qualification evidence. Do not duplicate their status here.
- `docs/dependencies.md` and `docs/licensing.md` govern dependency and distribution changes.
- Workspace packages: `chassis-runtime` (format-independent semantics and lifecycle), `chassis-clap` (CLAP deployment), and two exported qualification fixtures under `examples/`.
- Development tracks `stable` in `rust-toolchain.toml`, Edition 2024. CI's explicit compiler is a validation baseline, not a declared MSRV.

```sh
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-features --all-targets --locked -- -D warnings
cargo deny check
cargo machete
```

CLAP packaging, pinned validator, and bounded fuzz commands live in `.github/workflows/clap-conformance.yml`. Do not substitute a Rust build for format qualification.

## Scope and API

- Build reusable audio infrastructure, not product DSP or DAW document/workflow models. Plugins are a deployment path, not the definition of a component.
- Existing product ports are evidence sources, not roadmap authorities. Do not duplicate or silently migrate separate audio-product repositories.
- These packages are unpublished pre-alpha. Replace weak interfaces and migrate all callers; there is no compatibility-shim requirement. Existing tests and prior plans do not make an abstraction mandatory.
- Add package boundaries for actual dependency, safety, platform, optionality, or release needs. Do not preallocate a catalog of future crates or introduce a facade solely to hide crate names.
- Keep plugin-format, toolkit, device-backend, and OS types out of `chassis-runtime` public APIs. Domain identifiers must remain independent of labels, Rust names, declaration order, dense indices, and backend IDs.

## Authority and lifecycle

- `Component` supplies immutable schema and factories. `ComponentSchema` owns parameter/audio/event metadata, whole-I/O policy, and semantic state identity.
- `InstanceRuntime<P>` owns durable parameter/custom state and lifecycle coordination. `Processor` owns active DSP history/resources. Deployment and shared handles are projections/orchestration, not independent semantic authorities.
- Stable keys are setup/persistence identity; dense indices belong to the validated schema and are never serialized. Runtime-owned metadata stays off the callback path.
- Accept audio I/O as one whole configuration before activation. An adapter must map semantics faithfully or reject unsupported use, including unsupported event ports or custom state.
- Active latency is snapshotted for one activation. Resource/latency changes require restart/reactivation; processing remains valid with the old resources until that transition.
- Keep durable parameter values, automation trajectories, modulation, and effective DSP values distinct. Do not smooth explicit host ramps automatically.
- Decode, migrate, and validate state before transactional publication. Active save must capture one completed generation; stale automation completion must lose to newer control/state publication.

## Realtime and FFI

- After activation, deterministic callbacks allocate/deallocate nothing, perform no blocking I/O, and take no contended/unbounded locks. Bound scratch, queues, events, retries, and work from the accepted activation/deployment requirements.
- Prepare replacements off-thread and reclaim replaced large objects off the callback. `Arc` release is not automatically realtime-safe.
- Controlled bounded copies are acceptable. Require a concrete need and evidence for lock-free containers, unsafe fast paths, SIMD, custom allocators, cache padding, or `no_std`.
- `chassis-runtime` forbids unsafe code. Keep necessary FFI unsafe code at adapters with explicit invariants and matching checks.
- Clack contains panics at CLAP FFI callbacks. Activation failure permits reuse; process failure terminates that processing run. Do not promise continued DSP after an arbitrary panic.

## Evidence and dependencies

- Separate Rust correctness, realtime bounds, format conformance, performance, and real-host support. Historical validator/REAPER/benchmark results do not qualify later revisions automatically.
- Keep the exact audited Clack revision unless the change includes source review and adapter conformance. The audit owner is `docs/research/clack-pinned-adapter-audit.md`; registry availability alone does not establish equivalent behavior.
- Chassis is AGPL-3.0-or-later with an intended commercial dual-license path. New dependencies must permit both distribution models. Contribution/relicensing terms are not yet established for substantive outside code.
