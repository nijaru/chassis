# Runtime / CLAP Execution Plan

This is the current implementation order on `main`. It is an execution plan, not a stable API promise.

## Validated checkpoint — `ad57f50`

The local Rust qualification gate passed through `ad57f50`:

```text
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-features --all-targets --locked -- -D warnings
cargo build --release -p chassis-clap-conformance --locked
```

The earlier full gate through `62b96cc` also included `cargo deny check` and `cargo machete`; no dependencies changed in the CLAP-I/O slice after that checkpoint. `cargo deny` emitted only the existing unmatched-license-allowance warnings.

The validated runtime now includes:

- durable single-owner `InstanceRuntime<P>` lifecycle and owned accepted active I/O;
- stable persistent parameter keys plus schema-local dense `ParameterIndex` identities for realtime work;
- CLAP parameter bindings that retain dense indices and never re-resolve stable keys in the hot path;
- adapter-local scalar publication qualified under Loom, including the fixed lossless `pending` handoff;
- terminal publication generation exhaustion without ABA wraparound;
- bounded decode of canonical CHSS semantic state;
- adjacent typed product-schema migrations;
- adversarial truncation/resource-bound/corruption coverage and a checked-in canonical v1 fixture;
- complete transactional parameter replacement;
- complete semantic instance state ownership: framework parameters and validated custom entries are accepted or rejected together;
- product validation over the complete parameter/custom candidate before publication;
- activation visibility of the accepted complete semantic state;
- failure-atomic migrated byte loads and deterministic complete-state export;
- explicit stable CLAP audio-port IDs rather than declaration-order identity;
- setup-time audio mapping validation for declared ports/layouts, direction-local IDs, main-port placement, and reciprocal in-place pairing;
- qualified stereo main I/O with zero or one stereo sidechain through that mapping;
- input-only sidechain channels delivered to product DSP without callback allocation.

The CLAP publication bridge remains adapter-local. Passing Loom demonstrates the current scalar protocol; it does not make that representation a deployment-independent core primitive.

## Slice 1 — runtime ownership and dense identity

Completed and qualified.

`InstanceRuntime<P>` is the durable format-independent owner. `Processor` exclusively owns mutable realtime DSP history while active. `ParameterKey` remains persistent/authoring identity while `ParameterIndex` is the dense schema-local realtime identity.

The older activation-local `Activated<'a, P>` path remains only as a migration convenience and must not become a second persistent authority.

## Slice 2 — publication protocol qualification

Completed and qualified.

Evidence covers:

1. exact-generation writer acquisition;
2. coherent multi-value snapshots;
3. stale realtime publication rejection;
4. bounded realtime failure while another writer is active;
5. terminal generation exhaustion;
6. weak-memory Acquire/Release/AcqRel behavior under Loom;
7. the `pending` Release → AcqRel handoff without lost notification.

Loom exposed a real race in the old `pending.swap(false, AcqRel)` consume: a consumer that did not observe a concurrent `true` could still overwrite it. Production and model now consume with `compare_exchange(true, false, AcqRel, Acquire)`, so an unsuccessful observation is read-only.

Typed non-scalar publication and reclamation remain separate problems and do not justify generalizing this CLAP-specific scalar helper into `chassis-core`.

## Slice 3 — complete state + migrations

Completed and qualified through `62b96cc`.

The accepted complete-state path is:

```text
bytes
 -> bounded decode
 -> adjacent product migration chain
 -> materialize complete current parameter/custom candidate
 -> validate framework parameter domain
 -> validate product parameter/custom invariants
 -> publish parameters + custom state together
```

`InstanceRuntime` retains canonical non-`parameter/` `StateEntry` values beside `ParameterStore`. It does not persist arbitrary Rust product structs. `Component::activate_with_state` can inspect the accepted complete semantic state during preparation while defaulting back to the earlier parameter-only activation hook for compatibility.

Qualified state evidence includes:

- unique adjacent migration chains and typed product migration failures;
- legacy v1 migration fixture;
- checked-in binary v1 fixture that decodes and re-encodes byte-identically;
- deterministic semantic value encoding;
- every proper truncation prefix rejected;
- configured total/product-ID/entry/key/payload bounds;
- extreme declared lengths rejected without backing payloads;
- bounded single-byte corruption corpus with no parser panic;
- repeated failed runtime loads leave live state unchanged;
- parameter + custom state commit atomically;
- product rejection leaves both parameter and custom state unchanged;
- migrated byte loads publish both together;
- complete state is visible to later activation and export.

CHSS envelope v1 is still **not frozen**. Remaining promotion evidence belongs with real adapters rather than more speculative core API:

- sustained fuzz corpus/infrastructure beyond the deterministic adversarial corpus;
- active save/load race qualification through the deployment consistency contract;
- CLAP/VST3/AU cross-format round trips and partial-stream boundary tests;
- golden fixtures for every actually released product schema.

## Slice 4 — general CLAP I/O

The mapped stereo/sidechain step is completed and qualified through `ad57f50`.

Qualified setup/process behavior now provides:

1. explicit stable CLAP audio-port IDs supplied by product/deployment metadata;
2. direction-local dense CLAP indices with main ports placed at index 0;
3. setup-time validation of arbitrary declared Chassis ports/layouts;
4. unique CLAP IDs within each direction;
5. reciprocal opposite-direction equal-width in-place pairing;
6. runtime activation from the mapped `AudioIoConfiguration` rather than a hard-coded default;
7. stereo main input/output processing with zero or one stereo input-only sidechain;
8. unsupported process topologies rejected during activation instead of reaching partial DSP code.

The remaining blocker is representation rather than metadata. The current product-facing `ProcessBlock` still owns a borrowed flat `&mut [ChannelBuffer<'_, S>]`. An arbitrary runtime number of lifetime-bearing channel views cannot be retained in processor storage, and allocating a `Vec<ChannelBuffer>` in each callback is forbidden. Do not solve that by lifetime erasure or hidden callback allocation.

### Buffer-source sub-slice

A follow-on after `ad57f50` introduces the proposed no-allocation source shape in `chassis-core`:

- `ProcessChannel<S>` abstracts only the already-proven `ChannelBuffer` operations;
- `ProcessBufferSource<S>` uses generic associated channel/iterator types so an adapter can yield safe channel views lazily;
- `ChannelBufferSlice` proves the existing materialized slice can act as a compatibility source;
- a core test source chains two independent slices, proving one traversal does not require a single flat backing collection;
- `validate_frame_count()` remains an explicit pre-DSP source responsibility.

This source abstraction is implemented but **not yet qualified** and is not yet wired into `ProcessBlock`/`InstanceRuntime`. It exists to get the Rust lifetime/GAT shape through the local gate before replacing the established process path.

After that source shape passes:

1. make `ProcessBlock` generic over `ProcessBufferSource<S>` while retaining `ChannelBufferSlice` for existing callers;
2. make `Process<S>::process` generic over the concrete buffer source so adapters remain allocation-free without type erasure;
3. add `InstanceRuntime::process_buffers` and keep the current slice entry point as a compatibility wrapper;
4. implement a CLAP source that traverses `PortPairsIter`/`PairedChannelsIter` lazily with setup-retained semantic endpoints;
5. validate all host buffer dimensions/sample representation before product DSP;
6. remove the stereo-specific callback materialization branches;
7. add asymmetric/multi-bus/high-channel-count conformance cases.

Required invariant: no callback-time owned `Vec<ChannelBuffer>` allocation.

## Slice 5 — native CLAP qualification

Once current Rust semantics and general I/O are coherent, rebuild/package the conformance `.clap` and qualify the **current** artifact:

- `clap-validator` normal suite and bounded fuzzing;
- audio-port enumeration, stable IDs, main/sidechain metadata, and in-place pairing;
- parameter enumeration/get/value conversion;
- parameter automation through process and flush paths;
- state save/load round trip while inactive;
- active state save while automation is running;
- state load followed by processing;
- repeated deactivate/reactivate preserving host-visible state;
- lifecycle/repeated-instance stress;
- REAPER render, automation, sidechain, save/load and reopen smoke tests;
- Bitwig when available because it exercises relevant CLAP/reentrancy behavior.

Do not treat an older validator artifact as evidence for current runtime/parameter/state/I/O code.

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

Use a real effect before growing generic conveniences much further. It should exercise many parameters, complete semantic state, general I/O where useful, latency/offline behavior, explicit smoothing policy, and meter/telemetry publication without sharing mutable processor state.

Only repeated product needs graduate into framework helpers.

## After native CLAP

Proceed to VST3/AU projection and editor integration after the same conformance product has qualified CLAP state, automation, lifecycle, and general I/O. Cross-format differential tests then become the compatibility gate.