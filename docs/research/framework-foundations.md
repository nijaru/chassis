# Framework foundation research

Status: active research. This document records why the current implementation direction starts from CLAP/Clack while keeping backends replaceable.

## Clack

Clack is a set of low-level safe Rust wrappers for implementing CLAP plugins and hosts. Its stated goals are memory/thread safety, minimal overhead, low-level access, and serving as a foundation for higher-level libraries. It provides separate plugin, host, and extension crates and is dual MIT/Apache-2.0 licensed.

This is a strong match for Chassis because Chassis wants to own the high-level conventions while delegating the raw CLAP ABI and lifetime/threading boundary to a focused lower layer.

Clack describes itself as feature-complete but still under active development with possible breaking API changes, so Chassis should isolate the dependency behind `chassis-clap` and pin versions deliberately.

Source:

- https://github.com/prokopyl/clack

## clap-wrapper

`clap-wrapper` hosts a CLAP plugin inside other plugin/application formats. Current documented targets include VST3, AUv2, AUv3, AAX, and a simple standalone executable. The project is MIT licensed; referenced SDKs retain their own licenses.

This can let Chassis reach VST3/AU much sooner without implementing every format ABI itself. It is a deployment backend, not part of the Chassis product API.

Chassis must independently validate wrapper behavior against real hosts and format validators. A wrapper target is not considered supported merely because it builds.

Source:

- https://github.com/free-audio/clap-wrapper

## nice-plug

nice-plug is an important high-level Rust reference because it carries the NIH-plug lineage and reflects substantial community experience with typed parameters, realtime processing, background work, state, and GUI integration.

Chassis should study its API and failure history, but there is no current reason to make Chassis a fork or compatibility layer. The goal is to derive Chassis conventions from requirements and evidence rather than reproduce another framework's API.

## Truce

Truce is a useful breadth/tooling reference: it demonstrates demand for an integrated Rust experience spanning formats, GUIs, validation, packaging, screenshots, state, workers, and standalone operation.

A code review prompted by community criticism found a mixed picture. Some early complaints are stale and the project has added substantial hardening and CI, but current/recent low-level wrapper code has included concurrency, lifetime, and format-correctness concerns serious enough that Chassis should not depend on Truce as its trust boundary.

Use Truce as evidence for desired developer experience and as a source of test cases/failure modes, not as Chassis's foundation.

## Current decision

Initial direction:

```text
Chassis product-facing API
        ↓
format-independent chassis-core
        ↓
chassis-clap adapter
        ↓
Clack
        ↓
CLAP
        ↓
clap-wrapper for VST3/AU/standalone where validated
```

This is provisional at the backend layer. The product-facing architecture should survive replacing Clack or clap-wrapper if future evidence requires it.

## Validation principle

Backend selection is evidence-driven. Before calling a format production-supported, Chassis should eventually require an appropriate subset of:

- format-native validators;
- pluginval;
- repeated activate/deactivate and editor open/close torture;
- state round trips and migrations;
- sample-accurate automation/event tests;
- unusual/variable block sizes;
- offline/realtime parity where applicable;
- allocation/RT checks;
- host matrix on current macOS/Windows/Linux versions;
- sanitizer/Miri/fuzz coverage around owned unsafe/FFI code.
