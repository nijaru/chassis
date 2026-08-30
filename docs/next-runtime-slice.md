# Runtime / CLAP Execution Plan

This is the current implementation order on `main`. It is an execution plan, not a stable API promise.

## Validated checkpoint — `73d7012`

The runtime, state, dense-parameter, and dense-projection work through `73d7012` passed the full local Rust gate:

- CLAP scalar publication serializes writers with a generation CAS token;
- realtime automation endpoint publication is rejected when its observed generation is stale;
- non-realtime state save obtains one completed coherent scalar generation;
- CLAP scalar state load requires a complete mapped parameter snapshot;
- CHSS decoding rejects empty entry keys and preserves `StateDocument` invariants;
- `InstanceRuntime<P>` owns durable core `ParameterStore` state across activation cycles;
- the component definition remains outside `InstanceRuntime`, so deployment transfer bounds apply to `Processor`, not `Component`;
- activation can observe current validated base parameters through `Component::activate_with_parameters`;
- active runtime owns a copy of negotiated `ConfiguredAudioPort` values allocated outside the callback;
- core runtime parameter state replacement is complete and transactional at the `InstanceRuntime` boundary;
- the CLAP audio processor uses `InstanceRuntime` and synchronizes published host state into the runtime before processor activation;
- `ParameterIndex` is the schema-local dense realtime identity while `ParameterKey` remains persistent/authoring identity;
- normalized CLAP parameter events map host IDs to dense indices before process validation;
- process event validation and trajectory cursors use dense indices rather than stable string lookup;
- `ParameterStore::set_index()` validates and replaces base values directly by schema-local `ParameterIndex`;
- each CLAP parameter binding retains its declaration-order `ParameterIndex` at setup;
- CLAP ID normalization and scalar projection synchronization reuse that stored dense index instead of re-resolving stable keys.

Validation passed:

```text
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-features --all-targets --locked -- -D warnings
cargo deny check
cargo machete
cargo build --release -p chassis-clap-conformance --locked
```

`cargo deny` emitted only the existing unmatched-license-allowance warnings.

The CLAP cross-domain parameter publication remains adapter-local. CLAP's audio-processor object exists only while active, so durable cross-activation host-visible state cannot simply live inside the audio-thread `InstanceRuntime`. Do not hide that host lifetime difference by moving the component or a mutex-protected processor into shared state.

Dense IDs deliberately do not add a second per-parameter event index yet. The bounded globally sorted event scan remains until measurements justify another structure.

## Completed follow-on — publication protocol qualification

The existing scalar publication algorithm is now isolated as a private helper under the CLAP parameter module. This is a testability boundary, not framework infrastructure: it still stores CLAP-compatible scalar `f64` values and is not exported from the adapter.

The qualified source after `73d7012` additionally:

- removes generation-counter wraparound as a correctness assumption;
- reserves `u64::MAX - 1` as the final stable generation and refuses further writes once reached, so an ancient generation can never become current again through ABA wraparound;
- lets coherent state save continue at the terminal generation while later state replacement fails explicitly rather than spinning or wrapping;
- keeps realtime writer acquisition one-shot/nonblocking;
- keeps realtime snapshot work bounded to a fixed retry count;
- keeps control/state publication allowed to wait for a short in-flight writer;
- exhaustively enumerates sequentially-consistent same-generation writer interleavings and proves only one writer acquires;
- exhaustively enumerates the reader/writer steps of a two-value snapshot and rejects every accepted mixed snapshot;
- tests concrete stale-generation rejection, bounded realtime failure while a writer owns the token, invalid value counts, and terminal generation exhaustion.

This follow-on is qualified through `3f18404`: the full local Rust gate passed, all five Loom tests passed, and the pending/synchronization handoff race is covered. The model is evidence for the current adapter-local scalar implementation, not a reason to extract it into `chassis-core`.

## Slice 2 — publication protocol qualification

Completed. The qualified evidence covers:

1. the standard local Rust gate;
2. the acquire/release protocol under Loom;
3. writer acquisition, coherent multi-value snapshots, stale-generation rejection, and the pending/synchronization handoff;
4. bounded realtime operations while a writer is active;
5. terminal generation exhaustion without ABA wraparound;
6. typed-value and reclamation work remain adapter-local until another client proves the need;
7. typed choice/string semantics remaining outside the CLAP scalar helper.

Loom 0.7.2 is the current upstream release as of this checkpoint. Adding it should be a deliberate dev/test dependency with the resulting `Cargo.lock` update reviewed and committed; do not hand-edit the lockfile.

A passing model does **not** imply that this helper belongs in `chassis-core`. Keep it adapter-local unless another deployment or real client demonstrates the same publication contract independently. A future deployment-independent primitive needs a semantic typed-value contract and a reclamation story, not merely a generalized version of CLAP's first scalar bridge.

The desired authority model remains semantic rather than requiring one physical object shared by every host thread. A deployment may use synchronized projections when its lifecycle requires them, but there must still be one defined publication order and no independently mutable semantic copies.

## Slice 3 — complete state + migrations

`InstanceRuntime::apply_parameter_state_for_product` provides complete transactional parameter replacement, while the lower-level `ParameterStore::apply_state_for_product` remains a patch-like semantic helper. `StateMigration`/`StateDocument::migrate_to` now provide the adjacent product-schema chain, and `InstanceRuntime::apply_parameter_state_bytes` performs bounded decode, migration, and complete application in that order.

Next state work:

```text
bytes
 -> bounded decode
 -> product migration chain
 -> materialize complete current semantic state
 -> validate all parameter/custom domains
 -> publish one generation
```

Acceptance criteria:

- retain adjacent-version migration fixtures for every released schema;
- failed migration or validation leaves current state unchanged;
- fuzz corruption/truncation/exhaustion and migration failures;
- extend the runtime state owner so parameter + custom product fields publish as one accepted generation;
- keep deterministic golden fixtures for every released schema;
- make partial parameter patches a separately named operation if a real client needs them.

Do not freeze CHSS v1 until migration and cross-format fixtures exist.

## Slice 4 — general CLAP I/O

The lifetime/ownership prerequisite is in place: active runtime owns its negotiated configuration. Use that to broaden the adapter instead of adding callback-time owned vectors.

Order:

1. default stereo main + optional stereo sidechain through the general mapping;
2. arbitrary declared input/output ports and layouts;
3. setup-time dense endpoint mapping;
4. negative-space tests for disabled/missing/asymmetric/unsupported layouts;
5. decide from adapter evidence whether the flat `ChannelBuffer` slice remains the right product-facing borrow shape.

Required invariant: no `Vec<ChannelBuffer>` allocation in the process callback.

## Slice 5 — native CLAP qualification

Once the current Rust checkpoint is green and the runtime/parameter/I/O changes are coherent, rebuild/package the conformance `.clap` and qualify the *current* artifact:

- `clap-validator` normal suite and bounded fuzzing;
- parameter enumeration/get/value conversion;
- parameter automation through process and flush paths;
- state save/load round trip while inactive;
- active state save while automation is running;
- state load followed by processing;
- repeated deactivate/reactivate preserving host-visible state;
- lifecycle/repeated-instance stress;
- REAPER render, automation, save/load and reopen smoke tests;
- Bitwig when available because it exercises relevant CLAP/reentrancy behavior.

Do not treat the older 19-pass validator artifact as evidence for the current parameter/state/runtime slice.

## Slice 6 — CLAP capability expansion

After current native semantics qualify:

1. f64 advertisement/dispatch;
2. render/offline mode;
3. choice parameter projection;
4. modulation and product-originated gesture output;
5. note/MIDI/event ports;
6. latency/tail/status metadata as required by real clients.

Each capability needs conformance plus native-host evidence rather than source support alone.

## Slice 7 — first real FX client

Use a real effect before growing generic conveniences much further. It should exercise many parameters, state, latency/offline behavior, explicit smoothing policy, and meter/telemetry publication without sharing mutable processor state.

Only repeated product needs graduate into framework helpers.

## After native CLAP

Proceed to VST3/AU projection and editor integration after the same conformance product has qualified CLAP state, automation, lifecycle, and general I/O. Cross-format differential tests then become the compatibility gate.
