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
  default_custom_state / validate_state / state_migrations
  activate(config, parameters, custom_state) -> Processor

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
  held external modulation, independent of saved state
  inactive/active lifecycle
  validated + policy-accepted audio configuration
  active Processor ownership
  activation-scoped latency and completed active tail snapshots

Processor
  exclusive active DSP/runtime-history owner
  caches activation-resolved dense identities/resources
  reset / restart request / latency / tail

Process<S>
  sample-representation-specific processing capability
  process(ProcessBlock<S>) -> Result<(), ProcessError>
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

`for_component(...)` is the only public construction path: it validates component-owned complete defaults. Schema-only and proof-order constructor families are removed.

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

CLAP authoring supplies `ClapComponent::CLAP_DESCRIPTOR: ClapDescriptor` separately from the semantic schema. Start with `ClapDescriptor::new(id, name)` and use public fields/struct update syntax to declare features, vendor, version, description, `url`, `manual_url`, and `support_url`. Optional fields default to empty, not inferred product metadata; a one-product crate can explicitly use `env!("CARGO_PKG_VERSION")`. Both f32-only and dual-precision factories preserve the full declaration. [Identity/export metadata](identity-metadata.md#clap) owns the deployment identity contract.

## Stable versus dense identity

Authoring/setup identity and realtime identity are intentionally different:

```text
AudioPortKey       -> AudioPortIndex
EventPortKey  -> EventPortIndex
ParameterKey  -> ParameterIndex
```

Stable keys are readable compatibility identities. Dense indices are immutable schema-local projections used after setup.

Processors that need named ports resolve them during activation and store the dense indices in processor state. They do not perform stable-string lookup in the callback.

```rust,ignore
fn activate(&self, config: &ActivationConfig<'_>, parameters: &ParameterStore, custom: &[StateEntry]) -> Result<MyProcessor, Error> {
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

`ChannelBuffer<S>` preserves already-proved relationships:

- exact in-place;
- separate input/output;
- input-only;
- output-only.

`ProcessBufferSource<S>` lends a stack-owned iterator over safe channel views. `ProcessBlock<'_, S>` hides the source type: use `for_each_channel` for independent channels, `try_for_each_channel` for early-stopping fallible work, or `with_channels` to retain multiple views together for stereo/multichannel DSP. No callback-owned channel vector or scratch copy is required. Match `ChannelBuffer::Separate` to borrow disjoint input/output slices simultaneously; match `InPlace` for its single mutable slice.

`InstanceRuntime` validates dense endpoints against the activation-owned resolved configuration before product DSP, rejecting unknown/inactive ports, wrong direction, and out-of-range channels. Adapters still own raw-pointer, aliasing, null-buffer, and other backend-specific proofs before constructing safe core views.

`copy_input_to_output()` may perform one bounded input -> output copy for separate buffers. Keep that simple behavior unless representative measurement justifies additional complexity.

## Parameters and state

Persistent identity uses stable parameter keys. Dense `ParameterIndex` values are schema-local realtime projections and are never serialized.

Processing distinguishes:

1. durable/base parameter state;
2. automation trajectory for the block;
3. modulation;
4. effective DSP value after product policy.

Resolve typed handles during activation with `parameters.float_parameter("gain")?`, `integer_parameter`, `boolean_parameter`, or `choice_parameter`, and store those handles in the processor. `handle.value(block.parameters())` reads the consumed base value without enum matching. `handle.cursor(block)?` follows the correctly typed trajectory; for floats it also composes opted-in global modulation with the consumed baseline and clamps to the declared domain. `FloatParameter::base_cursor` explicitly requests automation without modulation. Clone a freshly constructed cursor for each independent channel, or share one cursor while processing linked channels sample-by-sample.

Handles, like dense indices, belong to their resolving schema; they are not portable identities or cross-schema tokens. Raw indices and tagged process values remain available for adapters and dynamic routing. Declare `.with_global_modulation()?` to opt in; per-note and non-float modulation are unsupported, not silently approximated.

Complete semantic state derives component identity/version from `ComponentSchema`. Runtime save/load has one identity authority:

```rust,ignore
let bytes = runtime.encode_state(limits)?;
runtime.apply_state_bytes(&component, &bytes, limits)?;
```

`Component` supplies custom defaults, complete validation, and registered adjacent migrations; the default policy rejects custom fields. Descriptor defaults remain parameter authority. Loads are complete replacements, not merges with defaults/live state. Custom fields remain semantic entries rather than arbitrary Rust-layout serialization.

The runtime alone exports/applies complete instance state. `ParameterStore` remains a typed control-value store with no independent persistence/patch API. Use `StateDocument::component_id()` / `schema_version()` to inspect documents and `StateIdentity::schema_version()` to inspect the declared target. `StateEntry::parameter` and `parameter_key` own the reserved parameter namespace for adapters and document tooling.

Match `StateExportError` for export construction/encoding, `StateValidationError` for complete document admission, and `StateLoadError::{Decode, Migration, Validation}` for byte loads. Validation exposes identity, parameter completeness/domain, and component-policy evidence directly rather than through parameter-state wrapper layers; nested concrete failures remain available through `Error::source`.

An accepted active custom-state replacement freezes the pre-load DSP base/modulation controls and withholds trajectories until reactivation; base automation is discarded, while modulation updates only the desired external signal; desired replacement state remains saveable. Use complete loads for coupled non-realtime edits. Direct parameter edits and automation enforce descriptor domains only; activation rechecks product relationships and processors must handle legal automation safely with current resources.

Attach an immutable, value-comparable `ParameterControlPolicy` to a descriptor when default linear mapping/plain text is insufficient:

```rust,ignore
let trim = ParameterDescriptor::float("trim", "Trim", 0.0, 1.0, 0.5)?
    .with_control_policy(TrimControl { precision: 1 })?;
let plain = trim.normalized_to_value(0.5)?; // Float(0.25)
let mut text = String::new();
trim.format_value(&plain, &mut text)?; // "25.0 %"
```

[TrimControl](../../examples/clap-conformance/src/trim_control.rs) is the executable example; [parameters/state](parameters-state.md#control-mapping-and-presentation) owns validation, rounding, configuration equality, and deployment limits. Policies run off the callback and never transform stored values or automation. CLAP shares policy text, not an arbitrary host-knob normalized taper.

## Notes, MIDI, and output

Declare stable event-port keys and `EventDialect::Notes` in the schema. Resolve dense indices at setup; CLAP additionally requires explicit `ClapNotePort` IDs, one supported preferred dialect, and input/output attempt bounds. `ProcessContext::ordered_events()` exposes a source-defined mixed order only when supplied, otherwise returning `None`.

Declare `EventDialect::Midi1` for opaque three-byte MIDI and borrowed SysEx. Supply `MidiEventLimits` for count and aggregate payload bounds; CLAP declares corresponding input/output constants. A callback-local `MidiEventSource` can expose host payloads without storing borrowed slices in activation scratch. Declare `EventDialect::Ump` for `MidiMessage::Ump([u32; 4])`: preserve numeric words, including unused storage, without packet parsing or conversion. Both dialects share MIDI event/attempt capacity; UMP uses no SysEx-byte budget. Each message requires its own declared port capability. The [MIDI probe](../../examples/clap-midi-probe/src/lib.rs) demonstrates zero-audio, mixed-dialect processing and order-sensitive routing.

`ProcessBlock::try_push_note_event` and `try_push_midi_event` forward to one borrowed `EventSink`, with a shared successful-output order, bounded per-family attempts, and explicit rejection. SysEx byte attempts are charged without refund. Voice-end feedback uses the original input address, not an output port. Propagate essential failure with `?` from `Process::process`; the runtime does not complete parameter/modulation endpoints or tail metadata on error, and deployments need not resume the failed run. [Events/transport](events-transport.md) owns the detailed contract; the [note probe](../../examples/clap-note-probe/src/lib.rs) is the executable CLAP example.

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

`Processor::tail()` supplies a conservative `TailLength` independently of latency; the runtime caches completed results. Mark a unique Boolean descriptor with `.with_bypass()?` and consume stepped controls through `boolean_cursor`; product DSP owns bypass routing, latency/tail preservation, and transitions. [Runtime services](runtime-services.md) owns both contracts.

## Smoothing, modulation, and gestures

Explicit host/application ramps are reproduced without automatic extra smoothing. Smoothing is DSP policy; reusable ramp/smoother utilities may live in the DSP utility layer without changing automation semantics.

Global additive float modulation shares the ordered parameter-control stream but not durable base storage. Held amounts survive reset, reactivation, and state load, and are excluded from save. [Parameters/state](parameters-state.md#global-additive-float-modulation) owns the domain, combination, lifecycle, and custom-resource transition contract. Native note expression is a separate supported capability; it does not enable per-note parameter modulation.

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
