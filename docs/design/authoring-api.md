# Authoring API and Convention Model

Status: design target; syntax, trait names, and macros are illustrative until the explicit semantic core and first adapter prove them.

## Goal

A normal Chassis product should mostly contain:

- parameter declarations;
- DSP state/processing;
- optional custom persistent state;
- optional editor code.

It should not repeatedly implement format metadata, host parameter plumbing, state stream glue, bus enumeration, automation queues, native editor attachment, or per-format packaging scripts.

Convention removes mechanical work without hiding realtime timing, copies, allocation, state authority, or compatibility identity.

## Explicit layer first

Chassis should have one small explicit Rust API that owns the semantics. Optional derives/builders generate ordinary calls/impls against that API; they do not create a second runtime model.

Do not add proc macros until the conformance component and first CLAP adapter show which declarations are genuinely repetitive.

## Desired ordinary effect

Illustrative end-state:

```rust,ignore
#[derive(chassis::Parameters)]
struct Params {
    #[param(id = "input.gain", range = -24.0..=24.0, default = 0.0, unit = "dB")]
    input_gain: f32,

    #[param(id = "mix", range = 0.0..=1.0, default = 1.0)]
    mix: f32,
}

struct MyEffect;

impl chassis::Component for MyEffect {
    type Parameters = Params;
    type Processor = MyProcessor;

    fn activate(
        &self,
        config: &chassis::ActivationConfig<'_>,
    ) -> Result<MyProcessor, chassis::ActivateError> {
        MyProcessor::new(config)
    }
}

struct MyProcessor {
    // Product DSP/runtime history only.
}

impl chassis::Processor<Params> for MyProcessor {
    fn process(&mut self, block: &mut chassis::ProcessBlock<'_, Params>) {
        // Product DSP using block-local parameter trajectories and safe buffers.
    }
}
```

Exact signatures are intentionally not frozen. The ergonomic invariant is that an ordinary effect does not need custom host-format traits, state codecs, port tables, `Shared`, or `MainThread` merely to process audio correctly.

## Default effect convention

With no explicit audio-I/O declaration, an audio effect gets stable conventional ports:

```text
audio.main.in   stereo required
audio.main.out  stereo required
audio.sidechain stereo optional/inactive
```

Inactive optional ports add no process-time buffer work.

Common deviations use semantic policies/builders such as mono, mono-to-stereo, matching arbitrary layouts, or explicit bus sets. Authors never translate those policies separately into CLAP/VST3/AU metadata.

## Parameter declarations

Compatibility-relevant choices remain explicit:

- stable canonical string key;
- value type/domain;
- plain-unit range/default;
- mapping/distribution where needed;
- display/parse/unit behavior;
- automation/modulation capabilities;
- optional product smoothing policy.

Rust field names and display labels are not persistent identity.

Nested/repeated groups compose stable key prefixes. Repeated instances should use stable named identities when reordering may occur; raw array index is acceptable only when reordering is explicitly a compatibility break.

## DSP parameter access

The normal processing API must make sample-accurate correctness easy without forcing per-sample dynamic lookup when nothing changes.

Conceptually:

```rust,ignore
let threshold = block.params().threshold();

if let Some(value) = threshold.constant() {
    process_block(value);
} else {
    for span in block.spans() {
        process_span(span.frames(), span.params().threshold);
    }
}
```

The actual API may differ, but it should expose block-constant fast paths, timed sets/ramps, and raw ordered events where needed.

The processor's parameter view is derived from the framework-owned base-state authority plus current process events; it is not a second persistent store.

## Buffer ergonomics

Chassis exposes safe relationships proved by the adapter:

- exact in-place channel view;
- disjoint input/output channel views;
- input-only/output-only buses;
- port/channel iteration for unusual routing.

An explicit convenience can turn a separate input/output pair into a mutable output by performing one bounded copy. That is acceptable and visible behavior, not a hidden cost. Products that benefit from out-of-place DSP can avoid it.

No process convenience allocates.

## Main-thread / shared defaults

Most effects should need no product-specific non-RT runtime object:

```text
MainThread = ()
Shared = ()
```

Framework-owned canonical parameter/state/lifecycle machinery still exists internally; `()` only means the **product** has no additional state in those domains.

When custom shared state exists, it is a synchronized projection/snapshot with one named owner—not general shared mutability.

## Custom persistent state

Most user-visible state should be parameters. Extra persistent fields use stable keys and a deliberately supported Chassis value/codec contract.

Illustrative direction:

```rust,ignore
#[derive(chassis::State)]
struct ProductState {
    #[state(id = "quality.mode")]
    quality: QualityMode,
}
```

`Serialize` on an arbitrary Rust struct is not by itself a long-term plugin-state contract.

## GUI convention

A component can have no editor, a generic debug editor, or a product editor factory.

A GUI adapter should bind generated typed parameter handles that automatically provide current display/base value observation, begin/change/end gestures, host notification, formatting/parsing, and accessibility metadata where supported.

Chassis owns editor lifecycle/host attachment; the product/toolkit owns appearance and interaction design.

## Standalone / instrument convention

The same component lifecycle/state/processor model applies outside a plugin host.

An instrument changes capabilities/I/O, not the framework architecture:

```text
note/event input
no audio input
stereo or multi-bus audio output
```

Standalone supplies device/MIDI/window/runtime ownership around the same product implementation.

## Escape hatches

A convention-first framework needs deliberate lower-level paths for:

- custom I/O policy;
- manual parameter schema;
- explicit legacy/backend ID overrides;
- raw timed parameter/events;
- custom smoothing;
- out-of-place/multi-bus buffer processing;
- custom state field codec/migrations;
- custom editor/window integration;
- format-specific optional capabilities through adapter extension APIs.

Escape hatches do not relax memory safety, realtime, or stable-identity invariants.

## `cargo-chassis`

Build tooling should eventually own workspace/product discovery, export builds, local install, identity manifests, validators, packaging, signing, and notarization.

Likely UX:

```text
cargo chassis new
cargo chassis build
cargo chassis install
cargo chassis validate
cargo chassis package
cargo chassis identity
```

Build/release behavior belongs in tooling, not runtime proc macros.

When implementing this CLI, apply the repository's Rust CLI guidance in addition to `rust-expert`; do not invent argument/config conventions ad hoc.

## Promotion rule

Before introducing convenience macros, write the conformance component using the explicit API and count the real repetition. A macro earns its place only when it removes mechanical declarations without concealing timing, allocation, ownership, or compatibility behavior an author needs to debug.
