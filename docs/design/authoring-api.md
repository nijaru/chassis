# Authoring API and Convention Model

Status: explicit manual runtime/state/process APIs are implemented and pre-alpha. `ComponentSchema` + `InstanceRuntime<P>` are now the semantic ownership target; historical per-family schema accessors and conventional-effect defaults remain temporary migration surfaces.

## Goal

A normal audio component should mostly contain:

- one coherent immutable component schema;
- DSP/runtime state and processing;
- optional custom persistent state;
- optional editor/application integration.

Chassis should own repeated audio-runtime plumbing, state lifecycle, timing/event semantics, validation, deployment integration, and tooling without hiding realtime costs or compatibility identity.

Plugins should be especially easy to author because they are a common Chassis deployment, but the component API must remain equally valid for embedded processors, graph nodes, standalone applications, hosts, and audio engines.

## Target explicit model

```text
Component
  immutable definition / processor factory
  exposes one coherent ComponentSchema
  activate_with_state(...) -> Processor

ComponentSchema
  semantic state identity/version
  audio ports
  event ports
  parameters
  stable -> dense setup lookup
  future common capabilities / I/O policy

InstanceRuntime<P>
  owns one validated ComponentSchema generation
  durable ParameterStore + custom semantic state
  inactive/active lifecycle
  validated accepted audio configuration
  active Processor ownership
  activation-scoped latency snapshot

Processor
  exclusive active DSP/runtime-history owner
  caches activation-resolved dense identities/resources
  reset / restart request / latency

Process<S>
  sample-representation-specific processing capability
  process(ProcessBlock<S>)
```

The ownership split is the durable contract; exact Rust method spelling is still being simplified.

## Runtime construction

The preferred format-independent path is now:

```rust,ignore
let mut runtime = InstanceRuntime::for_component(&component)?;
runtime.activate(&component, process_config, audio_configuration)?;

// non-realtime control path
runtime.parameters_mut().set("gain", ParameterValue::Float(0.5))?;

// stable for one active lifetime
let latency = runtime.active_latency();

// realtime path supplied by a plugin/device/graph/embedding boundary
runtime.process(frame_count, context, buffers)?;

runtime.deactivate()?;
```

`InstanceRuntime::new(...)` and `new_with_event_ports(...)` are proof-era migration constructors and are not the intended stable API. They should be deleted after remaining fixtures migrate.

## Component schema authority

The current `Component` trait still exposes historical methods such as `audio_ports()`, `event_ports()`, and `parameter_descriptors()`, with a default `schema()` method that folds them into `ComponentSchema`.

This is temporary.

The stable direction is one coherent schema authority so audio/event/parameter metadata cannot drift independently and deployment/application layers can inspect the same definition.

A component schema owns semantic persistence identity separately from deployment identities:

```text
ComponentId + StateSchemaVersion
    semantic state/preset/project identity

CLAP ID / VST3 class ID / AU codes / bundle IDs
    deployment projections
```

Do not require a plugin-format identifier merely to instantiate an embedded Chassis processor.

## Stable versus dense identity

Authoring/setup identity and realtime identity are intentionally different:

```text
PortKey       -> AudioPortIndex
EventPortKey  -> EventPortIndex
ParameterKey  -> ParameterIndex
```

Stable keys are readable compatibility identities. Dense indices are immutable schema-local projections used after setup.

Processors that need named ports resolve them during activation and store the dense indices in processor state. They should not perform stable-string lookup in the callback.

Example direction:

```rust,ignore
fn activate(&self, ...) -> Result<MyProcessor, Error> {
    Ok(MyProcessor {
        main_in: schema.audio_port_index(MAIN_INPUT).unwrap(),
        main_out: schema.audio_port_index(MAIN_OUTPUT).unwrap(),
        // DSP resources...
    })
}
```

The current callback endpoint type temporarily carries both stable key and optional dense index while old fixtures migrate. The target endpoint is dense-only.

## Effect conventions belong above neutral core

The historical core `Component::audio_ports()` default creates conventional stereo main input/output plus an optional sidechain. That was useful for the first effect/CLAP proof but is not the desired neutral core contract.

The target layering is:

```text
neutral ComponentSchema
     |
     +-- effect convenience
     +-- instrument convenience
     +-- event processor convenience
     +-- custom/multibus schema
```

A gain/compressor author should still get an ergonomic stereo-effect path, but zero-audio event processors, instruments, analyzers, hosted/dynamic components, and application graph nodes must not be modeled as unusual exceptions to an effect-shaped core.

Do not remove useful conventions; relocate them to the author-facing facade/helpers where they do not become semantic defaults.

## Buffers

`ChannelBuffer<S>` / `ProcessChannel<S>` preserve already-proved relationships:

- exact in-place;
- separate input/output;
- input-only;
- output-only.

`ProcessBufferSource<S>` lets adapters, devices, graphs, and embeddings expose arbitrary channel storage lazily without constructing callback-owned channel vectors.

`make_in_place()` may perform one bounded input -> output copy for separate buffers. Keep that simple behavior unless representative measurement justifies additional complexity.

The next process-boundary requirement is endpoint legality validation. Generic embeddings must not be able to present an inactive port, output as input, or an out-of-range channel and have product DSP receive it merely because the slice length matched.

That validation should consume an activation-resolved dense audio configuration, not rescan stable keys on every callback.

## Parameters and state

Persistent identity uses stable parameter keys. Dense `ParameterIndex` values are schema-local realtime projections and are never serialized.

Processing distinguishes:

1. durable/base parameter state;
2. automation trajectory for the block;
3. modulation;
4. effective DSP value after product policy.

The first two are implemented; explicit modulation remains a pre-v1 completion item.

Complete semantic state now derives component identity/version from `ComponentSchema`. Normal runtime save/load does not accept duplicate product identity arguments:

```rust,ignore
let bytes = runtime.encode_state(limits)?;
runtime.apply_state_bytes(&bytes, limits, migrations, validate)?;
```

Explicit `*_for_product` methods remain migration-only until deployment adapters adopt schema-owned identity.

Loads remain complete transactional replacements. Product custom fields remain semantic entries rather than arbitrary Rust-layout serialization.

## Sample representation

`Processor` and `Process<S>` remain separate deliberately. One processor lifecycle may implement:

```text
Process<f32>
Process<f64>
```

Deployment/graph/device layers advertise/use only capabilities they support and qualify. Supporting another sample representation should not require another ownership architecture.

## Activation resources and latency

Lookahead, convolution, oversampling, models, FFT plans, resamplers, scratch storage, and similar resources are created/prepared before realtime processing and owned by the active processor or another explicit activation-scoped owner.

`Processor::latency()` reports the DSP fact established by that activation. `InstanceRuntime` snapshots it. If controls require resources with a different latency, the active processor remains valid and requests replacement/restart rather than mutating the latency contract underneath a graph/host.

Do not create generic lookahead/oversampling/model wrappers merely to make the framework appear broad. Add helpers when several materially different processors demonstrate the same lifecycle/resource pattern.

## Smoothing and gestures

Explicit host/application ramps are reproduced without automatic extra smoothing. Smoothing is DSP policy; reusable ramp/smoother utilities may live in the DSP utility layer without changing automation semantics.

Editor/application-originated parameter edits need typed begin/change/end gesture semantics, observation, and echo suppression. They must never expose unrestricted mutable processor access.

## GUI/editor boundary

Chassis owns editor lifecycle/integration semantics that audio software repeatedly needs:

- parent/native-window attachment;
- resize/scaling/focus lifecycle;
- parameter observation and gestures;
- realtime telemetry subscription;
- teardown/recreation.

Chassis does not own product visual design or a GUI widget toolkit. Toolkit adapters remain optional.

## General audio software escape hatches

A general audio framework must allow explicit lower-level paths for:

- custom/multiple I/O layouts;
- event-only/zero-audio components;
- multibus and multi-output processing;
- raw MIDI/SysEx/MIDI 2 where semantic events are insufficient;
- custom offline/random-access processing;
- manual state fields/migrations;
- unusual activation resources;
- custom editor/window integration;
- format/device/platform-specific optional capabilities.

Escape hatches do not relax memory safety, realtime bounds, schema/state authority, or compatibility identity.

## Convenience API rule

Do not add derives/builders/macros merely because explicit declarations are verbose during pre-alpha refactoring.

A convenience earns promotion when:

- the underlying semantics are stable;
- it removes repeated mechanical work across different component classes;
- it does not hide timing/allocation/lifecycle behavior;
- authors can still reach the explicit lower-level model.

Plugin/effect conveniences should be excellent, but they are facades over the neutral component/runtime model rather than the model itself.

## Tooling direction

Build/release tooling should eventually own workspace/product discovery, export builds, local install, validators, identity/deployment manifests, packaging, signing, notarization, compatibility fixtures, and templates.

Keep build/release concerns out of runtime proc macros and keep application/DAW project semantics outside Chassis.
