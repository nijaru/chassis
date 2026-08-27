# Validation and Conformance Strategy

Status: design direction; tooling evolves with implemented adapters.

## Goal

Chassis should make framework correctness observable. A format adapter is not considered production-ready because a trivial plugin loads once in one DAW.

Validation needs to cover:

- format/API conformance;
- realtime invariants;
- state/parameter compatibility;
- lifecycle and thread ownership;
- cross-format semantic parity;
- operating-system/host behavior;
- performance regressions in framework-owned paths.

## Conformance component

Before a real product becomes the primary framework test, build a deliberately boring component whose purpose is to exercise Chassis semantics.

It should eventually support enough features to stress the framework:

- mono and stereo configurations;
- optional stereo sidechain;
- auxiliary output or second configuration when routing support lands;
- float/int/bool/enum parameters;
- timestamped automation;
- explicit latency/tail changes;
- state save/load and at least one migration fixture;
- editor open/close/resize when GUI support lands;
- realtime-to-GUI telemetry;
- bounded background task round-trip;
- note/MIDI input/output when instrument/event support lands;
- offline rendering;
- deterministic output that is easy to verify numerically.

Do not make the conformance component an example of sophisticated DSP. Its output should be intentionally simple enough that framework failures are unambiguous.

## Layered tests

### Core unit/property tests

`chassis-core` and other format-independent crates test semantic invariants without loading plugin binaries:

- IDs and configuration validation;
- parameter mappings;
- state encoding/migrations;
- event ordering;
- bounded queue behavior;
- transport calculations;
- layout negotiation.

Property-based tests are appropriate for ranges/mappings/state decoders once a dependency is selected under the project license policy.

### Adapter tests

Each adapter tests host-format translation independently:

- metadata/identity;
- ports/layouts;
- parameters and formatting;
- automation sample offsets;
- state streams;
- lifecycle sequences;
- process buffers/in-place aliasing;
- latency/tail/bypass notifications;
- editor lifetime/resize;
- note/MIDI translation.

Format-specific unsafe/FFI code receives focused tests rather than being exercised only indirectly by a full plugin.

### Binary validators

Use the native ecosystem validators where applicable:

- CLAP validator tooling;
- Steinberg VST3 validator;
- pluginval where useful for VST3/AU and host-style lifecycle stress;
- `auval` for Audio Unit;
- future AAX/PACE/Avid validation when supported.

Run strict validator configurations in CI where the operating system/tooling permits it.

### Real host matrix

Validators cannot replace real DAWs. Maintain an explicit supported/tested host matrix.

Initial desktop proof should include at least:

- REAPER on macOS/Windows/Linux where formats apply;
- Ableton Live on macOS/Windows;
- Logic Pro for Audio Unit;
- one CLAP-heavy host such as Bitwig when CLAP-specific behavior matters.

The exact supported versions are release metadata, not forever assumptions.

Automate what can be automated; keep reproducible manual scripts/fixtures for host behavior that cannot run in public CI.

## Differential cross-format testing

The same Chassis conformance component should render deterministic test cases through each exported format and compare semantic results.

Examples:

```text
same input + same state + same automation
CLAP -> reference output/state
VST3 -> compare
AU   -> compare
```

The comparison may be bit-exact for framework-only/pass-through behaviors and tolerance-based where host sample format or target API semantics necessarily differ.

Also compare:

- restored parameter values;
- reported latency/tail;
- port configurations;
- product identity fixtures;
- automation timestamps;
- note/event output where applicable.

This catches translation bugs that each format's validator may individually consider legal.

## Realtime allocation checks

Processing code owned by Chassis must be testable under a guard that detects heap allocation on the audio thread.

The guard belongs in `chassis-test`/test-only support, not as a production global allocator requirement.

Test at least:

- normal process path;
- automation-heavy blocks;
- sidechain/aux routing;
- maximum configured block size;
- telemetry queue full/overflow behavior;
- event output capacity exhaustion.

Any intentional adapter conversion scratch is allocated during activation and reused.

## Lifecycle torture

Repeatedly exercise valid and adversarial host sequences:

- create/destroy without activation;
- activate/reset/process/deactivate loops;
- state load before/after activation where legal;
- changing block size/sample rate via reactivation;
- changing I/O configuration while inactive;
- editor create/open/close/destroy loops;
- editor close racing legal host callbacks;
- plugin teardown after failed initialization;
- rapid host rescan/instance creation where feasible.

The framework should maintain synthetic host tests for these sequences even before every sequence can be reproduced in a real DAW.

## State fuzzing

Host state blobs are untrusted input. Fuzz the decoder/migration boundary with:

- arbitrary bytes;
- truncated envelopes;
- oversized declared lengths;
- duplicate keys;
- unknown fields/versions;
- invalid numeric parameter values;
- malformed enum identities;
- migration chains.

The decoder must reject invalid input without panic, unbounded allocation, or partial mutation of live product state.

## Unsafe/FFI checking

Format-independent crates deny unsafe code by default.

Adapters that necessarily use FFI should:

- minimize unsafe surface area;
- document every unsafe invariant;
- use Miri where it can model the Rust portion;
- run sanitizers/host stress for native C/C++/Objective-C wrapper paths where practical;
- fuzz parsers/translators that accept arbitrary host data;
- avoid assuming a host follows the spec when a defensive check can be done off the realtime hot path.

Concurrency primitives with subtle memory ordering should receive model/property testing (for example Loom) before use in framework-wide realtime communication.

## CI tiers

Suggested progression:

```text
Every commit/PR
├── fmt/clippy
├── unit tests: Linux/macOS/Windows
├── cargo-deny license/advisory/source policy
└── fast core property tests

Adapter PRs
├── build exported binaries
├── native validators
├── state/identity fixtures
└── differential headless tests

Nightly/release
├── fuzz corpus/regression suite
├── sanitizer/Miri jobs where supported
├── extended lifecycle torture
├── benchmarks
└── broader host matrix/manual qualification
```

## Performance baselines

Benchmark framework-owned overhead separately from product DSP:

- pass-through processing;
- parameter/event iteration;
- format translation;
- telemetry queues;
- state encode/decode off-thread;
- editor telemetry throughput where relevant.

Track throughput and allocations rather than optimizing from intuition. Chassis should not add abstraction solely because it benchmarks well on an unrealistic microcase, but regressions in common framework paths should be visible.

## Release evidence

A Chassis release intended for commercial plugin use should publish/record a concise compatibility matrix:

- operating systems/architectures;
- plugin formats;
- validator versions/results;
- tested DAWs;
- known limitations;
- relevant identity/state compatibility guarantees.

The framework should earn trust through reproducible evidence rather than claims about implementation style or development process.
