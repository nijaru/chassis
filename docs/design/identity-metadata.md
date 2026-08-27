# Product Identity and Export Metadata

Status: design direction; generated identifiers and configuration syntax are not frozen.

## Goal

A product should declare its identity once. Chassis and its adapters should derive or validate the repetitive format-specific metadata needed for CLAP, VST3, Audio Unit, standalone bundles, installers, and future formats.

Identity is a compatibility contract. Changing an internal Rust type or crate name must not accidentally make hosts believe an existing plugin is a different product.

## Canonical product identity

Use one stable reverse-domain product ID as the canonical Chassis identity, for example:

```text
com.nijaru.invisibull
com.example.synth
```

The canonical ID should be ASCII, lowercase by convention, and immutable once the product is released.

Common metadata should include:

- canonical product ID;
- product display name;
- vendor identity;
- semantic product kind/capabilities;
- version;
- description;
- product/manual/support URLs where available.

Product format adapters consume this metadata rather than asking the product to duplicate it in multiple traits/configuration files.

## Vendor identity

Vendor information is shared across products and should be configurable once per workspace/company where practical:

```text
Vendor
├── canonical vendor ID/domain
├── display name
├── website/support metadata
└── format-specific vendor identity that cannot be derived safely
```

The important current exception is Audio Unit's manufacturer code. Apple uses a four-character manufacturer identifier and expects developers to register a creator/manufacturer code. Chassis should therefore require an explicit AU manufacturer code for AU builds rather than inventing one and implying registration/uniqueness.

## CLAP

CLAP explicitly encourages a unique reverse-URI/reverse-domain string ID. Chassis can expose the canonical product ID directly as the CLAP plugin ID.

Product type/capability metadata maps to CLAP feature strings where semantics are equivalent. Format-specific feature declarations remain available for capabilities Chassis does not normalize.

## VST3

VST3 requires globally unique persistent class IDs/FUIDs and expects an existing plugin to retain the same ID across normal version updates.

The initial wrapper path can derive the VST3 component TUID deterministically from the canonical CLAP/product ID, as `clap-wrapper` already does when no explicit TUID is supplied.

Chassis should nonetheless own and freeze the derivation rule at its adapter boundary rather than relying on an undocumented/transient upstream implementation forever. Before stable release:

- specify the exact hash/UUID mapping and byte ordering;
- add golden fixture tests from product ID -> VST3 class ID;
- provide an explicit legacy/override ID path;
- never change the default mapping in a compatible adapter release.

If a VST3 implementation exposes separate processor/controller class IDs, derive them from distinct stable namespaces/roles so they can never collide.

## Audio Unit

Audio Unit identity includes a type, subtype, and manufacturer code.

Chassis can derive the AU type from the product kind where the mapping is unambiguous, for example an ordinary effect versus an instrument/music effect.

The manufacturer code is explicit vendor configuration because it is a registered vendor identity.

The product subtype is a four-character alphanumeric product code. Chassis should make the common path low-ceremony:

1. derive a deterministic candidate subtype from the canonical product ID;
2. validate it is legal for the target AU format;
3. detect collisions among products visible to the build/package workspace;
4. allow an explicit stable subtype override.

Once an AU product ships, its resolved subtype becomes a compatibility fixture and must not change just because the derivation algorithm evolves. A release manifest/golden test should pin the resolved identifiers.

If deterministic derivation proves more confusing than helpful during the AU implementation spike, requiring one explicit four-character subtype per product is an acceptable fallback. The framework should optimize for identity safety over saving four characters of configuration.

## Version

Use one semantic product version as the source of truth where possible. Adapters convert it into target-format representations and reject values that cannot be represented safely.

Do not silently truncate version components to satisfy a format-specific integer representation.

The Cargo package version may be the conventional source for a single-product crate, with an explicit product version override available for workspaces/bundles that need different packaging semantics.

## Product kind

Chassis needs a small semantic product classification used to generate conventional defaults and adapter metadata, likely including at least:

- audio effect;
- instrument/generator;
- note/MIDI effect;
- analyzer/utility where it changes host classification;
- future combinations/capabilities rather than an ever-growing mutually-exclusive category enum.

The classification should not determine DSP architecture. It informs default ports, host tags/categories, and relevant deployment capabilities.

## Configuration location

Identity/build metadata should live outside realtime/product DSP code. The likely convention is Cargo/package/workspace metadata consumed by `cargo-chassis`, with generated compile-time metadata passed to adapters.

A possible direction:

```toml
[workspace.metadata.chassis.vendor]
id = "com.nijaru"
name = "Nijaru"
au-manufacturer = "NijR"

[package.metadata.chassis]
id = "com.nijaru.invisibull"
name = "Invisibull"
kind = "effect"
```

This syntax is illustrative only. Do not freeze it before the first export/tooling spike.

Code-level overrides remain useful for generated/multi-product components, but ordinary plugins should not need to repeat build metadata in Rust.

## Identity manifest

Before the first public product release, `cargo-chassis` should be able to emit a machine-readable identity manifest containing the resolved stable IDs for every target format.

That manifest becomes a release compatibility fixture and can be checked in CI to catch accidental identity changes.

Conceptually:

```text
canonical: com.nijaru.invisibull
clap:      com.nijaru.invisibull
vst3:      <stable 128-bit ID>
au:
  type:         aufx
  manufacturer: NijR
  subtype:      <stable 4CC>
```

A pull request that changes a released identity should fail unless the compatibility break is explicitly acknowledged.

## Bundle identity

Standalone/macOS bundle identifiers should normally derive from the canonical product ID with documented suffixes only when a platform requires distinct wrapper/app-extension identities.

Packaging suffixes must not leak back into the product's canonical Chassis/CLAP identity.
