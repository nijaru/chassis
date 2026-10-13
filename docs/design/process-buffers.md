# Processing and Audio Buffer Model

Status: safe channel relationships, generic allocation-free `ProcessBufferSource<S>`, and mapped CLAP f32/f64 traversal are implemented. Author-facing higher-level bus views remain pre-alpha.

## Safe relationships

`ChannelBuffer<'a, S>` represents already-proven host relationships:

```text
InPlace
  input endpoint + output endpoint + one &mut [S]

Separate
  input endpoint + &[S] + output endpoint + disjoint &mut [S]

InputOnly
  input endpoint + &[S]

OutputOnly
  output endpoint + &mut [S]
```

Endpoints carry schema-local dense `AudioPortIndex` plus channel index. Stable keys remain setup/persistence identity; dense Chassis indices and backend bus/channel indices are separate derived projections.

`InPlace` stores one mutable reference only. `Separate` is legal only after the adapter has proved input/output disjoint for the borrow lifetime. Product DSP never receives raw host pointers.

## Controlled copy convenience

`ChannelBuffer::copy_input_to_output()` provides one explicit convenience:

- exact in-place -> existing mutable slice, no copy;
- separate -> bounded input-to-output copy on every call, then output slice;
- input-only/output-only -> error because no valid paired in-place view exists.

This operation explicitly initializes output; it is not an idempotent mutable accessor. Initialize once for an in-place algorithm, then use `output_mut()` for subsequent stages. Products with out-of-place algorithms can use input/output access directly. Zero-copy is not a goal by itself; keep the bounded copy unless measurement justifies more complex ownership.

## Generic no-allocation source

Runtime-negotiated port/channel counts cannot safely be retained as lifetime-bearing `ChannelBuffer` values in processor storage, while constructing an owned vector every callback violates the realtime contract.

The implemented boundary is:

```text
ProcessBufferSource<S>
  validate_frame_count(expected)
  channels() -> allocation-free iterator

ProcessChannel<S>
  relationship
  semantic endpoints
  frame count
  input / output_mut / copy_input_to_output
```

A materialized slice uses `ChannelBufferSlice`; a format adapter can yield safe host-derived channel wrappers lazily. Laziness changes when the safe view is produced, not the alias proof required for it.

Every callback covers each active input/output endpoint exactly once, in any order, with paired or single-direction views. Missing/silent channels require explicit sample views; omission is not silence. Sources are finite and repeat the same topology, relationships, and dimensions across traversals. Runtime validation uses activation-owned endpoint coverage scratch, rejecting missing, duplicate, or endpoint-free views before DSP. Since each accepted view consumes a unique endpoint, even a repeated source is rejected within the activation's endpoint count plus one yielded view.

`InstanceRuntime::process_source()` is the authoritative runtime entry for arbitrary sources. `InstanceRuntime::process()` is the non-allocating materialized-slice convenience wrapper.

## ProcessBlock

`ProcessBlock<S, B>` exposes actual frame count, process mode/transport context, borrowed canonical base parameters, bounded ordered parameter and note events, and one borrowed buffer source. Activation configuration is used for validation, not retained as a public block accessor.

Before audio traversal, core rejects frame/event-bound or block-association failures. Before product DSP runs, it also validates complete audio topology, source dimensions, parameter type/domain, and note-port capabilities. Stable key/backend mapping belongs to setup and is not rediscovered per callback.

Float automation trajectories are evaluated lazily through cursors rather than expanded into per-sample event storage.

## CLAP mapping

`ClapAudioConfiguration` validates explicit stable `ClapAudioPort` metadata and builds setup-owned `ClapProcessSlot` mappings. `raw_audio` reads Clack-borrowed descriptors and raw pointer tables, validates the complete mapped sample alias topology, then creates channel references lazily for `ClapBufferSource`. It bypasses pinned Clack's reference-first sample pairing; the `ChannelPair` enum is only the resulting safe representation.

The host must supply live descriptor/table/sample allocations, sufficient initialized sample extent, metadata separate from sample memory, and exclusive writable access for the callback. Chassis checks nullness, alignment, address arithmetic, representation, and aliases, not arbitrary pointer provenance or actual allocation length.

The adapter handles:

- reciprocal semantic input/output pairs as one Chassis relationship;
- unrelated input/output ports sharing a dense CLAP index as independent single-direction channels;
- asymmetric input/output tails;
- mapped auxiliary/sidechain ports;
- f32 or f64 callbacks selected consistently across the mapped configuration.

All overlap involving writable samples is rejected before any sample reference exists, except an exact declared reciprocal input/output pair at the same channel, represented by one mutable slice. Shared input/input overlap is allowed. Partial pair, cross-channel/cross-port, and duplicate writable overlap are rejected. Empty views use aligned dangling pointers.

Scratch storage is O(C) and whole alias validation is O(C²), bounded by activation-mapped channels rather than callback-supplied counts. This is a correctness-first boundary, not a claim that large topologies have constant overhead. Output constant masks are conservatively cleared because the current processor contract makes no constant-output promise.

Mapped port count, channel count, sample representation, relationship shape, and callback frame count are checked before product traversal.

## Sample precision

Buffer/source types are generic over `S`; processor lifetime is not. One `Processor` may implement both `Process<f32>` and `Process<f64>`.

The CLAP adapter advertises f64 only through the explicit f64-capable export marker and requires the processor to implement both precisions. Mixed sample representations in one callback are rejected.

## Realtime rules

`Process<S>` and every `ProcessBufferSource` implementation must obey the deterministic callback contract:

- no heap allocation/deallocation after activation;
- no blocking I/O or contended/unbounded locks;
- work bounded by validated frame/channel/event configuration;
- no callback-owned dynamic channel collections;
- no stable-string/schema searches when setup can retain dense mappings;
- replacement/reclamation of large objects stays off the callback.

Mechanical allocation/work-bound instrumentation is still required before production claims; source inspection alone is not proof.

## Remaining design work

Higher-level bus/port helpers should come from real DSP clients. Useful candidates may include conventional stereo-main access, optional sidechain, and efficient explicit port lookup, but they must not add hidden per-block maps or allocation.

Future interleaved/incompatible backend layouts require activation-owned bounded conversion scratch plus measured cost. Higher-channel-count/surround/ambisonics layouts remain product-driven roadmap work.
