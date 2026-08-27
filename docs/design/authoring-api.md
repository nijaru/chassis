# Authoring API and Convention Model

Status: first explicit manual runtime slice implemented; syntax and higher-level conveniences remain pre-alpha.

## Goal

A normal Chassis product should mostly contain:

- parameter declarations;
- DSP state/processing;
- optional custom persistent state;
- optional editor code.

It should not repeatedly implement format metadata, host parameter plumbing, state stream glue, bus enumeration, automation queues, native editor attachment, or per-format packaging scripts.

Convention removes mechanical work without hiding realtime timing, copies, allocation, state authority, or compatibility identity.

## Explicit layer first

Chassis now has the first executable manual lifecycle/process layer in `chassis-core`:

```text
Component
  immutable definition/factory
  default effect audio-port schema
  activate -> Processor

Processor
  exclusive active DSP/runtime-history owner
  reset

Process<S>
  sample-representation-specific processing capability
  process(ProcessBlock<S>)

Activated<P>
  immutable activation config + exclusive Processor
  canonical base ParameterStore
  reset / process / consuming deactivate
```

This is intentionally smaller than the eventual ergonomic API. It proves ownership,
buffer, typed-parameter, and bounded-state semantics before automation, proc
macros, GUI bindings, or adapters make the surface harder to change.

Do not add proc macros until the conformance component and first CLAP adapter show which declarations are genuinely repetitive.

## Current explicit effect

The current external API is approximately:

```rust,ignore
struct MyEffect;

impl chassis_core::runtime::Component for MyEffect {
    type Processor = MyProcessor;
    type ActivationError = MyActivateError;

    // Omit audio_ports() to use the standard effect descriptors.

    fn activate(
        &self,
        config: &chassis_core::process::ActivationConfig<'_>,
    ) -> Result<Self::Processor, Self::ActivationError> {
        MyProcessor::new(config)
    }
}

struct MyProcessor {
    // Product DSP/runtime history only.
}

impl chassis_core::runtime::Processor for MyProcessor {
    fn reset(&mut self) {
        // Reset transient DSP history if needed.
    }
}

impl chassis_core::runtime::Process<f32> for MyProcessor {
    fn process(
        &mut self,
        block: &mut chassis_core::process::ProcessBlock<'_, '_, f32>,
    ) {
        for channel in block.buffers_mut() {
            // Use exact in-place storage directly, or explicitly copy a
            // separate input/output pair with make_in_place().
            process(channel);
        }
    }
}
```

The framework calls `runtime::activate()` after structural audio-I/O validation and returns an `Activated<MyProcessor>`. Product code does not construct `ActivationConfig` or `ProcessBlock` directly.

This spelling is not a compatibility promise. The useful constraints are the ownership split and absence of hidden realtime work.

## Why `Processor` and `Process<S>` are separate

Do not hard-code one host sample precision into processor ownership.

A processor has one lifecycle/DSP-history object and may implement:

```text
Process<f32>
Process<f64>
```

as supported. The current conformance effect proves only `f32`; optional host f64 advertisement/dispatch remains an adapter freeze gate.

This is preferable to duplicating the whole processor architecture merely to support double precision.

## Default effect convention

With no explicit audio-port declaration, `Component::audio_ports()` returns stable conventional descriptors:

```text
audio.main.in   required input
audio.main.out  required output
audio.sidechain optional input
```

The current default active configuration remains stereo main input/output with sidechain inactive.

Important: the explicit runtime currently validates **structural port presence/identity**, not the final whole-layout policy. The default descriptors are not permission to accept every representable channel layout. A dedicated semantic I/O-policy layer must be proven before API freeze.

Inactive optional ports add no process-time buffer work.

## Buffer ergonomics

`ChannelBuffer<S>` exposes already-proven safe relationships:

- exact in-place input/output with one mutable slice;
- disjoint input/output slices;
- input-only;
- output-only.

Each side carries a stable port/channel endpoint.

`make_in_place()` is an explicit convenience: exact in-place is zero-copy; separate input/output performs one bounded copy to output. Products with out-of-place algorithms can use `input()` and `output_mut()` directly.

No process convenience allocates.

Higher-level port/bus lookup helpers should be added only after representative DSP call sites show which views are actually useful. Avoid per-callback maps or other convenience structures that create hidden work.

## Parameters are not fake fields on Processor

The current explicit runtime slice accepts an immutable schema through
`Component::parameter_descriptors()`. Activation validates and owns a
`ParameterStore` containing the current base/control values. The store supports
validated edits, defaults, and parameter entries in the bounded state document.

When added, compatibility-relevant declarations remain explicit:

- stable canonical string key;
- value type/domain;
- plain-unit range/default;
- mapping/distribution where needed;
- display/parse/unit behavior;
- automation/modulation capabilities;
- optional product smoothing policy.

Rust field names and display labels are not persistent identity. The current
store is deliberately a control/non-realtime authority; process-time event
trajectories, host gestures, and atomic publication to `Processor` remain
follow-up contracts. The processor's eventual parameter view will be derived
from framework-owned base state plus current process events, not a second
persistent store.

Nested/repeated groups should compose stable key prefixes. Repeated instances should use stable named identities when reordering may occur; raw array index is acceptable only when reordering is explicitly a compatibility break.

## Main-thread / shared defaults

Most effects should need no product-specific non-RT runtime object:

```text
MainThread = ()
Shared = ()
```

Framework-owned canonical parameter/state/lifecycle machinery will still exist internally; `()` only means the **product** has no additional state in those domains.

When custom shared state exists, it is a synchronized projection/snapshot with one named owner—not general shared mutability.

## Custom persistent state

Most user-visible state should be parameters. Extra persistent fields use stable keys and a deliberately supported Chassis value/codec contract.

`Serialize` on an arbitrary Rust struct is not by itself a long-term plugin-state contract.

## GUI convention

A component can eventually have no editor, a generic debug editor, or a product editor factory.

A GUI adapter should bind typed parameter handles that provide current display/base value observation, begin/change/end gestures, host notification, formatting/parsing, and accessibility metadata where supported.

Chassis owns editor lifecycle/host attachment; the product/toolkit owns appearance and interaction design.

## Standalone / instrument convention

The same component lifecycle/state/processor model applies outside a plugin host.

An instrument changes capabilities/I/O, not the framework architecture:

```text
note/event input
no audio input
stereo or multi-bus audio output
```

The current `Component::audio_ports()` default is merely the effect convenience; instruments override it and future event capabilities without replacing `Processor`/`Process<S>`.

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

## Promotion rule

The integration conformance component now uses the explicit public API directly. Continue to grow that path before introducing derives/builders.

A macro earns its place only when it removes repeated mechanical declarations without concealing timing, allocation, ownership, or compatibility behavior an author needs to debug.
