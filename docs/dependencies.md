# Dependency and License Policy

Chassis is intended to remain available under AGPL-3.0-or-later and under a separate proprietary commercial license. Dependencies therefore have two independent legal requirements:

1. compatibility with AGPL distribution; and
2. terms that also permit proprietary commercial Chassis builds.

Permissive dependencies normally satisfy both. They retain their own licenses/notices; the Chassis commercial license applies only to code for which Chassis has sufficient rights.

## Current workspace

`chassis-core` remains std-only with no third-party Rust dependencies.

The first CLAP adapter pins Clack to one exact git revision:

```text
repository: https://github.com/prokopyl/clack
revision:   c5975f9f89f0953b00768680357985d46178078a
```

The revision identifies Clack's development 0.2.0 workspace after the plugin-side reentrancy fix and the immediately following safety/lifetime work. `clack-plugin` and `clack-extensions` are consumed from that same pinned source; `audio-ports`, `latency`, `params`, `render`, and `state` extensions are enabled for the plugin. Tests additionally enable host-side support.

### Why this git exception exists

The latest crates.io release available during this audit is Clack 0.1.1, published 2026-07-29. Clack issue #89 documents undefined behavior in that release line when hosts call plugin APIs re-entrantly because plugin handlers were handed exclusive `&mut` access that real hosts can re-enter. The issue specifically cites Bitwig and `clap-wrapper`, which makes this material to Chassis's intended CLAP and later VST3/AU path.

The reentrancy fix merged on 2026-07-30, after the 0.1.1 release, and changes affected plugin-side handlers to shared access where reentrancy requires it. Chassis therefore does not use 0.1.1 merely because it is the latest registry release.

This is a narrow safety exception to the normal crates.io preference, not a general acceptance of floating git dependencies:

- the git repository is explicitly allowlisted in `deny.toml`;
- the manifest pins a full commit SHA;
- upgrades require deliberate source/audit/conformance review;
- when a suitable safety-fixed Clack release is published, prefer returning to crates.io after validating that release.

Clack's workspace is Edition 2024, declares Rust 1.85 as its MSRV for the relevant crates, and is `MIT OR Apache-2.0`. Its underlying CLAP bindings and other current transitive dependencies use permissive terms compatible with both AGPL distribution and the intended separate proprietary Chassis license. The exact resolved dependency tree must still be reviewed after Cargo updates `Cargo.lock` locally.

The repository lockfile must be regenerated and reviewed by Cargo on the next local validation pass after dependency changes. Do not hand-edit git source entries or registry checksums into `Cargo.lock`.

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
| Clack pinned revision `c5975f9` | low-level safe CLAP plugin boundary | MIT OR Apache-2.0 | adopted safety-fixed source for initial adapter proof; qualification still required |
| `clack-extensions` from same revision | CLAP extension wrappers | MIT OR Apache-2.0 | audio ports, latency, parameters, render mode, and state |
| `clap-sys` | raw CLAP ABI used by Clack | MIT OR Apache-2.0 | transitive; inspect exact lockfile resolution |
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

Prefer crates.io releases for normal Rust dependencies. Git dependencies require an explicit reason and full pinned revision because they weaken ordinary registry reproducibility/supply-chain review.

The current Clack pin is the only approved git-source exception. `deny.toml` allowlists that repository specifically because the latest published release has a known memory-safety/reentrancy problem relevant to our deployment path.

`Cargo.lock` is committed. Chassis artifacts are plugin dylibs and, later, standalone executables where reproducible builds matter, and `cargo deny` audits the dependency graph from the lockfile. Keep it current with dependency changes.

Do not add wildcard dependency versions. Avoid duplicate major/version trees when practical, but do not contort correctness or platform support merely to eliminate harmless duplication.

## Checks

`deny.toml` is the machine-readable license/advisory/source policy. CI defines the following checks. Run them locally when changing the corresponding code and report actual results; the [validation guide](design/validation.md) records evidence boundaries:

```text
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-features --all-targets -- -D warnings
cargo deny check
cargo machete
```

Now that third-party dependencies exist, `cargo machete` applies. Do not widen `deny.toml` merely to make a check pass. Review/document a new license/source class first.
