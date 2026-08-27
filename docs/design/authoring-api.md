# Authoring API and Convention Model

Status: design target; syntax, trait names, and macros are illustrative until the semantic core and first adapter prove them.

## Goal

A normal Chassis plugin should mostly contain product code:

- parameter declarations;
- DSP state and processing;
- optional custom state;
- optional editor code.

It should not repeatedly implement format metadata, host parameter plumbing, state serialization glue, bus enumeration, automation queues, editor-host attachment, packaging scripts, or validator invocation.

Chassis should feel convention-first without hiding realtime timing, allocation, or compatibility semantics.

## Two-layer API

Provide both:

1. a small explicit Rust trait/type API that defines the real semantics; and
2. optional derive/attribute macros and constructors that generate repetitive conventional implementations.

The explicit layer remains usable when a product cannot fit the macros. Macros must lower to ordinary documented Chassis APIs rather than creating a second hidden runtime model.

Do not introduce proc macros until the manual semantic API has been exercised by the conformance component and first CLAP adapter.

## Ordinary effect target

The eventual conventional authoring experience should be approximately this amount of code:

```rust,ignore
#[derive(chassis::Parameters)]
struct Params {
    #[param(id = "input.gain", range = -24.0..=24.0, default = 0.0, unit = "dB")]
    input_gain: f32,

    #[param(id = "mix", range = 0.0..=1.0, default = 1.0, unit = "%")]
    mix: f32,
}

struct MyEffect;

impl chassis::Component for MyEffect {
    type Parameters = Params;
    type Processor = MyProcessor;

    // No explicit ports means the standard effect convention:
    // stereo main in/out + optional stereo sidechain.

    fn activate(
        &self,
        config: &chassis::ActivationConfig<'_>,
        params: &chassis::ParameterState<Params>,
    ) -> Result<MyProcessor, chassis::ActivateError> {
        MyProcessor::new(config, params)
    }
}

struct MyProcessor {
    // Product DSP state only.
}

impl chassis::Processor<Params> for MyProcessor {
    fn process(
        &mut self,
        block: &mut chassis::ProcessBlock<'_, Params>,
    ) -> chassis::ProcessStatus {
        // Product DSP.
        chassis::ProcessStatus::Continue
    }
}
```

The exact signatures will change. The important ergonomic target is that a simple effect does not need custom `Shared`, `MainThread`, port descriptors, state codecs, host-format traits, or export boilerplate unless it actually needs non-default behavior.

## Metadata convention

Common identity/vendor/build metadata should normally come from workspace/package metadata consumed by `cargo-chassis`, not from associated constants repeated in product code.

The Rust `Component` implementation therefore focuses on runtime/product semantics. A programmatic metadata API remains available for generated or unusual products.

## Default effect convention

If an audio effect does not declare I/O, Chassis supplies:

```text
main input      stereo required
main output     stereo required
sidechain       stereo optional/inactive by default
```

A product that does not use sidechain pays no process-time cost merely because the conventional descriptor exists.

Common variations should be declarative:

```rust,ignore
fn audio_io() -> impl AudioIoPolicy {
    chassis::io::mono_to_stereo()
}
```

or generated metadata equivalent. Authors should not manually translate port arrays for each target format.

## Parameters

Parameter declarations should make compatibility-relevant choices explicit and infer routine host plumbing.

A declaration needs, directly or by reusable type/default:

- stable canonical ID;
- type;
- plain-unit range/domain;
- default;
- optional distribution/mapping;
- formatting/parsing/unit behavior;
- optional smoothing convention;
- capability metadata such as automation/modulation/read-only where relevant.

Rust field names and display labels are not persistent identity.

Nested/repeated parameter groups should compose without manually concatenating IDs:

```rust,ignore
#[derive(chassis::Parameters)]
struct EqParams {
    #[params(prefix = "band.low")]
    low: BandParams,

    #[params(prefix = "band.high")]
    high: BandParams,
}
```

Arrays/repeated groups need deterministic stable instance identifiers. Array index is acceptable only when reordering is explicitly a compatibility break; named keys are preferred where product structure may evolve.

## DSP parameter access

The convenient DSP API must preserve the distinction between control/base state and sample-accurate process values.

Avoid an API where every processor merely reads a cross-thread atomic `params.threshold.value()` once per block and accidentally ignores automation points inside the block.

A process block should make common patterns easy, for example conceptually:

```rust,ignore
let threshold = block.params().threshold();

// Constant if no events affect it in this block.
if let Some(value) = threshold.constant() {
    process_block(value);
} else {
    for span in block.spans() {
        process_span(span.frames(), span.params().threshold);
    }
}
```

or an equivalent generated parameter cursor/trajectory API.

The framework should optimize the no-automation/common case so sample-accurate correctness does not require per-sample dynamic lookup when the value is constant.

Products can request a conventional smoother/trajectory. Products with custom detector/control behavior can consume raw timed parameter events.

## Processing ergonomics

Chassis should offer multiple zero/low-overhead views rather than prescribe one DSP loop:

- port/channel block access for vectorized/block DSP;
- channel-pair relationship preserving in-place versus separate host buffers;
- an opt-in `make_in_place` convenience that copies only when required;
- event/parameter span iteration for sample-accurate changes;
- raw ordered event iteration for products that need exact mixed event ordering.

No convenience adapter may allocate on the realtime path.

## Main-thread and shared state defaults

Most effects should need neither custom type:

```text
Shared = ()
MainThread = ()
```

A component opts into them only when needed.

The author-facing API may express this through associated-type defaults if Rust supports the desired ergonomics by implementation time, framework wrapper types, or separate extension traits. Do not make users write meaningless boilerplate just because the internal runtime has explicit domains.

## Custom persistent state

Most persistent state should be parameters. Additional product state should be declarative and versioned without making arbitrary processor fields persistent.

Illustrative direction:

```rust,ignore
#[derive(chassis::State)]
struct ProductState {
    #[state(id = "quality.mode")]
    quality: QualityMode,
}
```

or fields nested into the parameter/state schema.

Custom state values need a deliberately supported stable encoding. `Serialize` alone is not sufficient evidence that an arbitrary Rust type is safe as a long-term plugin state contract.

## GUI convention

The component can have no editor, use a generic debugging parameter editor, or register a product editor factory.

A GUI adapter such as `chassis-iced` should bind parameters through typed handles generated from the same parameter schema:

```rust,ignore
knob(params.input_gain())
```

The binding should automatically provide:

- current base/display value observation;
- begin/change/end gesture calls;
- host notification;
- value formatting/parsing;
- accessibility metadata where the GUI toolkit supports it.

The widget appearance remains product/toolkit code. Chassis should not require a visual component system merely to get correct parameter gestures.

## Standalone convention

A Chassis component that does not depend on plugin-host-only capabilities should be deployable by adding a standalone target/configuration rather than implementing a second product runtime.

The standalone host supplies the same activation/process/state/editor boundaries. Device/MIDI selection and app/window chrome belong to `chassis-standalone`, not to the product processor.

## Instruments

The same authoring model must work when the conventional I/O changes:

```rust,ignore
impl chassis::Component for MySynth {
    type Parameters = SynthParams;
    type Processor = SynthProcessor;

    fn capabilities() -> ComponentCapabilities {
        chassis::instrument()
            .stereo_output()
            .note_input()
    }
}
```

Again, syntax is illustrative. The significant rule is that the core lifecycle/parameter/state/process model does not change for instruments; only declared capabilities/I/O do.

## Escape hatches

Convention-over-configuration requires explicit escape hatches at each major layer:

- custom I/O negotiation policy;
- manually implemented parameter schema;
- custom host-ID overrides for legacy compatibility;
- raw timed parameter/event access;
- custom smoothing;
- separate input/output buffer processing;
- custom state codec fields/migrations;
- custom GUI toolkit/window implementation;
- format-specific extensions kept behind adapter extension APIs.

An escape hatch should not require forking Chassis. It also should not weaken invariants such as realtime safety or stable identity by default.

## What `cargo-chassis` should eventually do

The command-line tooling is part of the convention story:

```text
cargo chassis new
cargo chassis build
cargo chassis install
cargo chassis validate
cargo chassis package
cargo chassis identity
```

Likely responsibilities:

- discover Chassis product metadata from the Cargo workspace;
- build requested export formats;
- generate/copy bundles into conventional locations;
- generate and check stable identity manifests;
- run relevant validators;
- package/sign/notarize through explicit platform configuration;
- create reproducible release artifacts.

Do not put build/package behavior into proc macros or runtime crates.

## Design references

NIH-plug and nice-plug demonstrate useful authoring ideas such as derived typed parameters, stable string parameter IDs, nested parameter groups, built-in smoothers, standalone export, and optional background tasks. Clack demonstrates a lower-level structural separation between shared/main/audio domains.

Chassis should adopt the underlying lessons where they fit while retaining its own semantics:

- no unconditional input-to-output copy in the base buffer model;
- no requirement that all mutable plugin/product state live in one object owned by the audio thread;
- state serialization separated from live DSP runtime state;
- one format-independent component identity and state model;
- broader deployment path to embedded/standalone/application components.

## First proof

Before adding macros, write the conformance component using the explicit API. Measure the amount and repetition of code.

Only then introduce convenience derives/builders for boilerplate that is demonstrably mechanical. A macro is successful when it removes repeated declarations without concealing behavior an author needs to reason about during realtime processing or compatibility debugging.
