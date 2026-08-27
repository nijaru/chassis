# Processing and Audio Buffer Model

Status: first safe channel-view slice implemented; higher-level port views and public API remain pre-alpha.

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

## ProcessBlock

`ProcessBlock<'buffers, 'samples, S>` is the first borrowed realtime call type. It contains:

- actual frame count;
- per-call `ProcessMode`;
- a borrowed mutable slice of safe `ChannelBuffer<S>` views.

Its constructor is framework-private. `Activated::process()` constructs the block after validating callback-varying dimensions.

The block currently validates:

- positive activation minimum when one was guaranteed;
- activated maximum frame count;
- every supplied channel view has exactly the callback frame count.

It intentionally does **not** rescan the full stable endpoint/schema mapping every callback. That mapping should be resolved once by adapter/runtime setup so high-channel-count processing does not gain hidden allocation or O(n²) semantic validation work.

The first CLAP adapter/conformance host must prove the exact setup-time representation used for this resolved mapping.

## Rust aliasing contract

A future adapter may construct `InPlace` only for a valid exact alias where a single mutable slice is the sole live Rust reference to that memory.

It may construct `Separate` only after proving input and output ranges are disjoint for the full borrow lifetime.

Unexpected partial overlap, overlapping output channels, inconsistent lengths, invalid/null pointers, or any host state that cannot satisfy Rust reference rules must be contained at the adapter boundary. Never manufacture overlapping Rust references because a host specification says the host *should* behave.

If a legal backend relationship is awkward to express with ordinary references, keep raw pointers/private unsafe machinery inside the adapter and expose a smaller safe accessor. Do not weaken the product-facing API to raw pointers for adapter convenience.

## Port/bus ergonomics still open

The endpoint-bearing channel list is sufficient to prove alias ownership without freezing the final higher-level DSP ergonomics.

Before public API freeze, compare efficient views/helpers for:

- conventional stereo main input/output;
- optional sidechain;
- explicit port/channel lookup;
- multiple buses;
- input-only/output-only products;
- mono-to-stereo or other non-one-to-one routing.

Do not add a per-block map/allocation simply to make lookup convenient. Dense endpoint indices can be resolved at activation if real call sites justify them.

## Sample precision

The buffer types are generic over `S`. Runtime processing uses a separate `Process<S>` capability rather than parameterizing `Processor` itself.

The initial conformance processor implements `Process<f32>`. A future processor can additionally implement `Process<f64>` without a second lifecycle object.

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
