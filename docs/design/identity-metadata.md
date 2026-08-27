# Product Identity and Export Metadata

Status: design direction; manifest syntax/generation algorithm are not frozen.

## Goal

A product declares one canonical identity. Chassis tooling handles repetitive format metadata without making shipped identity depend forever on a particular derivation algorithm.

Changing a Rust type, crate name, directory, wrapper backend, or Chassis version must not make a host believe an existing plugin is a new product.

## Canonical identity

Use one stable reverse-domain product ID, for example:

```text
com.nijaru.invisibull
com.example.synth
```

This is the human-authored source identity and is immutable after release. Validate its grammar before generation/build.

Common metadata also includes display name, vendor, product capabilities/kind, semantic product version, description, and optional support/manual/vendor URLs.

## Stable export identity: generate once, then freeze

Format IDs should not generally be recomputed from scratch on every build once a product has an identity manifest.

Preferred workflow:

```text
canonical product metadata
        ↓
cargo chassis init/identity
        ↓
generate/validate format IDs once
        ↓
checked-in identity manifest
        ↓
all future builds consume the frozen IDs
```

Deterministic generation is useful for the initial value and reproducibility, but the **manifest becomes authoritative** for backend identities. A future Chassis release may improve its generation algorithm without changing already-created products.

Tooling can verify that the manifest still corresponds to the same canonical product/vendor and reject accidental edits after the identity is marked/released.

## Vendor identity

Vendor information is preferably configured once per workspace/company:

```text
Vendor
├── canonical vendor/domain ID
├── display name
├── URLs/support metadata
└── explicit format-specific vendor registration where required
```

Audio Unit manufacturer code is explicit because it is a registered four-character vendor identity. Chassis must not imply global uniqueness merely by deriving four characters from a domain.

## CLAP

CLAP's string plugin ID naturally uses the canonical reverse-domain ID.

Because the canonical ID itself is already frozen, no separate generated CLAP identity is normally necessary. Capability/category feature strings are derived where semantics match and explicit adapter extensions remain available.

## VST3

VST3 requires persistent globally unique class IDs/FUIDs.

For a new product, Chassis may deterministically generate candidate component/controller IDs from the canonical product ID and role namespace. The exact generation method is testable tooling behavior, not a permanent runtime dependency.

Once generated, write the resolved IDs into the identity manifest. Future builds use those exact bytes rather than relying on whatever derivation algorithm the current Chassis/clap-wrapper version happens to implement.

If an imported/legacy product already has VST3 IDs, record those explicitly instead of generating new ones.

Golden fixtures test manifest -> backend bytes/byte ordering.

## Audio Unit

AU identity contains type, subtype, and manufacturer.

- type may be derived from product capabilities when the mapping is unambiguous;
- manufacturer is explicit registered vendor configuration;
- subtype is a stable product four-character code.

`cargo-chassis` may suggest/generate a legal subtype for a new product and check collisions within the known workspace, but four characters cannot provide a global collision proof. The generated/supplied subtype is therefore immediately frozen in the identity manifest and remains user-overridable **before release**.

Identity safety matters more than saving four characters of configuration.

AUv3/macOS bundle/app-extension identifiers can be derived from the canonical product ID using documented suffixes where platform packaging requires distinct bundle identities; those too are emitted in the resolved manifest when they are compatibility-significant.

## Version

Use one semantic product version as source metadata where possible. Adapters convert it into target representations and fail configuration/build when it cannot be represented safely.

Never silently truncate or wrap version fields.

Cargo package version can be the default source for a one-product crate. Workspaces/bundles can override explicitly.

## Product capabilities, not one giant category enum

Chassis needs enough semantic metadata to choose normal defaults/host categories, such as audio effect, instrument/generator, note/MIDI processing, analyzer/utility, and combinations where formats permit them.

Prefer composable capabilities over an ever-growing mutually exclusive product-kind enum. Classification informs defaults/export metadata; it does not dictate DSP architecture.

## Configuration location

Identity/build metadata stays outside realtime DSP code. Likely sources are Cargo workspace/package metadata plus a generated Chassis identity manifest consumed by `cargo-chassis`.

Illustrative only:

```toml
[workspace.metadata.chassis.vendor]
id = "com.nijaru"
name = "Nijaru"
au-manufacturer = "NijR"

[package.metadata.chassis]
id = "com.nijaru.invisibull"
name = "Invisibull"
```

Resolved manifest concept:

```text
canonical: com.nijaru.invisibull
clap:      com.nijaru.invisibull
vst3:
  processor: <frozen 128-bit id>
  controller: <frozen id if separate>
au:
  type:         aufx
  manufacturer: NijR
  subtype:      Invi
bundle ids: ...
```

Exact file syntax is not frozen before the tooling spike.

## Release protection

Before first public product release, `cargo chassis identity` should:

- validate canonical/vendor metadata;
- generate unresolved IDs once;
- detect collisions within the workspace/manifest set;
- emit the fully resolved manifest;
- support explicit imported/legacy IDs;
- compare current metadata against frozen release identity.

A released format identity changes only through an explicit compatibility-breaking migration decision. Normal refactors/upgrades cannot regenerate it.

Hosted CI is not required for this guarantee: the identity command/fixtures are the authoritative check and can run locally or in any future automation.
