# Licensing

## Current license

Unless otherwise stated, Chassis source code is licensed under **AGPL-3.0-or-later**.

The repository currently contains the GNU Affero General Public License version 3 text in `LICENSE` and states the `-or-later` grant in project metadata/documentation.

## Intended dual-license model

The intended long-term model is:

- AGPL for open-source software that is willing to comply with the AGPL terms; and
- a separate commercial license for proprietary software that incorporates Chassis without accepting the AGPL copyleft obligations.

AGPL does not prohibit selling software. A developer can charge money for an AGPL-covered product while complying with the license. The commercial license is intended for developers and organizations that want to distribute proprietary/closed-source products incorporating Chassis.

Commercial pricing and contract terms are intentionally not defined during the pre-alpha phase.

## Why AGPL

For the expected desktop plugin use case, GPLv3 and AGPLv3 provide similar copyleft leverage when Chassis is incorporated into and distributed with a plugin. AGPL additionally covers certain modified network-service uses. The project prefers the standardized and widely understood AGPL license over a custom noncommercial/source-available license.

A custom "free unless you sell it" license would no longer be an open-source license and would add adoption and interpretation friction without a clear expected revenue advantage for Chassis.

## Dependency policy

Dual licensing only works if Chassis has the right to offer the complete framework under commercial terms.

Therefore:

- prefer MIT, Apache-2.0, BSD, ISC, public-domain, or similarly permissive dependencies;
- review every dependency that ships in or links into distributed Chassis artifacts;
- do not copy AGPL/GPL code from third parties into Chassis merely because Chassis itself is AGPL;
- isolate SDK-specific licensing constraints in the relevant adapter/tooling layer;
- document exceptions before adopting them.

Current backend candidates fit this direction: Clack is MIT/Apache-2.0 and clap-wrapper is MIT, while target SDKs retain their own terms.

## Contributions

Do not accept substantive outside code contributions until a contribution policy is in place that preserves the project's ability to provide commercial licenses.

The likely approach is a short, transparent contributor license agreement granting sufficient relicensing rights while contributors retain their copyright. This is a future governance task, not required while the code is authored solely by the project owner.

Bug reports, design discussion, and non-code feedback do not create the same relicensing issue.

## Trademark and product licensing

Framework copyright licensing is separate from plugin-format SDK agreements, trademarks, signing requirements, and distribution programs. VST3, Audio Unit, AAX, platform signing/notarization, and similar requirements must be reviewed independently in the adapters and release tooling.

This document records project intent and is not a substitute for legal advice before commercial licensing is launched.
