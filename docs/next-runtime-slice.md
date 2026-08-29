# Runtime / CLAP Execution Plan

This is the current implementation order on `main`. It is an execution plan, not a stable API promise.

## Validated checkpoint — `76eb6b3`

The following architecture work passed the full local Rust gate at `76eb6b3`:

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
- process event validation and trajectory cursors use dense indices rather than stable string lookup.

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

The CLAP cross-domain atomic parameter publication remains adapter-local. CLAP's audio-processor object exists only while active, so durable cross-activation host-visible state cannot simply live inside the audio-thread `InstanceRuntime`. Do not hide that host lifetime difference by moving the component or a mutex-protected processor into shared state.

Dense IDs deliberately do not add a second per-parameter event index yet. The bounded globally sorted event scan remains until measurements justify another structure.

## Current follow-on — dense projection cleanup

The next small cleanup is now implemented after `76eb6b3`:

- `ParameterStore::set_index()` validates and replaces base values directly by schema-local `ParameterIndex`;
- out-of-range dense writes return an explicit `UnknownParameterIndex` error;
- each CLAP parameter binding retains its declaration-order `ParameterIndex` at setup;
- CLAP ID normalization reuses the stored binding index instead of converting the binding position per event;
- CLAP scalar projection synchronization writes by dense index instead of binary-searching stable parameter keys.

This is intentionally a completion of the dense-identity cut, not a new abstraction layer. Stable keys remain authoritative for persistence/authoring/state, and `ParameterIndex` remains meaningful only relative to one immutable schema.

Re-run the same full local Rust gate before treating this follow-on as validated. Hosted GitHub Actions is configured for Rust 1.98, but recent runs have failed before any runner step starts and therefore provide no compiler/test evidence.

## Slice 2 — publication/generation generalization

The current CLAP scalar bridge is implementation evidence, not yet generic framework infrastructure.

Before extracting a shared core primitive:

- model concurrent writer acquisition, coherent snapshots, stale-generation rejection, and ordering with Loom or equivalent;
- define wraparound assumptions or remove dependence on them;
- prove realtime paths never spin/wait on control writers;
- define reclamation before supporting values that cannot fit directly in atomics;
- preserve typed choice/string semantics rather than forcing every parameter through `f64` merely because CLAP scalar values do;
- decide which state belongs to a deployment-independent core publication primitive and which remains format translation.

The desired authority model is semantic, not necessarily one physical object shared by every host thread. A deployment may use synchronized projections when its lifecycle requires them, but there must still be one defined publication order and no independently mutable semantic copies.

Do not promote the current CAS bridge into `chassis-core` merely because the CLAP scalar case works. Model the protocol first.

## Slice 3 — complete state + migrations

`InstanceRuntime::apply_parameter_state_for_product` provides complete transactional parameter replacement, while the lower-level `ParameterStore::apply_state_for_product` remains a patch-like semantic helper.

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

- add an adjacent-version migration fixture;
- failed migration/validation leaves current state unchanged;
- parameter + future custom product fields publish as one generation;
- keep deterministic golden fixtures for every released schema;
- fuzz corruption/truncation/exhaustion;
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