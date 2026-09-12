# Authoring API and Convention Model

Status: explicit manual runtime/state/process APIs are implemented and pre-alpha. `ComponentSchema` + `InstanceRuntime<P>` are the semantic ownership model; direct coherent schema authority, dense process identities, owned audio/event metadata, whole-I/O policy, and schema-owned runtime state identity are implemented. Remaining work is audio-semantic closure and higher-layer authoring ergonomics, not migration from proof-era core APIs.

## Goal

A normal audio component should mostly contain:

- one coherent immutable component schema;
- DSP/runtime state and processing;
- optional custom persistent state;
- optional editor/application integration.

Chassis should own repeated audio-runtime plumbing, state lifecycle, timing/event semantics, validation, deployment integration, and tooling without hiding realtime costs or compatibility identity.

Plugins should be especially easy to author because they are a common Chassis deployment, but the component API must remain equally valid for embedded processors, graph nodes, standalone applications, hosts, and audio engines.

## Current explicit model

```text
Component
  immutable definition / processor factory
  exposes one coherent ComponentSchema
  activate_with_state(...) -> Processor

ComponentSchema
  semantic state identity/version when persistent
  owned-capable audio ports
  AudioIoPolicy
  owned-capable event ports
  parameters
  stable -> dense setup lookup

InstanceRuntime<P>
  owns one validated ComponentSchema generation
  durable ParameterStore + custom semantic state
  inactive/active lifecycle
  validated + policy-accepted audio configuration
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

The ownership split is the durable contract; exact convenience API spelling remains pre-alpha.

## Runtime construction

The format-independent path is:

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

The proof-order `InstanceRuntime::new(...)` / `new_with_event_ports(...)` constructors are removed. `from_schema(...)` and `for_component(...)` are the coherent construction paths.

## Component schema authority

`Component::schema()` is the sole component metadata authority. Audio ports, event ports, parameters, audio-I/O policy, and semantic state identity are one validated generation rather than separate trait methods that can drift.

A component schema owns semantic persistence identity separately from deployment identities:

```text
ComponentId + StateSchemaVersion
    semantic state/preset/project identity

CLAP ID / VST3 class ID / AU codes / bundle IDs
    deployment projections
```

Identity-less schemas remain valid for transient/embedded components; a deployment that persists state can require an identified schema.

The model has been exercised through direct non-plugin runtime clients and CLAP deployment with conventional effects, zero-audio event processors, instruments, multiple output buses, multiple legal layouts/sidechain, and runtime-owned dynamic metadata. These cases share the same component/runtime lifecycle.

## Stable versus dense identity

Authoring/setup identity and realtime identity are intentionally different:

```text
PortKey       -> AudioPortIndex
EventPortKey  -> EventPortIndex
ParameterKey  -> ParameterIndex
```

Stable keys are readable compatibility identities. Dense indices are immutable schema-local projections used after setup.

Processors that need named ports resolve them during activation and store the dense indices in processor state. They do not perform stable-string lookup in the callback.

```rust,ignore
fn activate(&self, config: &ActivationConfig<'_>) -> Result<MyProcessor, Error> {
    Ok(MyProcessor {
        main_in: config.schema().audio_port_index(&MAIN_INPUT).unwrap(),
        main_out: config.schema().audio_port_index(&MAIN_OUTPUT).unwrap(),
        // DSP resources...
    })
}
```

Audio callback endpoints are dense-only. Note/event callback addressing likewise uses `EventPortIndex`; stable audio/event strings remain in setup/schema metadata.

## Effect conventions belong above neutral core

Neutral core semantics do not assume a plugin or stereo effect. Conventional helpers provide the common case without changing that model.

`ComponentSchema::stereo_effect(...)` and `stereo_effect_with_state(...)` create an ordinary schema with stereo main input/output and an optional stereo sidechain, backed by an explicit `AudioIoPolicy`.

The layering is:

```text
neutral ComponentSchema
     |
     +-- conventional effect helper
     +-- future instrument/helper conveniences when repeated use earns them
     +-- custom/multibus schema
```

Do not add helper-specific lifecycle or processor traits. Add another convenience only when materially different clients repeat the same declaration pattern.

## Audio I/O and buffers

Port descriptors define what ports exist. `AudioIoPolicy` defines which complete active-port/layout combinations are supported. Runtime activation performs structural validation, policy acceptance, and stable-key -> dense-index resolution before product resources are created.

The initial policy forms are deliberately small:

- any structurally valid configuration, chosen explicitly;
- one of an enumerated set of whole configurations.

That is sufficient for current effect, instrument, sidechain/layout-switching, multi-output, and dynamically constructed fixtures. Do not add a rule language until a real component needs a configuration family that is impractical to enumerate.

`ChannelBuffer<S>` / `ProcessChannel<S>` preserve already-proved relationships:

- exact in-place;
- separate input/output;
- input-only;
- output-only.

`ProcessBufferSource<S>` lets adapters, devices, graphs, and embeddings expose arbitrary channel storage lazily without constructing callback-owned channel vectors.

`InstanceRuntime` validates dense endpoints against the activation-owned resolved configuration before product DSP, rejecting unknown/inactive ports, wrong direction, and out-of-range channels. Adapters still own raw-pointer, aliasing, null-buffer, and other backend-specific proofs before constructing safe core views.

`make_in_place()` may perform one bounded input -> output copy for separate buffers. Keep that simple behavior unless representative measurement justifies additional complexity.

## Parameters and state

Persistent identity uses stable parameter keys. Dense `ParameterIndex` values are schema-local realtime projections and are never serialized.

Processing distinguishes:

1. durable/base parameter state;
2. automation trajectory for the block;
3. modulation;
4. effective DSP value after product policy.

The first two are implemented; explicit sample-accurate modulation remains a pre-v1 completion item and must not be folded into durable state or automation merely for convenience.

Complete semantic state derives component identity/version from `ComponentSchema`. Runtime save/load has one identity authority:

```rust,ignore
let bytes = runtime.encode_state(limits)?;
runtime.apply_state_bytes(&bytes, limits, migrations, validate)?;
```

The old runtime explicit-product-ID and parameter-only compatibility paths are removed. Loads remain complete transactional replacements. Product custom fields remain semantic entries rather than arbitrary Rust-layout serialization.

Parameter formatting/value mapping still needs a clear authoring contract with round-trip/domain tests.

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

Common tail and bypass semantics remain open and should be added only when their cross-environment meaning is explicit.

## Smoothing, modulation, and gestures

Explicit host/application ramps are reproduced without automatic extra smoothing. Smoothing is DSP policy; reusable ramp/smoother utilities may live in the DSP utility layer without changing automation semantics.

Sample-accurate modulation needs a separate process-time contract from durable/base parameter state and ordinary automation.

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

The current heterogeneous schema/runtime fixtures did not expose a need for more constructors or a richer I/O-policy language. Treat that as evidence against speculative authoring abstraction until real clients provide pressure.

## Tooling direction

Build/release tooling should eventually own workspace/product discovery, export builds, local install, validators, identity/deployment manifests, packaging, signing, notarization, compatibility fixtures, and templates.

Keep build/release concerns out of runtime proc macros and keep application/DAW project semantics outside Chassis.
