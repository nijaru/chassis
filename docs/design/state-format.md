# Chassis State Format

Status: preferred prototype direction; **not a stable wire-format promise yet**. Freeze only after implementation, migration fixtures, fuzzing, and cross-format round-trip tests.

## Goal

Use one deterministic, bounded, format-independent state representation for CLAP, VST3, Audio Unit wrappers, standalone products, embedded components, and factory/user presets.

The state format should be optimized for long-term plugin compatibility rather than arbitrary Rust object serialization.

## Why not serialize product structs directly

A generic serializer can have a stable wire format while the serialized Rust data model remains an unstable product contract.

Directly serializing a product struct tends to couple saved state to:

- field/declaration order;
- enum discriminant order;
- nesting/container representation;
- serializer configuration/version;
- Rust-oriented schema choices that are awkward to migrate by stable plugin parameter IDs.

Postcard, for example, has a documented stable wire format but is intentionally non-self-describing: encoder and decoder share an external schema. That is useful infrastructure, but using `#[derive(Serialize, Deserialize)]` on product structs would not by itself give Chassis the migration semantics we want.

Bincode similarly has a specified binary format and configurable decode limits, but directly encoding product structs still makes that struct/schema the wire contract.

Chassis can use a third-party byte codec later if it adds enough value, but the **semantic state document** must remain a Chassis contract keyed by stable product identities.

## State document

Conceptually decode host bytes into a flat semantic document:

```text
StateDocument
├── envelope version
├── product schema version
└── entries sorted by stable key
    ├── parameter/input.gain -> F64(-3.0)
    ├── parameter/mode       -> Enum("clean")
    ├── state/quality        -> Enum("high")
    └── state/custom-data    -> Bytes(...)
```

Exact key namespaces are not frozen. The important property is that stable string identity, not Rust layout/order, determines meaning.

Flat namespaced keys are preferred over an arbitrary recursive object model. Nested product concepts can already compose stable hierarchical keys such as `band.low.frequency`. This keeps decoding/migration small and predictable.

## Value types

The initial semantic value set should be deliberately small:

- boolean;
- signed integer;
- unsigned integer if real product requirements justify both integer domains;
- finite IEEE-754 `f64`;
- UTF-8 string;
- stable enum/choice identifier (semantically distinct from arbitrary text even if encoded similarly);
- bounded byte string for explicit product-owned codecs/data.

`f32` parameters serialize canonically through the numeric state representation rather than persisting host-normalized values.

NaN and infinities are invalid persistent parameter values by default. Products that genuinely need arbitrary float bit patterns should use an explicit bytes/custom codec rather than weakening all state validation.

Runtime-only DSP structures are never serialized merely because their Rust types are serializable.

## Proposed binary envelope

A simple first prototype is a length-delimited little-endian format:

```text
header
  magic                 fixed Chassis bytes
  envelope_version      u16
  product_schema        u32
  entry_count           u32

entry repeated entry_count times
  key_length            u16
  value_type            u8
  entry_flags/reserved  u8
  payload_length        u32
  key_bytes             UTF-8
  payload               type-specific bytes
```

The exact magic/version/tag assignments must be specified in a checked-in wire-format fixture before release.

Length-delimited entries allow a decoder to reject malformed sizes before interpreting data and leave room for explicitly designed future envelope evolution.

Do not add fields merely for hypothetical flexibility. The first encoder/decoder should stay small enough to audit manually.

## Canonical encoding

For a given semantic state, bytes should be deterministic.

Rules should include:

- entries encoded in lexicographic byte order by canonical UTF-8 key;
- duplicate keys rejected;
- integers use one specified width/endianness per semantic type;
- floats use one specified IEEE representation and reject non-finite values unless a field explicitly supports them;
- if `-0.0` has no semantic distinction for Chassis numeric state, canonicalize it to `+0.0`;
- booleans have one valid false encoding and one valid true encoding;
- UTF-8 is validated on decode;
- reserved bits/bytes must be zero until assigned meaning.

Golden fixture tests make any accidental encoder change visible.

## Bounds and hostile input

Host state is untrusted input even when it normally originates from the same plugin.

The decoder must enforce configurable hard bounds before allocation:

- total state bytes;
- entry count;
- key length;
- individual payload length;
- cumulative decoded allocation.

Chassis should provide a conservative normal-plugin default and allow a product to raise it explicitly for legitimate larger state. Large sample libraries/resources should generally use resource/file facilities rather than silently embedding unbounded data in a DAW project state blob.

Integer arithmetic used for lengths/offsets must be overflow-checked.

Decode into temporary non-live state. Failure never partially mutates the active product.

## Schema version and migrations

The envelope version describes the Chassis wire format. The product schema version describes the product's semantic state.

These evolve independently.

Normal load path:

```text
bytes
  -> validate Chassis envelope
  -> decode bounded typed StateDocument
  -> inspect product schema version
  -> sequential product migrations
  -> validate current parameter/custom schema
  -> publish canonical state
  -> transfer changes across the runtime boundary
```

A product schema version should be a monotonically increasing integer suitable for migration ordering. Product marketing/semantic versioning is separate metadata.

Chassis should make adjacent sequential migrations the conventional path:

```text
v1 -> v2 -> v3 -> current
```

Tests retain at least one golden state fixture from every publicly released product schema.

Loading a **newer** unknown product schema should fail safely by default rather than guessing that unknown fields can be discarded. A product can explicitly provide a compatibility policy if it has evidence that a newer schema is safely consumable.

## Parameters

Framework-managed parameter state is encoded automatically using canonical parameter IDs and meaningful plain typed values.

Examples:

- float parameter -> finite `f64` plain value;
- integer -> integer;
- bool -> bool;
- enum -> stable variant ID string.

The normal decoder validates the migrated value against the current parameter domain. A migration is the place to intentionally remap values whose historical range/meaning changed.

Host-normalized 0..1 values are not the persistent Chassis representation.

## Custom persistent fields

Products should not get an unrestricted "serialize any Rust type" attribute as the only persistence path.

Common custom fields can implement/use explicit Chassis state-value traits for the supported semantic types.

For genuinely richer data, provide a bounded bytes field with a product-owned stable codec. That codec has its own tests/migration responsibility. Chassis can offer helper codecs later when repeated use proves they are common.

A custom field's Rust type may change without breaking saved state if its stable key and codec/migration semantics remain compatible.

## Presets and debugging

Factory/user presets can use the same binary semantic state payload plus separate preset metadata where needed.

Chassis tooling should eventually be able to render a state document into a human-readable diagnostic form (for example JSON/text) without making that diagnostic representation the host wire format. This is useful for:

- inspecting presets;
- migration reviews;
- golden test diffs;
- support/debugging.

## Format adapters

CLAP and VST3 provide stream-style state boundaries, so the Chassis bytes can be written/read directly through bounded streaming adapters.

Audio Unit exposes dictionary-style `fullState`/`fullStateForDocument`. An AU adapter/wrapper can store the canonical Chassis payload inside an owned namespaced binary value while also satisfying any wrapper/platform metadata requirements. The Chassis semantic state remains identical.

Adapter state glue must not independently serialize parameter values into a second competing product state model.

## Security and correctness tests before freeze

Before this becomes `state format v1`:

- round-trip every semantic value type;
- deterministic/golden byte fixtures;
- decode malformed/truncated headers and entries;
- overflowed/oversized lengths;
- duplicate keys;
- invalid UTF-8;
- invalid bool/type/reserved values;
- non-finite floats;
- random byte fuzzing;
- migration fixtures;
- state save/load through CLAP, VST3, and AU wrappers;
- repeated load failure must leave current canonical/live state unchanged.

The parser should be small enough that fuzzing supplements, rather than substitutes for, manual review.

## Dependency position

The first prototype can be implemented in `chassis-core`/a dedicated state crate using only the standard library so the wire contract is explicit.

If a third-party codec is later adopted internally, it must:

- be under a dependency license compatible with Chassis's commercial path;
- have a stable documented wire specification;
- support strict decode bounds;
- not force product state identity to follow Rust type layout.

Postcard (MIT OR Apache-2.0) and bincode (MIT) are technically compatible candidate building blocks, but neither is needed simply to avoid writing this intentionally tiny state envelope.
