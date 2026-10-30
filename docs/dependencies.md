# Dependency and License Policy

Chassis is intended to remain available under AGPL-3.0-or-later and under a separate proprietary commercial license. Dependencies therefore have two independent legal requirements:

1. compatibility with AGPL distribution; and
2. terms that also permit proprietary commercial Chassis builds.

Permissive dependencies normally satisfy both. They retain their own licenses/notices; the Chassis commercial license applies only to code for which Chassis has sufficient rights.

## Current workspace

`chassis-runtime` remains std-only with no third-party Rust dependencies.

The first CLAP adapter pins Clack to one exact git revision:

```text
repository: https://github.com/prokopyl/clack
revision:   c5975f9f89f0953b00768680357985d46178078a
```

The revision identifies Clack's development 0.2.0 workspace after the plugin-side reentrancy fix and the immediately following safety/lifetime work. Host retains this exact source. Workspace Cargo patches consume common, plugin, and extensions from `vendor/clack`, copied from the same revision with narrowly reviewed nonlogging process-error, read-only output-descriptor, typed-event provenance, native-expression layout/decoding, and MIDI wrapper layout corrections. The [Clack audit](research/clack-pinned-adapter-audit.md) owns its provenance and complete source delta. Original MIT/Apache license texts are retained; no dependency versions change. `audio-ports`, `latency`, `note-ports`, `params`, `render`, `state`, and `tail` extensions are enabled for the plugin; tests additionally enable host-side support and logging observation.

### Why this git exception exists

The original audit selected the pin when registry 0.1.1 was the available release. On 2026-10-11, the crates.io API reports `clack-plugin` 0.2.0, published 2026-09-12, and the GitHub `v0.2` tag resolves to `27ca28391b57a61bad4bb5099288a7bf9247350d`, not the current Chassis pin. Registry availability has changed; source equivalence has not yet been audited. Clack issue #89 documents undefined behavior in the 0.1.1 release line when hosts call plugin APIs re-entrantly because plugin handlers were handed exclusive `&mut` access that real hosts can re-enter. The issue specifically cites Bitwig and `clap-wrapper`, which makes this material to Chassis's intended CLAP and later VST3/AU path.

The reentrancy fix changes affected plugin-side handlers to shared access where reentrancy requires it. Chassis must not downgrade to the affected 0.1.1 line. Adoption of registry 0.2.0 requires reviewing the source delta from the audited pin and rerunning adapter conformance; it is not an automatic dependency refresh.

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
| Clack `c5975f9` + local patches | low-level CLAP boundary | MIT OR Apache-2.0 | common/plugin/extensions corrections at the same pin; host unchanged; audit and validation own evidence |
| `clack-extensions` from same revision | CLAP extension wrappers | MIT OR Apache-2.0 | audio/note ports, latency, parameters, render mode, state, and host tail notification; local snapshot getter replaces unsafe live-DSP tail access |
| `clap-sys` 0.5.0 | raw CLAP ABI | MIT OR Apache-2.0 | direct adapter dependency for the shared-snapshot tail vtable; already used transitively by Clack |
| CLAP SDK | CLAP ABI specification | MIT | compatible |
| `clap-wrapper` | project CLAP into VST3/AU/AAX/standalone | MIT | preferred initial projection candidate; validate semantics per target |
| Steinberg VST3 SDK | VST3 SDK | MIT | compatible |
| Apple AudioUnitSDK | AUv2 SDK | Apache-2.0 | compatible |
| Iced | GUI adapter candidate | MIT | compatible; keep out of headless/core dependency graph |
| egui | alternate/debug GUI candidate | MIT OR Apache-2.0 | compatible |
| CPAL | standalone audio device I/O candidate | Apache-2.0 | compatible; standalone/device layer only |
| midir | standalone MIDI I/O candidate | MIT | compatible; standalone/device layer only |

Clack still owns CLAP ABI lifecycle, descriptor borrowing, and panic containment. Chassis owns the shared-snapshot tail ABI shim and a narrow unsafe sample-pointer boundary in `chassis-clap/src/raw_audio.rs`: it classifies the entire mapped alias topology before creating sample references rather than using pinned Clack's reference-first channel pairing. The pin is unchanged. Host pointer provenance/extent, initialization, metadata separation, and concurrency remain obligations; see the [audit](research/clack-pinned-adapter-audit.md). The local plugin patch bypasses synchronous reporting only for process callbacks, returning actual processing failure; arbitrary boxed errors and panics remain outside realtime qualification.

## Qualification tool source

CLAP qualification uses the exact validator revision in `.github/workflows/clap-conformance.yml`, with MIT-licensed global-modulation and native-expression generator corrections under `vendor/clap-validator`. Their source deltas, regressions, isolated build, and refresh trigger are owned by [validation](design/validation.md#validator-source-and-local-fix). This is a development tool, not a new runtime dependency or part of distributed plugin artifacts; the Chassis lockfile and Clack pin are unchanged.

## AAX exception

`clap-wrapper` is MIT, but AAX has separate Avid SDK/PACE requirements. The AAX SDK is available under GPLv3 or Avid's commercial SDK agreement. Proprietary AAX products therefore need appropriate Avid commercial terms independently of the Chassis commercial license.

Keep AAX-specific dependencies/code/tooling isolated from format-independent crates.

## Source policy

Prefer crates.io releases for normal Rust dependencies. Git dependencies require an explicit reason and full pinned revision because they weaken ordinary registry reproducibility/supply-chain review.

The current Clack pin is the only approved git-source exception. `deny.toml` allowlists that repository to retain the audited trust boundary until a replacement source passes review and qualification. The historical reason was the registry 0.1.1 reentrancy defect; registry 0.2.0 is now available, but retains the process-error reporter that required the local plugin patch. Remove the vendor overrides only when a reviewed upstream replacement preserves the realtime failure and descriptor/provenance contracts recorded in the audit.

Release availability sources (checked 2026-10-11): [crates.io 0.2.0 metadata](https://crates.io/api/v1/crates/clack-plugin/0.2.0), [GitHub release](https://github.com/prokopyl/clack/releases/tag/v0.2), and [tag reference](https://api.github.com/repos/prokopyl/clack/git/ref/tags/v0.2).

`Cargo.lock` is committed. Chassis artifacts are plugin dylibs and, later, standalone executables where reproducible builds matter, and `cargo deny` audits the dependency graph from the lockfile. Keep it current with dependency changes.

Do not add wildcard dependency versions. Avoid duplicate major/version trees when practical, but do not contort correctness or platform support merely to eliminate harmless duplication.

## Checks

`deny.toml` is the machine-readable license/advisory/source policy. CI defines the following checks. Run them locally when changing the corresponding code and report actual results; the [validation guide](design/validation.md) records evidence boundaries:

```text
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-features --all-targets --locked -- -D warnings
cargo deny check
cargo machete
```

Now that third-party dependencies exist, `cargo machete` applies. Do not widen `deny.toml` merely to make a check pass. Review/document a new license/source class first.
