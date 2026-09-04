# Authoring API and Convention Model

Status: explicit manual runtime/state/process APIs are implemented and still pre-alpha. `InstanceRuntime<P>` is the sole durable lifecycle/state authority.

## Goal

A normal product should mostly contain:

- parameter declarations;
- DSP state/processing;
- optional custom persistent state;
- optional editor code.

Chassis should own repeated format plumbing, state stream glue, host parameter projection, common lifecycle integration, validation, and packaging support without hiding realtime costs or compatibility identity.

## Current explicit model

```text
Component
  immutable product schema/capabilities/factory
  default effect audio-port descriptors
  activate_with_state(...) -> Processor

InstanceRuntime<P>
  durable ParameterStore + custom semantic state
  inactive/active lifecycle
  validated accepted audio configuration
  active Processor ownership
  activation-scoped latency snapshot

Processor
  exclusive active DSP/runtime-history owner
  reset
  latency established by this activation

Process<S>
  sample-representation-specific processing capability
  process(ProcessBlock<S>)
```

The useful contract is the ownership split, not the current trait spelling.

A format-independent client currently follows this shape:

```rust,ignore
let mut runtime = InstanceRuntime::new(component.parameter_descriptors())?;
runtime.activate(&component, process_config, audio_configuration)?;

// control path
runtime.parameters_mut().set("gain", ParameterValue::Float(0.5))?;

// deployment may query the stable latency for this active lifetime
let latency = runtime.active_latency();

// realtime path supplied by the deployment/runtime
runtime.process(frame_count, context, buffers)?;

runtime.deactivate()?;
```

Product code normally implements `Component`, `Processor`, and one or more `Process<S>` capabilities. Deployment code constructs `ProcessContext`/buffer views from the host or device boundary.

## Why `Processor` and `Process<S>` are separate

One processor owns one DSP-history lifecycle and may support multiple sample representations:

```text
Process<f32>
Process<f64>
```

Formats advertise only the capabilities the product actually implements and qualifies. Supporting f64 must not require a duplicate processor architecture.

`Processor::latency()` belongs to the activation rather than the sample representation. Products with lookahead, convolution, oversampling, or another delayed path establish the resulting sample count when they construct the processor. Chassis snapshots it for that active lifetime; changing required latency means a new activation rather than mutating host-visible latency from the process callback.

## Default effect convention

Without an explicit audio-port schema, ordinary effects use stable descriptors for:

```text
audio.main.in
audio.main.out
audio.sidechain
```

The default active configuration is stereo main input/output with sidechain inactive. This is an authoring convention, not a core stereo assumption.

Whole-layout semantic policy remains explicit for products that accept multiple configurations.

## Buffers

`ChannelBuffer<S>` preserves already-validated host relationships:

- exact in-place;
- separate input/output;
- input-only;
- output-only.

`ProcessBufferSource<S>` lets adapters expose arbitrary mapped channels lazily without constructing callback-owned channel vectors.

`make_in_place()` may perform a bounded input->output copy for separate buffers. Keep it unless representative measurement justifies a more complex ownership path.

Higher-level bus/port helpers should emerge from real DSP clients rather than speculative convenience APIs.

## Parameters and state

Persistent identity uses stable parameter keys. Dense `ParameterIndex` values are schema-local realtime projections and are never serialized.

Processing distinguishes base state, host automation trajectory, and effective DSP value. `ProcessBlock` exposes the canonical base store plus borrowed events; processors derive effective values without creating another persistent authority.

State is a format-independent semantic document with explicit schema versioning and migrations. Normal loads are complete transactional replacements. Product custom fields remain typed semantic entries rather than arbitrary Rust-layout serialization.

The active-save contract is one coherent completed publication generation. A save racing a process block may linearize before or after that block's endpoint publication, never across a mixed generation. Local publication tests exercise that contract; real-host active-save qualification remains a deployment gate.

## Latency and activation resources

A product that needs lookahead or another delayed algorithm should allocate/precompute the relevant resources during activation and return the corresponding `LatencySamples` from its processor.

The framework should not infer latency from buffer sizes or parameter names. The product owns the DSP fact; the runtime owns the stable activation snapshot; each deployment adapter projects it according to the format's lifecycle rules.

The first mastering-limiter client should determine whether repeated lookahead/oversampling setup deserves higher-level authoring helpers. Do not create those helpers before real product code demonstrates the common shape.

## Smoothing and gestures

Explicit host ramps are reproduced without automatic extra smoothing. Smoothing is product DSP policy unless a later framework helper proves reusable semantics.

Product/editor-originated parameter edits eventually use typed begin/change/end gesture handles with host notification and echo-suppression behavior. They must not expose unrestricted mutable processor access.

## GUI

Chassis owns editor lifecycle/host attachment and parameter/telemetry integration, not visual design.

A future generic/debug editor may be framework-provided. Production toolkit adapters remain optional; headless products do not depend on them.

## Escape hatches

Convention-first authoring still needs explicit lower-level paths for:

- custom I/O policy;
- manual parameter schema and backend/legacy ID overrides;
- raw timed events;
- custom smoothing;
- out-of-place/multibus DSP;
- custom activation resources and latency;
- custom state fields/migrations;
- custom editor/window integration;
- format-specific optional capabilities.

Escape hatches do not relax memory safety, realtime, state-authority, or identity invariants.

## Tooling direction

Build/release tooling should eventually own product discovery, export builds, local install, validators, identity manifests, packaging, signing, and notarization. Keep those concerns out of runtime proc macros.

## Promotion rule

Do not add derives/builders/macros until the conformance export and real FX clients show stable repeated declarations. A convenience earns promotion only when it removes mechanical work without hiding timing, allocation, ownership, or compatibility behavior an author needs to reason about.
