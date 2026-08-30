# Chassis State Format

Status: preferred prototype implemented in `chassis-core`, including adjacent product-schema migration traversal; **not a stable wire-format promise**. Freeze only after corruption/property/fuzz tests, migration fixtures, and cross-format round trips.

## Goal

Use one deterministic, bounded, format-independent semantic state representation for plugin formats, standalone/embedded deployment, and presets.

Optimize for long-lived compatibility and safe migration, not arbitrary Rust object serialization.

## Semantic document before byte codec

The durable contract is a Chassis semantic document keyed by stable product identities, regardless of the eventual byte encoder. The current core implementation provides `StateDocument`, a bounded v1 prototype encoder/decoder, and `ParameterStore` integration for the `parameter/` namespace.

Conceptually:

```text
StateDocument
├── canonical product identity
├── product schema version
└── entries sorted by stable key
    ├── parameter/input.gain -> F64(-3.0)
    ├── parameter/mode       -> Enum("clean")
    ├── state/quality        -> Enum("high")
    └── state/custom-data    -> Bytes(...)
```

Including product identity prevents a syntactically valid state/preset from another product being accepted merely because some keys overlap. The identity must match the component unless an explicit import/migration path says otherwise.

Flat namespaced keys are preferred initially over an arbitrary recursive object model. Product structure can still compose keys such as `band.low.frequency` while the parser/migration model remains small.

## Why not serialize Rust structs directly

A stable serializer does not make a changing Rust struct a good product compatibility schema. Direct struct serialization couples durable state to declaration/nesting/enum/layout decisions that are awkward to migrate by stable plugin identities.

Postcard/bincode/Serde may still be useful implementation tools later, but `#[derive(Serialize, Deserialize)]` on product runtime structs is not Chassis's persistence contract.

## Value types

Keep the semantic value set deliberately small:

- boolean;
- signed integer;
- unsigned integer when needed;
- finite IEEE-754 `f64`;
- UTF-8 string;
- stable enum/choice identifier;
- bounded bytes for an explicitly product-owned stable codec.

Parameter values persist in meaningful plain units, not host-normalized 0..1 values.

NaN/infinity are invalid ordinary persistent numeric values. A product requiring exact arbitrary float bit patterns uses an explicit byte/custom codec.

Runtime DSP history is never serialized simply because its Rust type happens to be serializable.

## Prototype envelope

A small length-delimited little-endian prototype is reasonable:

```text
header
  magic
  envelope_version      u16
  product_id_length     u16
  product_schema        u32
  entry_count           u32
  product_id_bytes      UTF-8 canonical product ID

entry repeated entry_count times
  key_length            u16
  value_type            u8
  entry_flags/reserved  u8
  payload_length        u32
  key_bytes             UTF-8
  payload               type-specific bytes
```

Exact magic, tag assignments, integer widths, and version behavior remain provisional until checked-in golden fixtures define state format v1.

If implementation experience shows a standard codec can preserve the same semantic/invariant model with less risk, adopting it before v1 is still open.

## Canonical encoding

For one semantic document, encoded bytes are deterministic:

- exact product identity encoding is specified;
- entries sorted lexicographically by canonical UTF-8 key bytes;
- duplicate keys rejected;
- integer widths/endianness fixed by the wire spec;
- finite IEEE float encoding fixed; canonicalize `-0.0` if Chassis does not assign it distinct meaning;
- boolean has exactly one encoding per value;
- UTF-8 validated;
- reserved bits/bytes must have specified values until assigned meaning.

Golden fixtures make accidental encoder changes observable.

## Bounds and hostile input

Host/project/preset bytes are untrusted input.

Decode limits cover at least:

- total bytes;
- product-ID length;
- entry count;
- key length;
- individual payload length;
- cumulative decoded allocation/work.

Do not choose unexplained universal limits merely to claim the parser is bounded. Chassis can provide a conservative conventional profile based on measured ordinary plugin state, while each component can explicitly declare larger limits when its documented state requirements justify them.

Limits are validated before allocation/copy. Length arithmetic uses checked operations. Parsing work is bounded by validated bytes/entries.

Large media/sample resources normally belong in an explicit resource mechanism rather than silently inflating host project state.

## Transactional decode/publication

Decoding creates temporary non-live state. Failure never partially mutates the instance.

Normal path:

```text
bytes
  -> validate envelope + product identity + limits
  -> decode typed StateDocument
  -> migrate product schema
  -> validate current parameter/custom domains
  -> build accepted state generation
  -> publish through the runtime's state-replacement boundary
```

This parser never runs on the audio thread.

## Envelope versus product schema

`envelope_version` describes Chassis bytes. `product_schema` describes product semantics. They evolve independently.

Product migrations conventionally run adjacent versions:

```text
v1 -> v2 -> v3 -> current
```

`chassis-core::StateMigration` and `StateDocument::migrate_to()` now enforce a
unique one-version-at-a-time chain. Each migration receives a temporary
`StateDocument`, must preserve its product identity, and must return its
declared next schema. Missing, ambiguous, non-adjacent, identity-changing,
and schema-mismatched steps fail before a caller can publish the result.
Product-defined migration errors remain typed. The runtime byte-load boundary
decodes and migrates before its complete transactional parameter replacement;
custom entries survive the document migration for a future product-state owner.

Keep at least one golden fixture from every public product schema. A newer unknown product schema fails safely by default unless the product explicitly proves a forward-compatibility rule.

The envelope decoder similarly needs an explicit compatible-version policy; never guess how to interpret a future wire version.

## Parameter state

Framework-managed parameters encode automatically by canonical parameter key and typed plain value:

- float -> finite `f64` plain value;
- integer -> integer;
- bool -> bool;
- enum -> stable variant key.

After migration, current parameter definitions validate the value. Intentional semantic range/meaning changes belong in migrations.

## Custom state

Do not make unrestricted arbitrary-Rust serialization the only extension path.

Common custom fields use supported stable semantic value types. Richer data uses a bounded bytes field with a product-owned stable codec and its own version/tests/migrations.

Changing a Rust implementation type is safe when stable key + semantic codec/migration remain compatible.

## Presets and diagnostics

Factory/user presets can use the same canonical state payload plus separate preset metadata.

Tooling may render a `StateDocument` into readable JSON/text for support, migration review, and fixture diffs. That diagnostic form is not the host wire format unless explicitly chosen later.

## Adapter behavior

CLAP/VST3 stream boundaries can carry canonical Chassis bytes directly through bounded readers/writers.

An AU wrapper can place the canonical payload inside its required state container while retaining AU-specific metadata separately. Adapters must not create a second competing serialization authority for the same product parameters.

Short/partial stream reads/writes and backend errors are real boundary cases: adapter tests must simulate them where the format permits streaming rather than assuming one complete read/write call.

## v1 promotion gates

Before declaring state format v1:

- round-trip every semantic value type;
- golden deterministic fixtures;
- wrong-product identity rejection;
- malformed/truncated header/entry tests;
- checked overflow/oversized lengths and resource-exhaustion tests;
- duplicate key / invalid UTF-8 / invalid type/bool/reserved data tests;
- non-finite numeric rejection;
- random/fuzz corpus;
- sequential migration fixtures;
- active state-save/load race tests according to the runtime consistency contract;
- CLAP/VST3/AU cross-format round trips;
- repeated decode/load failures leave live state unchanged.

Keep the parser small enough for manual review; fuzzing supplements rather than replaces reasoning.

## Dependency position

The first prototype can use only the standard library so the wire contract is explicit. Any later codec dependency must be permissively licensed for the commercial path, have a stable documented format, support strict bounds, and not make Rust memory/type layout the durable identity model.
