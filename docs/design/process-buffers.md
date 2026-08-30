# Processing and Audio Buffer Model

Status: safe channel relationships are implemented; a no-allocation generic buffer-source shape is now implemented for qualification but is not yet wired into `ProcessBlock`/`InstanceRuntime`. Higher-level port views and public API remain pre-alpha.

## Goal

Give ordinary effects a simple processing path while preserving host buffer semantics and Rust aliasing guarantees. Avoid gratuitous copies, but do not make zero-copy a goal in itself.

The core buffer model must support effects, generators/instruments, auxiliary inputs/outputs, multiple buses, variable block sizes, offline rendering, and eventual high-channel-count processing.

## Host relationships

Target formats can provide exact in-place input/output buffers or distinct buffers. Chassis therefore cannot assume every process call is in-place or every input/output pair is separate.

The adapter owns all raw-pointer validation. Product DSP never receives host pointers directly.

## Implemented safe channel view

`chassis-core::buffer::ChannelBuffer<'a, S>` currently represents four already-proven safe relationships:

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

Endpoints use stable `PortKey` + zero-based channel index. They associate the safe sample view with semantic product ports without exposing backend bus indices.

`InPlace` deliberately stores **one mutable reference only**. Chassis never constructs both `&[S]` and `&mut [S]` for the same host range.

`Separate` can only be constructed from safe Rust references that are already disjoint. Any unsafe pointer arithmetic/range proof needed to create those references belongs in the format adapter.

Input-only and output-only views preserve generators, analyzers, event-oriented components, unusual routing, and multi-output products without forcing a fake one-to-one effect topology.

## Construction and frame bounds

Channel constructors accept the current callback frame count, validate that backing storage is long enough, and expose exactly that prefix. They never allocate.

`ProcessConfig` also carries the activation-owned maximum number of normalized
parameter events accepted per callback. Adapters choose that bound from their
accepted host/product contract; zero is valid for an adapter that has no event
projection yet.

A short host/synthetic buffer is rejected with a typed `ChannelBufferError` before product DSP receives it.

This slice does not attempt to infer or validate raw pointer aliasing because the existence of the safe Rust references is itself the post-validation boundary.

## Conventional in-place helper

`ChannelBuffer::make_in_place()` is the explicit convenience path for ordinary in-place DSP:

- exact in-place: returns the existing `&mut [S]` with no copy;
- separate: performs one bounded `copy_from_slice` from input to output and returns output;
- input-only: fails because no output exists;
- output-only: fails because no input exists to copy.

This copy is controlled product/framework behavior, not hidden adapter normalization. It is acceptable when it simplifies ownership and measurement does not justify a separate DSP path.

Products that benefit from out-of-place processing can use `input()` and `output_mut()` directly and skip the copy.

## No-allocation buffer sources

General CLAP I/O exposed a representation constraint that the first flat-slice model cannot solve cleanly: the number of active ports/channels is negotiated at runtime, but lifetime-bearing `ChannelBuffer` values cannot be retained in processor storage across callbacks. Building `Vec<ChannelBuffer>` every callback would violate the realtime contract, and lifetime erasure would weaken the aliasing model.

The proposed source shape is therefore iterator-based rather than collection-based:

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

`ProcessChannel<S>` deliberately abstracts only operations already provided by `ChannelBuffer`; it is not a second buffer semantic model. `ChannelBuffer` and a mutable borrow of `ChannelBuffer` both implement it.

`ProcessBufferSource<S>` uses generic associated types for its channel item and iterator. That allows a materialized slice to yield borrowed `&mut ChannelBuffer` items while a format adapter can later yield freshly constructed safe `ChannelBuffer` values directly from its host iterator. No trait-object lifetime erasure or callback-owned collection is required.

`ChannelBufferSlice` is the compatibility source over the current flat slice. A core qualification test also implements a source by chaining two independent channel slices. That test exists specifically to establish that a legal process traversal does not require one contiguous/flat `ChannelBuffer` collection.

`validate_frame_count()` is separate from traversal because all callback-varying dimensions still have to be checked before product DSP runs. A future CLAP source must validate all host port/channel dimensions and sample representation before yielding product-visible channel views.

This source API is currently implemented but not yet qualified or wired into `ProcessBlock`. The next step is to make `ProcessBlock` and `Process<S>` generic over the source while retaining the current slice entry point as a compatibility wrapper.

## ProcessBlock

`ProcessBlock<'buffers, 'samples, 'context, 'parameters, S>` is the current borrowed realtime call type. It contains:

- actual frame count;
- per-call `ProcessContext` with `ProcessMode`;
- block-start transport snapshot;
- validated borrowed parameter event views;
- a borrowed immutable view of the active base/control `ParameterStore`;
- currently, a borrowed mutable slice of safe `ChannelBuffer<S>` views.

Its constructor is framework-private. `Activated::process()` first validates
context, activation event bounds, and callback-varying dimensions, then checks
event values against the active schema before product DSP receives the block.

The block currently validates:

- positive activation minimum when one was guaranteed;
- activated maximum frame count;
- the event context was validated for this exact callback frame count;
- every supplied channel view has exactly the callback frame count.

`ParameterEvents` validates a caller-supplied event count bound, finite values,
canonical identifiers, and nondecreasing sample offsets without copying event
storage. The active parameter schema then validates types and domains before
product DSP. `FloatParameterCursor` evaluates set/linear trajectories lazily;
it never expands a ramp into one event per sample.

It intentionally does **not** rescan the full stable endpoint/schema mapping every callback. That mapping should be resolved once by adapter/runtime setup so high-channel-count processing does not gain hidden allocation or O(n²) semantic validation work.

The source migration must preserve that rule: generic source traversal is not permission to redo stable semantic lookup each block. CLAP keeps stable port-key/ID/dense-index mapping in setup-owned state and uses those retained identities while generating callback channel views.

## Rust aliasing contract

A future adapter may construct `InPlace` only for a valid exact alias where a single mutable slice is the sole live Rust reference to that memory.

It may construct `Separate` only after proving input and output ranges are disjoint for the full borrow lifetime.

Unexpected partial overlap, overlapping output channels, inconsistent lengths, invalid/null pointers, or any host state that cannot satisfy Rust reference rules must be contained at the adapter boundary. Never manufacture overlapping Rust references because a host specification says the host *should* behave.

If a legal backend relationship is awkward to express with ordinary references, keep raw pointers/private unsafe machinery inside the adapter and expose a smaller safe accessor. Do not weaken the product-facing API to raw pointers for adapter convenience.

The generic source does not change this rule. A lazy adapter iterator may defer construction of each `ChannelBuffer`, but every yielded view must already satisfy the same ordinary-reference aliasing guarantees as a materialized view.

## Port/bus ergonomics still open

The endpoint-bearing channel traversal is sufficient to prove alias ownership without freezing the final higher-level DSP ergonomics.

Before public API freeze, compare efficient views/helpers for:

- conventional stereo main input/output;
- optional sidechain;
- explicit port/channel lookup;
- multiple buses;
- input-only/output-only products;
- mono-to-stereo or other non-one-to-one routing.

Do not add a per-block map/allocation simply to make lookup convenient. Dense endpoint indices can be resolved at activation if real call sites justify them.

The generic source intentionally solves storage/lifetime representation first. It does not yet assert that a flat endpoint traversal is the final author-facing bus API.

## Sample precision

The buffer types are generic over `S`. Runtime processing uses a separate `Process<S>` capability rather than parameterizing `Processor` itself.

The initial conformance processor implements `Process<f32>`. A future processor can additionally implement `Process<f64>` without a second lifecycle object.

The buffer source is likewise parameterized over `S`; a format adapter chooses one source representation for the sample precision actually dispatched into the corresponding `Process<S>` implementation.

This establishes a useful direction but does not yet freeze host precision advertisement/dispatch. The first CLAP/VST3 work must still prove how optional f64 support is declared and selected.

Do not genericize unrelated control/state types over sample precision.

## Layout and data movement

Planar/non-interleaved channel access remains the initial product convention because it matches plugin processing well.

If a backend supplies interleaved or otherwise incompatible storage, conversion scratch is allocated during activation and bounded by accepted channel/frame configuration. Document and benchmark conversion cost before calling it negligible.

Avoid cache padding, bespoke packing, SIMD-specific alignment, or custom allocators until measurement shows a real benefit.

## Variable and edge block sizes

Processing cannot assume a fixed frame count. Activation establishes resource bounds; each callback supplies the actual count.

The current conformance slice covers:

- ordinary blocks;
- exact configured maximum rejection when exceeded;
- a guaranteed minimum rejection when violated;
- zero-frame processing when no positive minimum was promised;
- separate and exact-in-place sample storage.

Future adapter tests add target-specific zero/null/inactive-buffer legality, changing block sizes, offline rendering, and high-channel-count cases.

## Realtime rule

No convenience available from `Process<S>` may allocate, block, perform I/O, or perform work whose upper bound is unrelated to the validated configuration/block.

Stable validation and lookup work should be hoisted out of callbacks when possible. Controlled bounded copies are allowed; hidden dynamic memory growth is not.

`ProcessBufferSource::channels()` is subject to the same rule. A source may traverse host-owned descriptors and construct safe borrowed views, but it may not grow owned storage merely to present those views to product DSP.