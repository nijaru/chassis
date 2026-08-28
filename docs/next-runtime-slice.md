# Next Runtime Slice

This is the implementation order after the scalar CLAP publication hardening on `fix/runtime-state-authority`. It is an execution plan, not a stable API promise.

## Gate 0 — validate the hardening branch

Before merging or extending the branch:

```text
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-features --all-targets -- -D warnings
cargo deny check
cargo machete
cargo build --release -p chassis-clap-conformance
```

Then rebuild/package the `.clap` artifact and rerun native qualification with the newly exported params/state extensions:

- `clap-validator` normal suite and bounded fuzzing;
- parameter enumeration/get/value conversion;
- parameter automation through process and flush paths;
- state save/load round trip while inactive;
- active state save while automation is running;
- state load racing a later process block;
- repeated deactivate/reactivate preserving host-visible parameter state;
- REAPER render/automation/state smoke tests;
- Bitwig when available because it exercises relevant CLAP/reentrancy behavior.

Do not claim the new scalar state/parameter slice is qualified until that artifact has actually passed these checks.

## Slice 1 — durable `InstanceRuntime` authority

Move persistent instance semantics out of `Activated`.

Required ownership:

```text
Component definition
        |
        v
InstanceRuntime
  canonical ParameterStore
  persistent state generation
  inactive accepted I/O configuration
  lifecycle generation
        |
        +---- control/state APIs
        |
        v
Active runtime
  Processor
  owned activation config/resources
  derived realtime base projection
```

Acceptance criteria:

- canonical parameter values survive deactivate/reactivate without an adapter becoming a second semantic authority;
- state loaded before activation is visible to processor activation/preparation;
- `Activated`/its successor cannot be mistaken for the durable state owner;
- a full state load publishes one complete accepted generation;
- failed decode/migration/validation leaves the canonical generation unchanged;
- stale realtime/asynchronous completion cannot overwrite a newer control/state generation;
- audio-thread synchronization remains bounded and nonblocking;
- large replacement/reclamation work cannot migrate onto the audio thread accidentally.

Keep the current CLAP atomic bridge adapter-local while this ownership shape is implemented. If a generic atomic publication primitive is proposed for `chassis-core`, model its concurrent writer/reader behavior before promotion.

## Slice 2 — dense runtime parameter identity

Resolve stable keys once during schema/runtime setup:

```text
ParameterKey (persistent) -> ParameterIndex (runtime dense)
```

Use `ParameterIndex` in normalized process events, schema validation, and trajectory cursors.

Acceptance criteria:

- stable keys remain the only persistence/authoring identity;
- runtime indices are deterministic only within one validated schema/runtime instance and are never serialized;
- adapters resolve backend IDs -> runtime indices outside the hot path;
- process validation no longer binary-searches string keys for every event;
- per-parameter trajectory evaluation does not repeatedly scan unrelated events when a bounded indexed representation can avoid it;
- no callback-time allocation is introduced.

Benchmark the old and new event/cursor paths before adding more elaborate indexing structures.

## Slice 3 — complete state and migrations in core

Replace the old patch-like `ParameterStore::apply_state_for_product` prototype as part of the runtime authority transition.

Required semantics:

```text
bytes
 -> bounded decode
 -> product migration chain
 -> materialize complete current state
 -> validate all domains
 -> publish one generation
```

Acceptance criteria:

- missing current parameters/required fields fail unless a migration explicitly supplies them;
- custom state and parameter state publish together as one semantic generation when product custom fields land;
- one fixture exists for an older schema migration;
- corrupted/oversized/unknown/wrong-product state never partially mutates the instance;
- deterministic golden fixtures cover the eventual v1 wire format before it freezes.

Partial parameter patches, if needed, are a separate API and never implicit plugin-state behavior.

## Slice 4 — owned negotiated I/O + sidechain/multibus

Before general CLAP I/O support, remove the lifetime trap where a dynamic active configuration would have to borrow adapter-owned storage for the full active lifetime.

Acceptance criteria:

- instance/active runtime owns accepted dynamic I/O configuration allocated while inactive;
- product activation receives a borrowed view of runtime-owned configuration;
- stable ports resolve to setup-time dense endpoints;
- default stereo + optional stereo sidechain works through the general mechanism;
- arbitrary multibus mapping does not allocate a `Vec<ChannelBuffer>` in every callback;
- exact alias/disjoint/input-only/output-only safety remains explicit;
- negative-space tests cover missing/disabled/asymmetric/unsupported layouts.

Use this adapter evidence to decide whether the current flat `ChannelBuffer` slice remains the right core borrowing shape.

## Slice 5 — CLAP capability expansion

After the runtime/state/I/O contracts above are validated:

1. f64 advertisement/dispatch;
2. render/offline mode;
3. choice parameter projection;
4. modulation and product-originated gesture output;
5. note/MIDI/event ports;
6. latency/tail/status metadata as required by real clients.

Each capability adds conformance and real-host evidence rather than relying on source-level support alone.

## Slice 6 — first real FX client

Once native CLAP semantics are stable enough, use a real effect to decide what becomes framework convenience rather than extending the conformance component speculatively.

A first client should exercise:

- many parameter updates and persistent state;
- latency/offline behavior;
- custom editor lifecycle later;
- meters/telemetry without making the processor shared;
- parameter smoothing as explicit product policy.

Only repeated product needs graduate into Chassis helpers.

## After native CLAP

Proceed to VST3/AU projection and editor integration only after the same conformance product has qualified native CLAP state, automation, lifecycle, and general I/O semantics. Cross-format differential tests then become the compatibility gate.
