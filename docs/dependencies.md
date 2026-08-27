# Dependency and License Policy

Chassis is intended to remain available under AGPL-3.0-or-later and under a separate proprietary commercial license. Dependencies therefore have two independent legal requirements:

1. compatibility with AGPL distribution; and
2. terms that also permit proprietary commercial Chassis builds.

Permissive dependencies normally satisfy both. They retain their own licenses/notices; the Chassis commercial license applies only to code for which Chassis has sufficient rights.

## Current workspace

`chassis-core` currently has **no third-party Rust dependencies**. It is std-only and inherits the workspace AGPL-3.0-or-later license.

## Normally acceptable license classes

Subject to source/security/attribution review:

- MIT / MIT-0
- Apache-2.0
- BSD-2-Clause / BSD-3-Clause / 0BSD
- ISC
- Zlib
- BSL-1.0
- CC0-1.0 / public-domain-equivalent grants
- Unicode-3.0 where required by ecosystem dependencies

A license outside this set requires an explicit review before adoption. Third-party strong-copyleft code is not incorporated into distributed Chassis artifacts by default because it can prevent the separate proprietary licensing path even when AGPL distribution itself would be compatible.

## Engineering review is separate from license review

A permissive license does not automatically make a dependency suitable.

For every dependency, review:

- ownership/lifecycle impact;
- maintenance activity and issue quality;
- unsafe/FFI surface;
- platform support;
- build/toolchain impact;
- transitive dependencies and source provenance;
- security/advisory posture;
- whether Chassis actually needs the abstraction.

For anything reachable from `Processor::process` or another deterministic realtime path, additionally audit:

- heap allocation after activation;
- blocking/locks/syscalls;
- destructor/reclamation behavior;
- boundedness of loops/queues/work;
- atomic memory ordering and unsafe invariants;
- measurable overhead versus an owned/simple implementation.

Dependencies are acceptable at non-realtime edges much more readily than on the critical audio path. Do not reimplement a mature peripheral crate merely for purity; do own or tightly audit code that becomes part of Chassis's realtime correctness contract.

## Current candidates

| Component | Role | License | Position |
| --- | --- | --- | --- |
| Chassis workspace | framework | AGPL-3.0-or-later + intended separate commercial license | current |
| Clack / `clack-plugin` | low-level safe CLAP plugin boundary | MIT OR Apache-2.0 | preferred candidate; still requires adapter/RT audit |
| CLAP SDK | CLAP ABI | MIT | compatible |
| `clap-wrapper` | project CLAP into VST3/AU/AAX/standalone | MIT | preferred initial projection candidate; validate semantics per target |
| Steinberg VST3 SDK | VST3 SDK | MIT | compatible |
| Apple AudioUnitSDK | AUv2 SDK | Apache-2.0 | compatible |
| Iced | GUI adapter candidate | MIT | compatible; keep out of headless/core dependency graph |
| egui | alternate/debug GUI candidate | MIT OR Apache-2.0 | compatible |
| CPAL | standalone audio device I/O candidate | Apache-2.0 | compatible; standalone/device layer only |
| midir | standalone MIDI I/O candidate | MIT | compatible; standalone/device layer only |

This table records compatibility/research, not permission to add a crate without an executable requirement.

## AAX exception

`clap-wrapper` is MIT, but AAX has separate Avid SDK/PACE requirements. The AAX SDK is available under GPLv3 or Avid's commercial SDK agreement. Proprietary AAX products therefore need appropriate Avid commercial terms independently of the Chassis commercial license.

Keep AAX-specific dependencies/code/tooling isolated from format-independent crates.

## Source policy

Prefer crates.io releases for normal Rust dependencies. Git dependencies require an explicit reason and pinned revision because they weaken reproducibility/supply-chain review.

`Cargo.lock` is committed. Chassis artifacts are plugin dylibs and, later, standalone executables where reproducible builds matter, and `cargo deny` audits the dependency graph from the lockfile. Keep it current with dependency changes.

Do not add wildcard dependency versions. Avoid duplicate major/version trees when practical, but do not contort correctness or platform support merely to eliminate harmless duplication.

## Checks

`deny.toml` is the machine-readable license/advisory/source policy. The repository currently has no authoritative hosted CI, so run checks locally and report them accurately:

```text
cargo deny check
cargo machete   # once dependencies exist
```

Do not widen `deny.toml` merely to make a check pass. Review/document a new license/source class first.
