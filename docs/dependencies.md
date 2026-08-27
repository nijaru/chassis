# Dependency and License Policy

Chassis is intended to remain available under AGPL-3.0-or-later and under a separate proprietary commercial license. Dependencies therefore have two independent requirements:

1. they must be legally compatible with AGPL distribution; and
2. their own terms must permit use in proprietary commercial Chassis builds.

Permissive dependencies satisfy both requirements cleanly. Chassis does not need the right to relicense permissive third-party code as Chassis code; those dependencies retain their own licenses and notices. The commercial Chassis license applies only to code for which the Chassis project has sufficient rights.

## Current workspace

`chassis-core` currently has no third-party Rust dependencies. It is std-only and inherits the workspace AGPL-3.0-or-later license.

## Approved license classes

Dependencies under the following license families are normally acceptable, subject to normal source/security review and attribution requirements:

- MIT / MIT-0
- Apache-2.0
- BSD-2-Clause / BSD-3-Clause / 0BSD
- ISC
- Zlib
- BSL-1.0
- CC0-1.0 / public-domain-equivalent grants
- Unicode-3.0 where required by ecosystem dependencies

A new license outside this set requires explicit review before adoption. Strong-copyleft dependencies are not accepted into distributed Chassis artifacts by default because they can undermine the proprietary commercial-license path even when they are compatible with AGPL distribution.

## Current candidates

| Component | Role | License | Position |
| --- | --- | --- | --- |
| Chassis workspace | framework | AGPL-3.0-or-later + intended separate commercial license | current |
| Clack / `clack-plugin` | safe low-level CLAP plugin boundary | MIT OR Apache-2.0 | preferred candidate |
| CLAP SDK | CLAP ABI | MIT | compatible |
| `clap-wrapper` | project CLAP into VST3/AU/AAX/standalone | MIT | preferred initial projection candidate |
| Steinberg VST3 SDK | VST3 SDK | MIT | compatible |
| Apple AudioUnitSDK | AUv2 SDK | Apache-2.0 | compatible |
| Iced | GUI adapter candidate | MIT | compatible |
| egui | alternative/debug GUI adapter candidate | MIT OR Apache-2.0 | compatible |
| CPAL | standalone audio device I/O candidate | Apache-2.0 | compatible |
| midir | standalone MIDI I/O candidate | MIT | compatible |

These are candidates unless already present in `Cargo.toml`; the table is not permission to add them without architectural review.

## AAX exception

`clap-wrapper` itself remains MIT, but AAX has separate Avid SDK and PACE requirements. The AAX SDK is available under GPLv3 or Avid's commercial SDK agreement. Proprietary AAX products therefore require the appropriate Avid commercial terms independently of any Chassis license. AAX-specific code and tooling must remain isolated from format-independent Chassis core crates.

## Automated enforcement

`cargo-deny` is used to reject unreviewed dependency licenses. The allow-list is deliberately conservative; adding a new license family requires a documented review rather than silently widening policy.
