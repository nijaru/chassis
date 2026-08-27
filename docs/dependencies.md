# Dependency and License Policy

Chassis is intended to remain available under AGPL-3.0-or-later and under a separate proprietary commercial license. Dependencies therefore have two independent legal requirements:

1. compatibility with AGPL distribution; and
2. terms that also permit proprietary commercial Chassis builds.

Permissive dependencies normally satisfy both. They retain their own licenses/notices; the Chassis commercial license applies only to code for which Chassis has sufficient rights.

## Current workspace

`chassis-core` remains std-only with no third-party Rust dependencies.

The first CLAP adapter slice adds exact crates.io requirements for:

- `clack-plugin = 0.1.1`;
- `clack-extensions = 0.1.1`, with only `audio-ports` + plugin-side support enabled directly.

The crates.io index shows 0.1.1 as the latest published Clack release as of this audit. Clack's repository bumped its development workspace to 0.2.0 on 2026-08-05, but 0.2.0 is not currently published. Chassis therefore uses 0.1.1 rather than adding a git dependency solely for unreleased API changes.

Clack 0.1.1 is Edition 2024, declares Rust 1.85 as its MSRV, and is `MIT OR Apache-2.0`. Its runtime tree for this slice is intentionally small: `clack-common 0.1.1`, `clap-sys ^0.5.0`, and `bitflags ^2.11.0` in addition to the two declared Clack crates. These terms are compatible with both AGPL distribution and the intended separate proprietary Chassis license.

The repository lockfile must be regenerated and reviewed by Cargo on the next local validation pass after dependency changes. Do not hand-edit registry checksums into `Cargo.lock`.

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

## Current candidates / adopted dependencies

| Component | Role | License | Position |
| --- | --- | --- | --- |
| Chassis workspace | framework | AGPL-3.0-or-later + intended separate commercial license | current |
| Clack / `clack-plugin` 0.1.1 | low-level safe CLAP plugin boundary | MIT OR Apache-2.0 | adopted for initial adapter proof; qualification still required |
| `clack-extensions` 0.1.1 | stable CLAP extension wrappers | MIT OR Apache-2.0 | adopted narrowly for audio ports |
| `clap-sys` 0.5.x | raw CLAP ABI used by Clack | MIT OR Apache-2.0 | transitive; lock exact version during local validation |
| CLAP SDK | CLAP ABI specification | MIT | compatible |
| `clap-wrapper` | project CLAP into VST3/AU/AAX/standalone | MIT | preferred initial projection candidate; validate semantics per target |
| Steinberg VST3 SDK | VST3 SDK | MIT | compatible |
| Apple AudioUnitSDK | AUv2 SDK | Apache-2.0 | compatible |
| Iced | GUI adapter candidate | MIT | compatible; keep out of headless/core dependency graph |
| egui | alternate/debug GUI candidate | MIT OR Apache-2.0 | compatible |
| CPAL | standalone audio device I/O candidate | Apache-2.0 | compatible; standalone/device layer only |
| midir | standalone MIDI I/O candidate | MIT | compatible; standalone/device layer only |

The CLAP adapter currently uses only Clack's safe plugin/process/audio APIs. Chassis owns no raw CLAP pointer dereference in this slice. That reduces our initial unsafe surface, but does not remove the obligation to review Clack's unsafe/lifetime implementation before production qualification.

## AAX exception

`clap-wrapper` is MIT, but AAX has separate Avid SDK/PACE requirements. The AAX SDK is available under GPLv3 or Avid's commercial SDK agreement. Proprietary AAX products therefore need appropriate Avid commercial terms independently of the Chassis commercial license.

Keep AAX-specific dependencies/code/tooling isolated from format-independent crates.

## Source policy

Prefer crates.io releases for normal Rust dependencies. Git dependencies require an explicit reason and pinned revision because they weaken reproducibility/supply-chain review.

`Cargo.lock` is committed. Chassis artifacts are plugin dylibs and, later, standalone executables where reproducible builds matter, and `cargo deny` audits the dependency graph from the lockfile. Keep it current with dependency changes.

Do not add wildcard dependency versions. Avoid duplicate major/version trees when practical, but do not contort correctness or platform support merely to eliminate harmless duplication.

The first Clack requirements are exact `=0.1.1` requirements rather than a floating 0.1 range. Upgrade them deliberately after reviewing upstream changes and rerunning adapter conformance. In particular, moving to 0.2 should happen only after a 0.2 release is actually available (or there is a concrete reason to adopt a pinned git revision) and the reentrancy/API changes have been reviewed.

## Checks

`deny.toml` is the machine-readable license/advisory/source policy. The repository currently has no authoritative hosted CI, so run checks locally and report them accurately:

```text
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-features --all-targets -- -D warnings
cargo deny check
cargo machete
```

Now that third-party dependencies exist, `cargo machete` applies. Do not widen `deny.toml` merely to make a check pass. Review/document a new license/source class first.
