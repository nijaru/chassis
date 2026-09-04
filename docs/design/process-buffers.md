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

Endpoints carry stable semantic port identity plus channel index. Backend dense bus/channel indices are adapter projections, not product identity.

`InPlace` stores one mutable reference only. `Separate` is legal only after the adapter has proved input/output disjoint for the borrow lifetime. Product DSP never receives raw host pointers.

## Controlled copy convenience

`ChannelBuffer::make_in_place()` provides one explicit convenience:

- exact in-place -> existing mutable slice, no copy;
- separate -> bounded input-to-output copy, then output slice;
- input-only/output-only -> error because no valid paired in-place view exists.

Products with out-of-place algorithms can use input/output access directly. Zero-copy is not a goal by itself; keep the bounded copy unless measurement justifies more complex ownership.

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
  input / output_mut / make_in_place
```

A materialized slice uses `ChannelBufferSlice`; a format adapter can yield safe host-derived channel wrappers lazily. Laziness changes when the safe view is produced, not the alias proof required for it.

`InstanceRuntime::process_source()` is the authoritative runtime entry for arbitrary sources. `InstanceRuntime::process()` is the non-allocating materialized-slice convenience wrapper.

## ProcessBlock

`ProcessBlock<S, B>` borrows:

- actual frame count;
- active process configuration;
- process mode and transport snapshot;
- canonical base `ParameterStore`;
- bounded sample-sorted `ParameterEvents`;
- one `ProcessBufferSource<S>`.

Before product DSP runs, core validates frame bounds, event block association/type/domain, and source dimensions. Stable endpoint/backend mapping belongs to setup and is not rediscovered per callback.

Float automation trajectories are evaluated lazily through cursors rather than expanded into per-sample event storage.

## CLAP mapping

`ClapAudioConfiguration` validates explicit stable `ClapAudioPort` metadata and builds setup-owned `ClapProcessSlot` mappings. `ClapBufferSource` then traverses Clack's safe `ChannelPair` relationships directly.

The adapter handles:

- reciprocal semantic input/output pairs as one Chassis relationship;
- unrelated input/output ports sharing a dense CLAP index as independent single-direction channels;
- asymmetric input/output tails;
- mapped auxiliary/sidechain ports;
- f32 or f64 callbacks selected consistently across the mapped configuration.

An exact alias supplied for unrelated semantic ports is rejected before DSP because the same mutable region cannot safely represent two independent outputs/owners.

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
