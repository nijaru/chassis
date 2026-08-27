# Processing and Audio Buffer Model

Status: design direction; public API not frozen.

## Goal

Give ordinary effects a simple processing path while preserving host buffer semantics and Rust aliasing guarantees. Avoid gratuitous copies, but do not make zero-copy a goal in itself.

The core buffer model must support effects, generators/instruments, auxiliary inputs/outputs, multiple buses, variable block sizes, offline rendering, and eventual high-channel-count processing.

## Host relationships

Target formats can provide exact in-place input/output buffers or distinct buffers. Chassis therefore cannot assume every process call is in-place or every input/output pair is separate.

The adapter owns all raw-pointer validation. Product DSP never receives host pointers directly.

## Rust aliasing contract

The safe Chassis view may only contain references whose exclusivity has actually been proved.

Conceptually the useful relationships are:

```text
ChannelBuffer<S>
├── InPlace(&mut [S])
├── Separate { input: &[S], output: &mut [S] }
├── InputOnly(&[S])
└── OutputOnly(&mut [S])
```

This is semantic pseudocode, not a promise that the final type is this enum.

An adapter may construct `InPlace` only for a valid exact alias where a single mutable slice is the sole live Rust reference to that memory. It may construct `Separate` only after proving the ranges are disjoint for the duration of the borrow.

Unexpected partial overlap, overlapping output channels, inconsistent frame lengths, invalid/null pointers, or other host data that cannot satisfy Rust's reference rules must be handled at the adapter boundary. Never manufacture overlapping `&`/`&mut` references and rely on the host specification to make undefined behavior impossible.

If a backend makes some legal relationship awkward to express with ordinary references, keep raw pointers/private unsafe machinery inside the adapter and expose a smaller safe accessor whose borrowing rules can be enforced. Do not weaken the product-facing API to raw pointers merely for adapter convenience.

## Conventional in-place helper

Many effects naturally implement an in-place transform. Chassis should make that path easy without making it the underlying representation.

Conceptually an explicit helper can behave as follows:

- exact in-place input/output: return the existing mutable output view with no copy;
- separate input/output: copy the bounded input block into output once, then process output in place;
- output-only: allow mutable output after explicit initialization semantics;
- input-only: reject an in-place output request.

The copy is controlled product/framework behavior, not an accidental adapter normalization step. It is acceptable when it makes ownership clearer and measurement does not justify a more complex DSP path.

DSP that benefits from separate input/output buffers can consume those views directly and avoid the copy.

## Ports and process block

Audio buffers are associated with the accepted `AudioIoConfiguration`, not hard-coded stereo positions or backend bus indices.

A process call eventually needs a realtime-only context conceptually containing:

```text
ProcessBlock
├── actual frame count
├── safe audio port/buffer views
├── parameter automation/modulation trajectories
├── ordered note/MIDI/event input and bounded output sink
├── transport/process context
└── narrow realtime-safe host/runtime capabilities
```

The exact decomposition remains open. Product code should be able to obtain conventional main stereo/sidechain views cheaply while unusual products can address multiple buses explicitly.

## Sample precision

Do not hard-code `f32` into the long-term semantic model. Initial code may prove the path with `f32`, but CLAP/VST3 allow useful double-precision paths.

Before freezing `Processor`, compare:

- an `f32` base processor plus optional double-precision capability;
- generic DSP over a sealed Chassis sample trait with adapter dispatch;
- generated paired entry points backed by shared generic product DSP.

Do not require authors to duplicate the whole DSP implementation simply to support `f64`. Also do not genericize every unrelated control type over sample precision.

## Layout and data movement

Planar/non-interleaved channel access is the initial product convention because it matches plugin processing well.

If a backend supplies interleaved or otherwise incompatible storage, any conversion scratch is allocated during activation and bounded by the accepted channel/frame configuration. The adapter should document and benchmark conversion cost before Chassis claims it is negligible.

Avoid cache-line padding, SoA/AoS transformations, SIMD-specific alignment, or bespoke buffer packing until the real access pattern shows a benefit. Data layout on the realtime path is a measured design decision.

## Variable and edge block sizes

Processing cannot assume a fixed frame count. Activation establishes supported bounds/resources; each callback supplies the actual count.

Conformance tests should cover:

- blocks smaller than common vector widths;
- changing block sizes across calls;
- the configured maximum;
- zero-frame calls where a target permits them;
- offline rendering with unusual block sizes;
- inactive/null/absent buffers only where a target permits them.

Every per-block loop and scratch view must be bounded by validated activation/callback dimensions. Reject host values that exceed the activated capacity rather than reallocating on the audio thread.

## Silence and activity hints

Backend silence flags/scheduling hints are adapter capabilities, not prerequisites for the first buffer API. Add a portable semantic only after cross-format behavior is proven useful. A processor can always remain conservatively active.

## Realtime rule

No Chassis convenience method available from `Processor::process` may allocate, block, perform I/O, or perform work whose upper bound is unrelated to the validated block/configuration.

Scratch required for conversion or convenience copies is provisioned before processing. Controlled bounded copies are allowed; hidden dynamic memory growth is not.
