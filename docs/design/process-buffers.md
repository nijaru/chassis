# Processing and Audio Buffer Model

Status: uniform safe channel views, scoped allocation-free channel iterators, cross-channel borrowing, and mapped CLAP f32/f64 traversal are implemented. The author-facing API remains pre-alpha.

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

This operation explicitly initializes output; it is not an idempotent mutable accessor. Initialize once for an in-place algorithm, then use `output_mut()` for subsequent stages. Out-of-place algorithms match `ChannelBuffer::Separate { input_samples, output_samples, .. }` to consume both disjoint slices simultaneously. In-place algorithms match `InPlace { samples, .. }` and use the single mutable slice. Separate `input()` and `output_mut()` accessor calls cannot express simultaneous borrowing; enum destructuring is the authoring boundary. Zero-copy is not a goal by itself; keep the bounded copy unless measurement justifies more complex ownership.

## Object-safe no-allocation source

Runtime-negotiated port/channel counts cannot safely be retained as lifetime-bearing `ChannelBuffer` values in processor storage, while constructing an owned vector every callback violates the realtime contract.

The implemented boundary is:

```text
ProcessBufferSource<S>
  with_channels(&mut dyn FnMut(&mut dyn Iterator<Item = ChannelBuffer<'_, S>>))

ChannelBuffer<'_, S>
  uniform enum with semantic endpoints and safe sample slices
```

A materialized slice uses `ChannelBufferSlice` and `ChannelBuffer::reborrow()`; a format adapter constructs the same enum lazily. The source creates its iterator on the stack and invokes the visitor exactly once, including for an empty topology. There is no second channel trait or generic source type in the product process signature.

Multiple views may coexist inside that traversal: stereo-linked or multichannel DSP can take successive iterator items and process their samples together without collecting them or copying input to scratch. Sample references cannot escape the closure. The complete alias proof must license all yielded views simultaneously, not just each view in isolation. Laziness changes when the safe view is produced, not the proof required for it. Consumers stop requesting iterator items on the first error.

Every callback covers each active input/output endpoint exactly once, in any order, with paired or single-direction views. Missing/silent channels require explicit sample views; omission is not silence. Sources are finite and repeat the same topology, relationships, and dimensions across traversals. Runtime validation uses activation-owned endpoint coverage scratch, rejecting missing, duplicate, or endpoint-free views before DSP. Since each accepted view consumes a unique endpoint, even a repeated source is rejected within the activation's endpoint count plus one yielded view.

`InstanceRuntime::process_source()` is the authoritative runtime entry for arbitrary sources. `InstanceRuntime::process()` is the non-allocating materialized-slice convenience wrapper.

## ProcessBlock

`ProcessBlock<'a, S>` exposes actual frame count, process mode/transport context, borrowed canonical base parameters, bounded ordered parameter and note events, and one borrowed buffer source. Activation configuration is used for validation, not retained as a public block accessor.

`Process<S>::process(&mut self, block: &mut ProcessBlock<'_, S>)` has no source generic or independent source/context/parameter lifetimes. `block.with_channels(|channels| ...)` lends the iterator for cross-channel DSP and returns the closure's result. `block.for_each_channel(|channel| ...)` visits independent channels; `block.try_for_each_channel(|channel| -> Result<(), E> { ... })` stops on the first error. These helpers allocate nothing and do not roll back earlier writes.

Before audio traversal, core rejects frame/event-bound or block-association failures. One bounded source traversal validates complete audio topology and sample dimensions together. Parameter type/domain and event-port capabilities are checked before product DSP runs. Block construction only projects these accepted inputs; it does not repeat validation or traverse the source again. Stable key/backend mapping belongs to setup and is not rediscovered per callback.

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

`tests/realtime_alloc.rs` mechanically checks a sample-generic processor using simultaneous separate input/output slices at f32 and f64, including allocation-free dimension/topology rejection. Early-stop and repeated traversal are covered by core source tests. Full format qualification and representative performance evidence remain separate; source inspection and Rust tests alone are not production qualification.

## Remaining design work

Cross-channel algorithms already use `with_channels`; they do not depend on future bus helpers. Higher-level port discovery or sidechain views may simplify repeated endpoint selection, but must not add hidden per-block maps or allocation.

Future interleaved/incompatible backend layouts require activation-owned bounded conversion scratch plus measured cost. Higher-channel-count/surround/ambisonics layouts remain product-driven roadmap work.
