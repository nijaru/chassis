# AGENTS.md

## Project purpose

Chassis is a convention-first Rust framework for professional audio components. Effects are the first production clients; instruments, standalone deployment, immersive layouts, and optional host/application layers are planned without making the core depend on any one of them.

Read `docs/architecture.md`, `docs/roadmap.md`, and `docs/licensing.md` before making architectural changes.

## Design rules

- Design from common audio-component/plugin requirements, not by cloning or reacting to another framework's scope.
- Prefer convention over configuration for behavior most products want; preserve explicit escape hatches.
- Do not overfit framework APIs to any one Nijaru plugin.
- Do not assume stereo in the core data model. Stereo main I/O plus optional stereo sidechain is the default effect convention only.
- Do not assume every component has audio input; instruments and event-only processors must fit the model.
- Do not assume every component runs inside a plugin host; standalone and embedded deployment must remain possible.
- Keep DAW/application product semantics outside Chassis core: timelines, arrangements, projects, media libraries, mastering revisions/QC, mixer UX, etc.
- Keep CLAP/VST3/AU/Clack/clap-wrapper/toolkit/platform types out of the product-facing core API.
- Extract optional common utilities only when their semantics are broadly reusable or proven by repeated clients.

## Realtime rules

- The audio callback must not allocate, block, perform filesystem/network I/O, or take unbounded/contended locks.
- Prefer ownership/capability APIs that make thread misuse difficult to express.
- Keep mutable processor state audio-thread-owned unless a specific lock-free/shared design is justified.
- Main-thread/editor/background services must not receive unrestricted mutable access to realtime processor state.
- Bounded queues and explicit overflow/drop policy are required for realtime communication.
- Any adapter `unsafe`/FFI code must be isolated, documented with invariants, and independently tested.
- Format-independent workspace crates deny unsafe code by default.

## Compatibility and validation

- Treat host behavior and format specifications as contracts, not suggestions.
- Preserve sample-accurate event offsets where the source format provides them.
- State and parameter IDs are persistent compatibility contracts once released.
- Test unusual block sizes, zero/short blocks where formats permit them, offline rendering, repeated activate/deactivate, editor open/close, state load/save, and automation.
- Run format-native validators in addition to unit tests once adapters exist.

## Licensing and dependencies

- Chassis is AGPL-3.0-or-later with an intended commercial dual-license path.
- Prefer dependencies that can legally ship in proprietary commercial-license builds (MIT/Apache/BSD/ISC/public-domain or similarly permissive terms).
- Do not import third-party strong-copyleft code into the framework without an explicit licensing decision.
- Do not accept substantive external code contributions until contributor/relicensing terms are established.

## Current implementation priority

1. Small, format-independent `chassis-core` contracts.
2. Conformance component/test harness.
3. CLAP adapter via Clack.
4. Parameters/state/process-context and robust validation.
5. VST3/AU projection and GUI/editor integration.
6. Real FX clients, then instruments and standalone.

Do not add roadmap features merely to make the crate tree look complete. Add a crate or abstraction when there is an executable requirement for it.
