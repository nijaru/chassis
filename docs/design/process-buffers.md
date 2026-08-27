# Processing and Audio Buffer Model

Status: design direction; public API not frozen.

## Goal

Give ordinary effects a simple processing path without forcing hidden copies or losing the buffer semantics exposed by plugin hosts.

The core buffer model must support effects, generators/instruments, auxiliary inputs/outputs, multiple buses, variable block sizes, offline rendering, and eventual high-channel-count processing.

## Host evidence

The target formats permit multiple buffer relationships:

- CLAP exposes read-only input buffers and writable output buffers and separately declares which ports may be processed in place.
- VST3 explicitly permits an input and output channel pointer to be the same or different.
- Audio Units can process in place and also render into separate output buffers.

Therefore Chassis should not assume that every process call is in-place or that every input/output pair is separate.

## Safe channel relationship

The format adapter is responsible for turning host pointers into a safe Chassis representation. Product DSP should not manipulate raw pointers.

A useful conceptual representation is:

```text
ChannelBuffer<S>
├── InPlace(&mut [S])
├── Separate { input: &[S], output: &mut [S] }
├── InputOnly(&[S])
└── OutputOnly(&mut [S])
```

The exact enum/API may change, but preserving this distinction avoids unnecessary framework copies and makes host aliasing explicit.

Unsafe pointer/alias validation required to construct these references belongs in a format adapter, not `chassis-core` product code.

## Conventional in-place helper

Many effects naturally implement an in-place transform. Chassis should make that path easy without making it the underlying representation.

Conceptually a helper can provide:

```text
channel.make_in_place()
```

Behavior:

- `InPlace`: return the existing mutable slice with no copy;
- `Separate`: copy input to output once, then return the output slice;
- `OutputOnly`: return the output slice after the product chooses initialization semantics;
- `InputOnly`: not valid for an in-place output transform.

The copy is therefore explicit in API semantics and only happens when required by the host relationship and the product chooses the convenience path.

DSP that benefits from separate input/output buffers can use the raw safe representation and avoid that copy.

## Ports and process block

Audio buffers are indexed/mapped against the active `AudioIoConfiguration`, not against hard-coded stereo positions.

A process call eventually needs a block-level context containing at least:

```text
ProcessBlock
├── frame count
├── audio input/output port buffers
├── timestamped parameter/automation/modulation events
├── note/MIDI/event input and output
├── transport/process context
└── bounded realtime host callbacks
```

The exact decomposition into structs/arguments remains open. The important rule is that the processor receives only realtime-safe capabilities.

## Sample precision

Chassis should not hard-code `f32` into the long-term processing model. CLAP requires 32-bit audio support and makes 64-bit optional; VST3 can process either 32- or 64-bit samples.

Initial implementation may prove the API with `f32`, but the design must leave an ergonomic path for products that also support `f64`.

Likely directions to compare before freezing the trait:

- required `f32` processor plus an optional double-precision capability/trait;
- a generic process method over a sealed Chassis sample trait;
- paired framework-generated dispatch methods backed by shared generic product DSP.

Do not force every plugin author to duplicate an entire DSP implementation merely to advertise optional 64-bit host buffers.

## Buffer layout

The product-facing convention should prefer planar/non-interleaved channel slices because that matches the normal plugin processing model and makes per-channel DSP straightforward.

Adapters must review any backend that supplies interleaved audio. Do not silently introduce an unbounded or per-block allocation to normalize it. If copying/deinterleaving is required for a particular backend, preallocate scratch storage during activation and make the cost explicit in adapter documentation/validation.

## Variable and edge block sizes

Processing code must not assume a fixed block size. Activation supplies an allowed/minimum/maximum range; each process call supplies the actual frame count.

The conformance suite should test:

- blocks smaller than typical SIMD/vector widths;
- changing block sizes across calls;
- maximum configured block size;
- zero-frame calls if a target format/host permits them;
- offline rendering with unusual block sizes;
- inactive/null buffers where target formats permit them.

## Silence and status

Host formats can expose silence flags or process-return status that permit sleeping/tail optimization. Chassis should eventually provide a semantic process status rather than leaking backend constants.

This belongs after the base buffer/event API works correctly; it should not complicate the first processing spike.

## Realtime rule

No Chassis convenience method may allocate during processing unless the method is explicitly documented as non-realtime and structurally unavailable from the realtime context. Any scratch needed for conversion, smoothing, analysis transport, or format adaptation must be bounded/preallocated during activation.
