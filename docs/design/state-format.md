# Chassis State Format

Status: preferred bounded semantic-state prototype implemented and qualified through `62b96cc`; **not yet a stable wire-format promise**. The runtime now owns complete parameter + custom semantic state transactionally. CHSS envelope v1 remains provisional until adapter/cross-format evidence is complete.

## Goal

Use one deterministic, bounded, format-independent semantic representation for plugin formats, standalone/embedded deployment, and presets.

Optimize for long-lived compatibility and migration safety rather than serializing Rust implementation layout.

## Semantic document

The durable contract is `StateDocument`:

```text
StateDocument
├── canonical product identity
├── product schema version
└── entries keyed by stable semantic identity
    ├── parameter/input.gain -> Float(-3.0)
    ├── parameter/mode       -> Choice("clean")
    ├── state/quality        -> Choice("high")
    └── state/custom-data    -> Bytes(...)
```

Encoding canonicalizes entries lexicographically by UTF-8 key bytes. Runtime indices, object addresses, Rust enum discriminants, and struct layout are never durable identities.

## Why not serialize Rust structs directly

A stable serializer does not make a changing Rust struct a good compatibility schema. Direct struct serialization couples product state to declaration, nesting, enum, and implementation-type changes.

Chassis instead keeps stable semantic keys and a deliberately small value set. A richer product-owned field may use bounded bytes with its own stable codec/version/migration tests.

Changing a Rust implementation type is safe when the stable semantic key and codec/migration contract remain compatible.

## Semantic value types

The current prototype supports:

- boolean;
- signed integer;
- unsigned integer;
- finite IEEE-754 `f64`;
- UTF-8 text;
- stable choice identifier;
- bounded product-owned bytes.

Parameter values persist in meaningful plain units, not host-normalized 0..1 values. NaN and infinity are rejected. `-0.0` is canonicalized to `0.0` by the current envelope.

Runtime DSP history is never serialized merely because its Rust type can be serialized.

## Prototype envelope

The current CHSS envelope is length-delimited little-endian data:

```text
header
  magic                 "CHSS"
  envelope_version      u16
  product_id_length     u16
  product_schema        u32
  entry_count           u32
  product_id_bytes      UTF-8

entry repeated entry_count times
  key_length            u16
  value_type            u8
  reserved_flags        u8
  payload_length        u32
  key_bytes             UTF-8
  payload               type-specific bytes
```

A checked-in binary v1 fixture now pins the current bytes and must decode and re-encode byte-identically. That makes accidental changes observable, but it does not by itself declare the envelope public/stable.

## Canonical encoding

For one semantic document:

- product identity encoding is exact;
- entries are sorted lexicographically by canonical UTF-8 key bytes;
- duplicate and empty keys are rejected;
- integer widths and endianness are fixed;
- ordinary floats must be finite and negative zero is canonicalized;
- booleans have one encoding for each value;
- choice identifiers obey their stable-identity restrictions;
- UTF-8 is validated;
- reserved flags must be zero;
- unknown value tags fail safely.

## Bounds and hostile input

Host/project/preset bytes are untrusted.

`StateLimits` bounds:

- total encoded bytes;
- product-ID length;
- entry count;
- key length;
- individual payload length.

Limits are checked before associated allocation/copy. Length arithmetic uses checked operations. Parsing work is bounded by validated bytes/entries.

The qualified adversarial suite additionally checks:

- every proper truncation prefix of a valid rich document is rejected;
- each configured resource limit can reject an otherwise valid document;
- extreme declared product/key/payload/count lengths fail without backing bytes;
- a bounded single-byte corruption corpus never panics;
- repeated failed truncated runtime loads leave live state unchanged.

Large media/sample resources should use a future explicit resource mechanism rather than silently inflating project state.

## Envelope versus product schema

`envelope_version` describes Chassis bytes. `product_schema` describes product semantics. They evolve independently.

Product migration is adjacent:

```text
v1 -> v2 -> v3 -> current
```

`StateMigration` and `StateDocument::migrate_to()` require one unique adjacent step at a time, preserve product identity, verify the declared output schema, and keep typed product migration failures away from live runtime state. Missing, ambiguous, non-adjacent, identity-changing, and schema-mismatched steps fail before publication.

Keep a deterministic fixture for every actually released product schema. Newer unknown schemas fail by default unless a product explicitly proves a forward-compatibility rule.

## Transactional runtime ownership

`InstanceRuntime` now owns the complete accepted semantic state:

- framework-managed parameters in `ParameterStore`;
- canonical validated non-`parameter/` `StateEntry` values.

Normal complete load is:

```text
bytes
 -> validate limits + decode StateDocument
 -> migrate product schema
 -> construct complete current parameter candidate
 -> construct canonical custom-state candidate
 -> validate framework parameter domain
 -> validate product invariants over both candidates
 -> publish parameter + custom state together
```

No live state is touched until all steps succeed. Product validation can inspect both temporary candidates, so cross-field constraints do not require partial mutation. Product rejection, migration failure, malformed input, and framework validation failure all leave both live domains unchanged.

`Component::activate_with_state()` may inspect the accepted complete semantic state during non-realtime processor preparation. Persistence authority remains in `InstanceRuntime`, not in the active processor.

Parameter-only state methods remain compatibility helpers for adapter paths that have not yet adopted complete product state; they intentionally do not replace custom state.

## Parameter state

Framework parameters encode automatically by canonical `parameter/<key>` identity:

- float -> finite `Float`;
- integer -> `Signed`;
- bool -> `Boolean`;
- choice -> stable `Choice` ID.

Current descriptor definitions validate values after migration. Intentional semantic range/meaning changes belong in product migrations.

## Custom state

Custom product fields remain semantic entries rather than unrestricted arbitrary-Rust serialization.

The complete runtime state owner retains them in canonical order and publishes them in the same accepted generation as framework parameters. Products provide validation logic for their own semantic keys and may use bounded byte values for richer stable sub-codecs.

## Adapter behavior

CLAP/VST3 stream boundaries can carry canonical Chassis bytes directly through bounded readers/writers. AU may embed the same canonical payload in its required container while keeping format-specific metadata separate.

Adapters must not create a second persistence authority for the same product semantics.

Short/partial stream reads/writes and backend errors are real boundary cases and require adapter tests rather than an assumption that state arrives in one complete call.

## Evidence through `62b96cc`

Qualified core evidence includes:

- all semantic value round trips;
- deterministic ordering and negative-zero canonicalization;
- invalid tag/flags/bool/float/choice rejection;
- duplicate/empty key rejection;
- explicit resource bounds;
- checked-in legacy v1 binary fixture with byte-identical re-encoding;
- adjacent migration and legacy-key migration fixture;
- adversarial truncation/declared-length/corruption corpus;
- failure-atomic runtime byte loads;
- parameter + custom state transactional publication;
- product validation failure atomic across both domains;
- migrated complete byte load;
- complete-state export and activation visibility.

## v1 promotion gates still open

Before declaring CHSS envelope v1 stable:

- run sustained fuzzing/property infrastructure beyond the deterministic adversarial corpus;
- qualify active state-save/load races through deployment publication semantics;
- test short/partial adapter stream reads and writes;
- perform CLAP/VST3/AU cross-format round trips with the same semantic fixture;
- retain deterministic golden fixtures for every released product schema.

The parser should remain small enough for manual review; fuzzing supplements rather than replaces reasoning.

## Dependency position

The current codec remains standard-library-only so its wire behavior and bounds are explicit. Any future codec dependency must be permissively licensed for commercial use, have a stable documented format, support strict bounds, and preserve semantic identities independently from Rust memory/type layout.
