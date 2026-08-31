# Processing and Audio Buffer Model

Status: safe channel relationships, the no-allocation generic buffer-source boundary, and the mapped f32 CLAP source are implemented. The generic core/source path and expanded CLAP mapping are locally Rust-1.98-qualified through `dfc137c`. Higher-level author-facing port views and public API remain pre-alpha.

## Goal

Give ordinary effects a simple processing path while preserving host buffer semantics and Rust aliasing guarantees. Avoid gratuitous copies, but do not make zero-copy a goal in itself.

The core buffer model must support effects, generators/instruments, auxiliary inputs/outputs, multiple buses, variable block sizes, offline rendering, and eventual high-channel-count processing.

## Safe channel relationships

Target formats can provide exact in-place input/output buffers or distinct buffers. Chassis therefore cannot assume every process call is in-place or every input/output pair is separate.

`chassis-core::buffer::ChannelBuffer<'a, S>` represents four already-proven safe relationships:

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

Endpoints use stable `PortKey` plus a zero-based channel index. They associate a safe sample view with semantic product ports without exposing backend bus indices.

`InPlace` deliberately stores **one mutable reference only**. Chassis never constructs both `&[S]` and `&mut [S]` for the same host range. `Separate` may exist only after the input/output ranges have already been proved disjoint for the borrow lifetime.

Input-only and output-only views preserve generators, analyzers, auxiliary routing, and multi-output products without forcing a fake one-to-one effect topology.

The adapter owns host-pointer and host-layout validation. Product DSP never receives raw host pointers.

## Construction and frame bounds

`ChannelBuffer` constructors accept the current callback frame count, validate that backing storage is long enough, and expose exactly that prefix. They never allocate.

`ProcessConfig` also carries the activation-owned maximum number of normalized parameter events accepted per callback. Adapters choose that bound from their accepted host/product contract; zero is valid for a deployment with no parameter-event projection.

A short synthetic/host-derived safe view is rejected before product DSP receives it.

## Conventional in-place helper

`ChannelBuffer::make_in_place()` is the explicit convenience path for ordinary in-place DSP:

- exact in-place: returns the existing `&mut [S]` with no copy;
- separate: performs one bounded `copy_from_slice` from input to output and returns output;
- input-only: fails because no output exists;
- output-only: fails because no input exists to copy.

This copy is controlled product/framework behavior, not hidden adapter normalization. Products that benefit from out-of-place processing can use `input()` and `output_mut()` directly and skip it.

## No-allocation buffer sources

Runtime-negotiated port/channel counts cannot be represented safely by retaining lifetime-bearing `ChannelBuffer` values in processor storage. Building a `Vec<ChannelBuffer>` every callback would violate the realtime contract, while erasing the lifetimes would weaken the aliasing model.

The implemented source boundary is iterator-based:

```text
ProcessBufferSource<S>
  validate_frame_count(expected)
  channels() -> allocation-free iterator

iterator item: ProcessChannel<S>
  relationship
  input/output endpoint
  frame_count
  input / output_mut
  make_in_place
```

`ProcessChannel<S>` abstracts only operations already supplied by `ChannelBuffer`; it is not a second buffer semantic model. `ChannelBuffer` and mutable `ChannelBuffer` borrows implement it.

`ProcessBufferSource<S>` uses generic associated types for its channel item and iterator. A materialized source can therefore yield borrowed channel views while a format adapter can yield safe host-derived channel wrappers lazily, without trait-object lifetime erasure or callback-owned collections.

`ChannelBufferSlice` is the compatibility source for existing `&mut [ChannelBuffer]` callers. A core conformance source also chains two independent channel slices, proving that one legal process traversal does not require a contiguous materialized collection.

The source shape itself was qualified on Rust 1.98 at `e4b34e4`. It was then wired through `ProcessBlock`, `Process<S>`, `Activated`, and `InstanceRuntime::process_source`, with the slice entry points retained as compatibility wrappers; that full core boundary was qualified at `3349910`.

## ProcessBlock

The current product-facing call is:

```text
ProcessBlock<'source, 'context, 'parameters, S, B>
where B: ProcessBufferSource<S>
```

It contains:

- actual callback frame count;
- per-call `ProcessContext` and `ProcessMode`;
- block-start transport snapshot;
- validated borrowed parameter events;
- immutable active base/control `ParameterStore` view;
- one borrowed buffer source from which safe channel views can be traversed allocation-free.

Its constructor is framework-private. Before product DSP is entered, the runtime validates:

- positive activation minimum when one was guaranteed;
- activated maximum frame count;
- the event context belongs to this callback frame count;
- event count/type/domain constraints relative to the active parameter schema;
- the source's callback-varying frame dimensions.

`ParameterEvents` remains caller-storage-backed and sample-sorted. `FloatParameterCursor` evaluates set/linear trajectories lazily and never expands a ramp into per-sample event storage.

Stable semantic endpoint mapping is deliberately **not** rediscovered by `ProcessBlock`. Format adapters resolve stable keys, dense backend indices, and legal pairing at setup/activation and retain that plan for callback traversal.

## CLAP mapped source

The current CLAP adapter proves the generic source against a real runtime-negotiated format.

`ClapAudioConfiguration` validates declared Chassis ports against explicit stable `ClapAudioPort` metadata and builds setup-owned `ClapProcessSlot` values. Dense CLAP input/output indices are process-layout details, not product identity.

A process slot may contain an input, an output, or both. Two ports are marked `paired` only when their declared `in_place_pair` relationships are reciprocal and their layouts are compatible. Setup ordering aligns real semantic pairs when CLAP's dense arrays can represent them together.

This distinction matters because CLAP pairs its input/output arrays by dense index. Two unrelated ports may therefore appear in one Clack `PortPair` even though they are not one semantic Chassis channel relationship.

`ClapBufferSource` handles that case without allocation:

- a declared reciprocal pair may expose Clack `InputOutput` or exact `InPlace` as one Chassis channel relationship;
- unrelated `InputOutput` buffers at one dense index are split into independent input-only and output-only Chassis channels;
- asymmetric input/output tails remain single-direction channels;
- an `InPlace` alias for two unrelated ports is rejected before product DSP because one shared mutable region cannot safely represent two independent semantic ports.

The source wraps Clack's already-safe `ChannelPair` values directly instead of materializing callback `ChannelBuffer` arrays. It prevalidates mapped port count, channel count, sample representation, and relationship shape before entering product DSP.

The mapped f32 source, arbitrary auxiliary mappings, and split-independent iterator state passed Rust 1.98 formatting, full workspace tests, strict all-feature/all-target Clippy, and the release conformance build at `dfc137c`.

This is source/compiler qualification. The expanded topology still needs a repeated native `clap-validator`/real-host qualification run; the older REAPER/validator evidence applies to the earlier conventional stereo artifact.

## Rust aliasing contract

A format adapter may expose `InPlace` only for a valid exact alias where a single mutable slice is the sole live Rust reference to that memory.

It may expose `Separate` only after proving the input/output ranges disjoint for the full borrow lifetime.

Unexpected partial overlap, overlapping output channels, inconsistent lengths, invalid/null pointers, or host state that cannot satisfy Rust reference rules must be contained at the adapter boundary. Never manufacture overlapping references merely because a host specification says the host should behave.

If a legal backend relationship is awkward to express with ordinary references, raw pointers/private unsafe machinery must stay inside the adapter behind a smaller safe accessor. The product API is not weakened to raw pointers for adapter convenience.

The generic source does not relax this rule. Laziness changes when a safe relationship object is produced, not the proof required for that relationship.

## Port/bus ergonomics still open

Endpoint-bearing channel traversal is sufficient to prove ownership, aliasing, and arbitrary mapped routing without freezing the final author-facing bus API.

Before public API freeze, compare efficient views/helpers for:

- conventional stereo main input/output;
- optional sidechain;
- explicit port/channel lookup;
- multiple buses;
- input-only/output-only products;
- mono-to-stereo or other non-one-to-one routing.

Do not add a per-block map/allocation merely for lookup convenience. Dense endpoint helpers can be resolved at activation if real product call sites justify them.

## Sample precision

The buffer/source types are generic over `S`. Runtime processing uses a separate `Process<S>` capability rather than parameterizing `Processor` itself.

The conformance processor and CLAP adapter support `f32` and optional `f64`. F64 CLAP exports use an explicit capability marker and require one processor to implement both `Process<f32>` and `Process<f64>`; they do not create a second lifecycle object. A callback must use one precision across all mapped ports, so mixed or `Both` host representations are rejected before product DSP.

Do not genericize unrelated control/state types over sample precision.

## Render/offline mode

`ProcessContext` already distinguishes realtime-like modes from `ProcessMode::Offline`; format adapters should map an explicit host semantic signal rather than infer offline state from transport or timing.

For CLAP, that signal is the render extension. The adapter stores the host-selected realtime/offline mode in instance-local atomic adapter state and maps it into `ProcessContext` on each callback. The render setting is ephemeral host state, not persisted product state. Targeted native render qualification remains open.

## Layout and data movement

Planar/non-interleaved channel access remains the initial product convention because it matches plugin processing well.

If a future backend supplies interleaved or otherwise incompatible storage, conversion scratch must be allocated during activation and bounded by accepted channel/frame configuration. Document and benchmark conversion cost before calling it negligible.

Avoid cache padding, bespoke packing, SIMD-specific alignment, or custom allocators until measurement shows a real benefit.

## Variable and edge block sizes

Processing cannot assume a fixed frame count. Activation establishes resource bounds; each callback supplies the actual count.

Core conformance covers ordinary blocks, maximum/minimum violations, zero-frame processing where allowed, separate buffers, exact in-place buffers, and non-flat sources. Target-specific validation still needs expanded native coverage for changing block sizes, inactive/null-buffer legality where applicable, offline rendering, and higher-channel-count host configurations.

## Realtime rule

No convenience available from `Process<S>` may allocate, block, perform I/O, or perform work whose upper bound is unrelated to the validated configuration/block.

Stable validation and lookup work should be hoisted out of callbacks. Controlled bounded copies are allowed; hidden dynamic memory growth is not.

`ProcessBufferSource::channels()` is subject to the same rule. A source may traverse host-owned descriptors and construct safe borrowed views, but it may not grow owned storage merely to present those views to product DSP.
