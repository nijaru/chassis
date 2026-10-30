# Chassis

## Navigation and checks

- `docs/architecture.md` owns scope and architectural constraints; `docs/design/` owns individual contracts. Read the affected contract before changing it and update its owner when the decision changes.
- `docs/roadmap.md` owns deferred framework work; `docs/next-runtime-slice.md` owns the immediate implementation sequence; `docs/design/validation.md` owns qualification evidence. Do not duplicate their status here.
- `docs/dependencies.md` and `docs/licensing.md` govern dependency and distribution changes.
- Workspace packages: `chassis-runtime` (format-independent semantics and lifecycle), `chassis-clap` (CLAP deployment), and exported qualification fixtures under `examples/`.
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

## Contract changes

- For schema ownership, activation, latency, parameters, events, or state changes, read the affected sections of `docs/architecture.md` and the matching contract in `docs/design/`. Update those owners and migrate callers together; do not preserve a superseded type shape because it appeared in agent guidance.
- For callback, resource-reclamation, unsafe, or FFI changes, read `docs/architecture.md#realtime-and-failure-boundaries` and `docs/design/runtime.md`. Verify realtime bounds and failure behavior separately from ordinary Rust correctness; `Arc` release and panic containment are not evidence of safe continued processing.

## Evidence and dependencies

- Separate Rust correctness, realtime bounds, format conformance, performance, and real-host support. Historical validator/REAPER/benchmark results do not qualify later revisions automatically.
- Keep the exact audited Clack revision unless the change includes source review and adapter conformance. The audit owner is `docs/research/clack-pinned-adapter-audit.md`; registry availability alone does not establish equivalent behavior.
- Before adding dependencies or accepting outside code, check `docs/dependencies.md` and `docs/licensing.md` for both distribution models and contribution rights; do not infer approval from a crate's availability.
