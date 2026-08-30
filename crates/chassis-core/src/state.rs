//! Bounded, deterministic semantic state documents.
//!
//! This is a pre-release wire-format prototype. It stores product identity and
//! typed values by stable key without coupling persisted bytes to Rust layout.
//! Decoding and encoding belong to control/non-realtime paths.

use core::{fmt, str::Utf8Error};
use std::{string::String, vec::Vec};

const MAGIC: &[u8; 4] = b"CHSS";
const ENVELOPE_VERSION: u16 = 1;
const RESERVED_FLAGS: u8 = 0;

/// A typed value that can be persisted in a [`StateDocument`].
#[derive(Debug, Clone, PartialEq)]
pub enum StateValue {
    /// Boolean value.
    Boolean(bool),
    /// Signed 64-bit integer.
    Signed(i64),
    /// Unsigned 64-bit integer.
    Unsigned(u64),
    /// Finite IEEE-754 value.
    Float(f64),
    /// UTF-8 text.
    Text(String),
    /// Stable choice identifier represented as UTF-8 text.
    Choice(String),
    /// Product-owned bytes with a separate stable codec contract.
    Bytes(Vec<u8>),
}

impl StateValue {
    fn encoded_type(&self) -> u8 {
        match self {
            Self::Boolean(_) => 0,
            Self::Signed(_) => 1,
            Self::Unsigned(_) => 2,
            Self::Float(_) => 3,
            Self::Text(_) => 4,
            Self::Choice(_) => 5,
            Self::Bytes(_) => 6,
        }
    }

    fn payload_len(&self) -> usize {
        match self {
            Self::Boolean(_) => 1,
            Self::Signed(_) | Self::Unsigned(_) | Self::Float(_) => 8,
            Self::Text(value) | Self::Choice(value) => value.len(),
            Self::Bytes(value) => value.len(),
        }
    }

    fn validate(&self, limits: StateLimits) -> Result<(), StateEncodeError> {
        let payload_len = self.payload_len();
        if payload_len > limits.max_payload_bytes {
            return Err(StateEncodeError::PayloadTooLarge {
                actual: payload_len,
                maximum: limits.max_payload_bytes,
            });
        }
        if let Self::Float(value) = self
            && !value.is_finite()
        {
            return Err(StateEncodeError::NonFiniteFloat);
        }
        if let Self::Choice(value) = self
            && !valid_choice_identifier(value)
        {
            return Err(StateEncodeError::InvalidChoice);
        }
        Ok(())
    }
}

fn valid_choice_identifier(value: &str) -> bool {
    !value.is_empty()
        && !value.chars().any(char::is_whitespace)
        && !value.chars().any(char::is_control)
}

/// One stable-key state entry.
#[derive(Debug, Clone, PartialEq)]
pub struct StateEntry {
    key: String,
    value: StateValue,
}

impl StateEntry {
    /// Construct an entry. Empty keys are rejected when inserted into a document.
    #[must_use]
    pub fn new(key: impl Into<String>, value: StateValue) -> Self {
        Self {
            key: key.into(),
            value,
        }
    }

    /// Return the canonical key.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Return the typed value.
    #[must_use]
    pub const fn value(&self) -> &StateValue {
        &self.value
    }
}

/// Bounds applied before state decoding or encoding allocates/copies payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateLimits {
    /// Maximum complete encoded document size.
    pub max_total_bytes: usize,
    /// Maximum UTF-8 product identity length.
    pub max_product_id_bytes: usize,
    /// Maximum number of entries.
    pub max_entries: u32,
    /// Maximum UTF-8 key length.
    pub max_key_bytes: usize,
    /// Maximum individual value payload length.
    pub max_payload_bytes: usize,
}

impl Default for StateLimits {
    fn default() -> Self {
        Self {
            max_total_bytes: 1024 * 1024,
            max_product_id_bytes: 1024,
            max_entries: 4096,
            max_key_bytes: 4096,
            max_payload_bytes: 1024 * 1024,
        }
    }
}

/// Deterministic semantic state document.
#[derive(Debug, Clone, PartialEq)]
pub struct StateDocument {
    product_id: String,
    product_schema: u32,
    entries: Vec<StateEntry>,
}

impl StateDocument {
    /// Construct an empty state document for one product schema.
    ///
    /// # Errors
    ///
    /// Returns [`StateDocumentError::EmptyProductId`] for an empty identity.
    pub fn new(
        product_id: impl Into<String>,
        product_schema: u32,
    ) -> Result<Self, StateDocumentError> {
        let product_id = product_id.into();
        if product_id.is_empty() {
            return Err(StateDocumentError::EmptyProductId);
        }
        Ok(Self {
            product_id,
            product_schema,
            entries: Vec::new(),
        })
    }

    /// Return the canonical product identity.
    #[must_use]
    pub fn product_id(&self) -> &str {
        &self.product_id
    }

    /// Return the product schema version.
    #[must_use]
    pub const fn product_schema(&self) -> u32 {
        self.product_schema
    }

    /// Return entries in document insertion/decoded order.
    #[must_use]
    pub fn entries(&self) -> &[StateEntry] {
        &self.entries
    }

    /// Insert one unique stable-key entry.
    ///
    /// # Errors
    ///
    /// Returns an error without changing the document when the key is empty or
    /// already present.
    pub fn insert(&mut self, entry: StateEntry) -> Result<(), StateDocumentError> {
        if entry.key.is_empty() {
            return Err(StateDocumentError::EmptyKey);
        }
        if let StateValue::Choice(value) = &entry.value
            && !valid_choice_identifier(value)
        {
            return Err(StateDocumentError::InvalidChoice);
        }
        if self
            .entries
            .iter()
            .any(|existing| existing.key == entry.key)
        {
            return Err(StateDocumentError::DuplicateKey(entry.key));
        }
        self.entries.push(entry);
        Ok(())
    }

    /// Encode with the conventional [`StateLimits`].
    ///
    /// # Errors
    ///
    /// Returns an error when a value, key, or the complete document violates
    /// the conventional bounds or contains a non-finite float.
    pub fn encode(&self) -> Result<Vec<u8>, StateEncodeError> {
        self.encode_with_limits(StateLimits::default())
    }

    /// Encode deterministically under explicit bounds.
    ///
    /// # Errors
    ///
    /// Returns an error when limits are not representable, an entry is invalid,
    /// or checked size calculations exceed a configured bound.
    pub fn encode_with_limits(&self, limits: StateLimits) -> Result<Vec<u8>, StateEncodeError> {
        validate_limits(limits)?;
        validate_product_id(&self.product_id, limits)?;
        if self.entries.len() > limits.max_entries as usize {
            return Err(StateEncodeError::EntryCountTooLarge {
                actual: self.entries.len(),
                maximum: limits.max_entries,
            });
        }

        let mut entries: Vec<&StateEntry> = self.entries.iter().collect();
        entries.sort_unstable_by(|left, right| left.key.as_bytes().cmp(right.key.as_bytes()));

        let mut total = HEADER_LEN
            .checked_add(self.product_id.len())
            .ok_or(StateEncodeError::SizeOverflow)?;
        for entry in &entries {
            if entry.key.len() > limits.max_key_bytes {
                return Err(StateEncodeError::KeyTooLarge {
                    actual: entry.key.len(),
                    maximum: limits.max_key_bytes,
                });
            }
            if entry.key.len() > u16::MAX as usize {
                return Err(StateEncodeError::KeyLengthUnrepresentable(entry.key.len()));
            }
            entry.value.validate(limits)?;
            let payload_len = entry.value.payload_len();
            if payload_len > u32::MAX as usize {
                return Err(StateEncodeError::PayloadLengthUnrepresentable(payload_len));
            }
            total = total
                .checked_add(ENTRY_HEADER_LEN)
                .and_then(|size| size.checked_add(entry.key.len()))
                .and_then(|size| size.checked_add(payload_len))
                .ok_or(StateEncodeError::SizeOverflow)?;
        }
        if total > limits.max_total_bytes {
            return Err(StateEncodeError::TotalSizeTooLarge {
                actual: total,
                maximum: limits.max_total_bytes,
            });
        }

        let product_id_len = u16::try_from(self.product_id.len())
            .map_err(|_| StateEncodeError::ProductIdLengthUnrepresentable(self.product_id.len()))?;
        let entry_count = u32::try_from(entries.len())
            .map_err(|_| StateEncodeError::EntryCountUnrepresentable(entries.len()))?;
        let mut encoded = Vec::with_capacity(total);
        encoded.extend_from_slice(MAGIC);
        encoded.extend_from_slice(&ENVELOPE_VERSION.to_le_bytes());
        encoded.extend_from_slice(&product_id_len.to_le_bytes());
        encoded.extend_from_slice(&self.product_schema.to_le_bytes());
        encoded.extend_from_slice(&entry_count.to_le_bytes());
        encoded.extend_from_slice(self.product_id.as_bytes());

        for entry in entries {
            let key_len = u16::try_from(entry.key.len())
                .map_err(|_| StateEncodeError::KeyLengthUnrepresentable(entry.key.len()))?;
            let payload_len = entry.value.payload_len();
            let payload_len = u32::try_from(payload_len)
                .map_err(|_| StateEncodeError::PayloadLengthUnrepresentable(payload_len))?;
            encoded.extend_from_slice(&key_len.to_le_bytes());
            encoded.push(entry.value.encoded_type());
            encoded.push(RESERVED_FLAGS);
            encoded.extend_from_slice(&payload_len.to_le_bytes());
            encoded.extend_from_slice(entry.key.as_bytes());
            encode_payload(&mut encoded, &entry.value);
        }

        Ok(encoded)
    }

    /// Decode a complete document under the conventional [`StateLimits`].
    ///
    /// # Errors
    ///
    /// Returns an error for malformed, unsupported, truncated, oversized, or
    /// non-canonical typed input.
    pub fn decode(bytes: &[u8]) -> Result<Self, StateDecodeError> {
        Self::decode_with_limits(bytes, StateLimits::default())
    }

    /// Decode a complete document after validating all lengths and tags.
    ///
    /// # Errors
    ///
    /// Returns an error when limits are invalid or the input fails envelope,
    /// UTF-8, length, tag, value, or duplicate-key validation.
    pub fn decode_with_limits(bytes: &[u8], limits: StateLimits) -> Result<Self, StateDecodeError> {
        validate_limits(limits).map_err(StateDecodeError::InvalidLimits)?;
        if bytes.len() > limits.max_total_bytes {
            return Err(StateDecodeError::TotalSizeTooLarge {
                actual: bytes.len(),
                maximum: limits.max_total_bytes,
            });
        }
        let mut reader = Reader::new(bytes);
        if reader.take(4)? != MAGIC {
            return Err(StateDecodeError::InvalidMagic);
        }
        let version = reader.read_u16()?;
        if version != ENVELOPE_VERSION {
            return Err(StateDecodeError::UnsupportedEnvelopeVersion(version));
        }
        let product_id_len = usize::from(reader.read_u16()?);
        if product_id_len > limits.max_product_id_bytes {
            return Err(StateDecodeError::ProductIdTooLarge {
                actual: product_id_len,
                maximum: limits.max_product_id_bytes,
            });
        }
        let product_schema = reader.read_u32()?;
        let entry_count = reader.read_u32()?;
        if entry_count > limits.max_entries {
            return Err(StateDecodeError::EntryCountTooLarge {
                actual: entry_count,
                maximum: limits.max_entries,
            });
        }
        let product_id = decode_utf8(reader.take(product_id_len)?)
            .map_err(StateDecodeError::InvalidProductIdUtf8)?
            .to_owned();
        if product_id.is_empty() {
            return Err(StateDecodeError::InvalidDocument(
                StateDocumentError::EmptyProductId,
            ));
        }
        let mut entries = Vec::with_capacity(entry_count as usize);

        for _ in 0..entry_count {
            let key_len = usize::from(reader.read_u16()?);
            let value_type = reader.read_u8()?;
            let flags = reader.read_u8()?;
            if flags != RESERVED_FLAGS {
                return Err(StateDecodeError::NonZeroReservedFlags(flags));
            }
            let payload_len = usize::try_from(reader.read_u32()?)
                .map_err(|_| StateDecodeError::LengthUnrepresentable)?;
            if key_len > limits.max_key_bytes {
                return Err(StateDecodeError::KeyTooLarge {
                    actual: key_len,
                    maximum: limits.max_key_bytes,
                });
            }
            if payload_len > limits.max_payload_bytes {
                return Err(StateDecodeError::PayloadTooLarge {
                    actual: payload_len,
                    maximum: limits.max_payload_bytes,
                });
            }
            let key = decode_utf8(reader.take(key_len)?)
                .map_err(StateDecodeError::InvalidKeyUtf8)?
                .to_owned();
            if key.is_empty() {
                return Err(StateDecodeError::InvalidDocument(
                    StateDocumentError::EmptyKey,
                ));
            }
            let payload = reader.take(payload_len)?;
            let value = decode_value(value_type, payload)?;
            entries.push(StateEntry::new(key, value));
        }

        if reader.remaining() != 0 {
            return Err(StateDecodeError::TrailingBytes(reader.remaining()));
        }
        entries.sort_unstable_by(|left, right| left.key.as_bytes().cmp(right.key.as_bytes()));
        if let Some(duplicate) = entries.windows(2).find(|pair| pair[0].key == pair[1].key) {
            return Err(StateDecodeError::InvalidDocument(
                StateDocumentError::DuplicateKey(duplicate[1].key.clone()),
            ));
        }
        Ok(StateDocument {
            product_id,
            product_schema,
            entries,
        })
    }

    /// Migrate this document through adjacent product schema versions.
    ///
    /// Migrations receive ownership of a temporary document and must return a
    /// document with the same product identity and the declared next schema.
    /// The chain is local to this method; live runtime state is not touched
    /// until a caller accepts the returned document.
    ///
    /// # Errors
    ///
    /// Returns [`StateMigrationError`] when the target is older than the
    /// document, a unique adjacent migration is not available, a migration
    /// produces an invalid result, or the migration itself fails.
    pub fn migrate_to<M>(
        mut self,
        target_schema: u32,
        migrations: &[&M],
    ) -> Result<Self, StateMigrationError<M::Error>>
    where
        M: StateMigration + ?Sized,
    {
        if self.product_schema > target_schema {
            return Err(StateMigrationError::SchemaTooNew {
                actual: self.product_schema,
                target: target_schema,
            });
        }

        while self.product_schema < target_schema {
            let from_schema = self.product_schema;
            let mut migration = None;
            let mut ambiguous = false;
            for candidate in migrations
                .iter()
                .copied()
                .filter(|candidate| candidate.source_schema() == from_schema)
            {
                if migration.is_some() {
                    ambiguous = true;
                } else {
                    migration = Some(candidate);
                }
            }
            if ambiguous {
                return Err(StateMigrationError::AmbiguousMigration { from: from_schema });
            }
            let Some(migration) = migration else {
                return Err(StateMigrationError::MissingMigration {
                    from: from_schema,
                    target: target_schema,
                });
            };

            let expected_to =
                from_schema
                    .checked_add(1)
                    .ok_or(StateMigrationError::InvalidStep {
                        from: from_schema,
                        to: migration.to_schema(),
                    })?;
            if migration.to_schema() != expected_to {
                return Err(StateMigrationError::InvalidStep {
                    from: from_schema,
                    to: migration.to_schema(),
                });
            }

            let product_id = self.product_id.clone();
            self = migration
                .migrate(self)
                .map_err(StateMigrationError::Failed)?;
            if self.product_id != product_id {
                return Err(StateMigrationError::ProductIdentityChanged {
                    expected: product_id,
                    actual: self.product_id.clone(),
                });
            }
            if self.product_schema != migration.to_schema() {
                return Err(StateMigrationError::OutputSchemaMismatch {
                    expected: migration.to_schema(),
                    actual: self.product_schema,
                });
            }
        }
        Ok(self)
    }
}

/// One adjacent product-schema migration owned by a product.
///
/// Implementations should preserve every entry they do not own. Migrations
/// run only on temporary, non-realtime state and may return a product-defined
/// typed error.
pub trait StateMigration {
    /// Product-defined migration failure.
    type Error;

    /// Source product schema accepted by this migration.
    fn source_schema(&self) -> u32;

    /// Destination product schema produced by this migration.
    fn to_schema(&self) -> u32;

    /// Transform one temporary semantic document.
    ///
    /// # Errors
    ///
    /// Returns the product-defined error when the document cannot be migrated.
    fn migrate(&self, document: StateDocument) -> Result<StateDocument, Self::Error>;
}

/// Failure while traversing a product schema migration chain.
#[derive(Debug)]
pub enum StateMigrationError<E> {
    /// The document uses a schema newer than the requested target.
    SchemaTooNew {
        /// Input schema version.
        actual: u32,
        /// Requested target schema version.
        target: u32,
    },
    /// No migration starts at the current schema.
    MissingMigration {
        /// Current schema version.
        from: u32,
        /// Requested target schema version.
        target: u32,
    },
    /// More than one migration starts at the current schema.
    AmbiguousMigration {
        /// Current schema version.
        from: u32,
    },
    /// A migration was not adjacent to its declared source schema.
    InvalidStep {
        /// Declared source schema version.
        from: u32,
        /// Declared destination schema version.
        to: u32,
    },
    /// A migration changed the product identity.
    ProductIdentityChanged {
        /// Original product identity.
        expected: String,
        /// Returned product identity.
        actual: String,
    },
    /// A migration returned a schema other than its declared destination.
    OutputSchemaMismatch {
        /// Declared destination schema version.
        expected: u32,
        /// Returned schema version.
        actual: u32,
    },
    /// Product-defined migration failure.
    Failed(E),
}

impl<E> fmt::Display for StateMigrationError<E>
where
    E: fmt::Display,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SchemaTooNew { actual, target } => {
                write!(
                    formatter,
                    "state schema {actual} is newer than target {target}"
                )
            }
            Self::MissingMigration { from, target } => {
                write!(
                    formatter,
                    "no state migration exists from schema {from} to {target}"
                )
            }
            Self::AmbiguousMigration { from } => {
                write!(
                    formatter,
                    "multiple state migrations start at schema {from}"
                )
            }
            Self::InvalidStep { from, to } => {
                write!(
                    formatter,
                    "state migration step {from} to {to} is not adjacent"
                )
            }
            Self::ProductIdentityChanged { expected, actual } => write!(
                formatter,
                "state migration changed product identity from {expected:?} to {actual:?}"
            ),
            Self::OutputSchemaMismatch { expected, actual } => write!(
                formatter,
                "state migration returned schema {actual}, expected {expected}"
            ),
            Self::Failed(error) => write!(formatter, "state migration failed: {error}"),
        }
    }
}

impl<E> std::error::Error for StateMigrationError<E>
where
    E: std::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Failed(error) => Some(error),
            Self::SchemaTooNew { .. }
            | Self::MissingMigration { .. }
            | Self::AmbiguousMigration { .. }
            | Self::InvalidStep { .. }
            | Self::ProductIdentityChanged { .. }
            | Self::OutputSchemaMismatch { .. } => None,
        }
    }
}

const HEADER_LEN: usize = 4 + 2 + 2 + 4 + 4;
const ENTRY_HEADER_LEN: usize = 2 + 1 + 1 + 4;

fn encode_payload(encoded: &mut Vec<u8>, value: &StateValue) {
    match value {
        StateValue::Boolean(value) => encoded.push(u8::from(*value)),
        StateValue::Signed(value) => encoded.extend_from_slice(&value.to_le_bytes()),
        StateValue::Unsigned(value) => encoded.extend_from_slice(&value.to_le_bytes()),
        StateValue::Float(value) => {
            let value = if value.to_bits() == (-0.0_f64).to_bits() {
                0.0
            } else {
                *value
            };
            encoded.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        StateValue::Text(value) | StateValue::Choice(value) => {
            encoded.extend_from_slice(value.as_bytes());
        }
        StateValue::Bytes(value) => encoded.extend_from_slice(value),
    }
}

fn decode_value(value_type: u8, payload: &[u8]) -> Result<StateValue, StateDecodeError> {
    match value_type {
        0 => {
            if payload.len() != 1 {
                return Err(StateDecodeError::InvalidPayloadLength {
                    value_type,
                    actual: payload.len(),
                    expected: 1,
                });
            }
            match payload[0] {
                0 => Ok(StateValue::Boolean(false)),
                1 => Ok(StateValue::Boolean(true)),
                value => Err(StateDecodeError::InvalidBoolean(value)),
            }
        }
        1 => Ok(StateValue::Signed(read_i64(payload, value_type)?)),
        2 => Ok(StateValue::Unsigned(read_u64(payload, value_type)?)),
        3 => {
            let bits = read_u64(payload, value_type)?;
            let value = f64::from_bits(bits);
            if !value.is_finite() {
                return Err(StateDecodeError::NonFiniteFloat);
            }
            Ok(StateValue::Float(if bits == (-0.0_f64).to_bits() {
                0.0
            } else {
                value
            }))
        }
        4 | 5 => {
            let value = decode_utf8(payload)
                .map_err(StateDecodeError::InvalidValueUtf8)?
                .to_owned();
            if value_type == 5 && !valid_choice_identifier(&value) {
                return Err(StateDecodeError::InvalidChoice);
            }
            if value_type == 4 {
                Ok(StateValue::Text(value))
            } else {
                Ok(StateValue::Choice(value))
            }
        }
        6 => Ok(StateValue::Bytes(payload.to_vec())),
        value_type => Err(StateDecodeError::UnknownValueType(value_type)),
    }
}

fn read_i64(payload: &[u8], value_type: u8) -> Result<i64, StateDecodeError> {
    let bytes = payload_array(payload, value_type)?;
    Ok(i64::from_le_bytes(bytes))
}

fn read_u64(payload: &[u8], value_type: u8) -> Result<u64, StateDecodeError> {
    let bytes = payload_array(payload, value_type)?;
    Ok(u64::from_le_bytes(bytes))
}

fn payload_array(payload: &[u8], value_type: u8) -> Result<[u8; 8], StateDecodeError> {
    payload
        .try_into()
        .map_err(|_| StateDecodeError::InvalidPayloadLength {
            value_type,
            actual: payload.len(),
            expected: 8,
        })
}

fn validate_product_id(product_id: &str, limits: StateLimits) -> Result<(), StateEncodeError> {
    if product_id.is_empty() {
        return Err(StateEncodeError::EmptyProductId);
    }
    if product_id.len() > limits.max_product_id_bytes {
        return Err(StateEncodeError::ProductIdTooLarge {
            actual: product_id.len(),
            maximum: limits.max_product_id_bytes,
        });
    }
    if product_id.len() > u16::MAX as usize {
        return Err(StateEncodeError::ProductIdLengthUnrepresentable(
            product_id.len(),
        ));
    }
    Ok(())
}

fn validate_limits(limits: StateLimits) -> Result<(), StateEncodeError> {
    if limits.max_product_id_bytes > u16::MAX as usize {
        return Err(StateEncodeError::LimitUnrepresentable {
            field: "max_product_id_bytes",
            actual: limits.max_product_id_bytes,
            maximum: u16::MAX as usize,
        });
    }
    if limits.max_key_bytes > u16::MAX as usize {
        return Err(StateEncodeError::LimitUnrepresentable {
            field: "max_key_bytes",
            actual: limits.max_key_bytes,
            maximum: u16::MAX as usize,
        });
    }
    if limits.max_payload_bytes > u32::MAX as usize {
        return Err(StateEncodeError::LimitUnrepresentable {
            field: "max_payload_bytes",
            actual: limits.max_payload_bytes,
            maximum: u32::MAX as usize,
        });
    }
    Ok(())
}

fn decode_utf8(bytes: &[u8]) -> Result<&str, Utf8Error> {
    core::str::from_utf8(bytes)
}

struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], StateDecodeError> {
        if self.bytes.len() < length {
            return Err(StateDecodeError::Truncated {
                needed: length,
                remaining: self.bytes.len(),
            });
        }
        let (taken, remaining) = self.bytes.split_at(length);
        self.bytes = remaining;
        Ok(taken)
    }

    fn read_u8(&mut self) -> Result<u8, StateDecodeError> {
        Ok(self.take(1)?[0])
    }

    fn read_u16(&mut self) -> Result<u16, StateDecodeError> {
        Ok(u16::from_le_bytes(
            self.take(2)?
                .try_into()
                .map_err(|_| StateDecodeError::LengthUnrepresentable)?,
        ))
    }

    fn read_u32(&mut self) -> Result<u32, StateDecodeError> {
        Ok(u32::from_le_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| StateDecodeError::LengthUnrepresentable)?,
        ))
    }

    fn remaining(&self) -> usize {
        self.bytes.len()
    }
}

/// Structural error while constructing a semantic document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateDocumentError {
    /// Product identity was empty.
    EmptyProductId,
    /// Entry key was empty.
    EmptyKey,
    /// Choice identity was empty or contained whitespace/control characters.
    InvalidChoice,
    /// Entry key already existed.
    DuplicateKey(String),
}

impl fmt::Display for StateDocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyProductId => formatter.write_str("state product identity must not be empty"),
            Self::EmptyKey => formatter.write_str("state entry key must not be empty"),
            Self::InvalidChoice => formatter.write_str("state choice identity is invalid"),
            Self::DuplicateKey(key) => write!(formatter, "state entry key {key:?} is duplicated"),
        }
    }
}

impl std::error::Error for StateDocumentError {}

/// Encoding failure before a state document is published as bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateEncodeError {
    /// A configured limit cannot be represented by the wire width.
    LimitUnrepresentable {
        /// Limit field name.
        field: &'static str,
        /// Supplied limit.
        actual: usize,
        /// Maximum representable value.
        maximum: usize,
    },
    /// Product identity was empty.
    EmptyProductId,
    /// Product identity exceeded its configured bound.
    ProductIdTooLarge {
        /// Actual byte length.
        actual: usize,
        /// Configured maximum byte length.
        maximum: usize,
    },
    /// Product identity length exceeded its u16 wire field.
    ProductIdLengthUnrepresentable(usize),
    /// Entry count exceeded its configured bound.
    EntryCountTooLarge {
        /// Actual entry count.
        actual: usize,
        /// Configured maximum count.
        maximum: u32,
    },
    /// Complete encoded document exceeded its configured bound.
    TotalSizeTooLarge {
        /// Actual encoded byte length.
        actual: usize,
        /// Configured maximum byte length.
        maximum: usize,
    },
    /// Entry count exceeded its u32 wire field.
    EntryCountUnrepresentable(usize),
    /// Entry key exceeded its configured bound.
    KeyTooLarge {
        /// Actual key byte length.
        actual: usize,
        /// Configured maximum key byte length.
        maximum: usize,
    },
    /// Entry key length exceeded its u16 wire field.
    KeyLengthUnrepresentable(usize),
    /// Value payload exceeded its configured bound.
    PayloadTooLarge {
        /// Actual payload byte length.
        actual: usize,
        /// Configured maximum payload byte length.
        maximum: usize,
    },
    /// Value payload length exceeded its u32 wire field.
    PayloadLengthUnrepresentable(usize),
    /// Float value was not finite.
    NonFiniteFloat,
    /// Choice identity was empty or contained whitespace/control characters.
    InvalidChoice,
    /// Encoded size arithmetic overflowed.
    SizeOverflow,
}

impl fmt::Display for StateEncodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LimitUnrepresentable {
                field,
                actual,
                maximum,
            } => write!(
                formatter,
                "state limit {field}={actual} exceeds wire maximum {maximum}"
            ),
            Self::EmptyProductId => formatter.write_str("state product identity must not be empty"),
            Self::ProductIdTooLarge { actual, maximum } => {
                write!(
                    formatter,
                    "state product identity has {actual} bytes but {maximum} are allowed"
                )
            }
            Self::ProductIdLengthUnrepresentable(length) => {
                write!(
                    formatter,
                    "state product identity length {length} cannot fit the wire field"
                )
            }
            Self::EntryCountTooLarge { actual, maximum } => {
                write!(
                    formatter,
                    "state has {actual} entries but {maximum} are allowed"
                )
            }
            Self::TotalSizeTooLarge { actual, maximum } => {
                write!(
                    formatter,
                    "state has {actual} bytes but {maximum} are allowed"
                )
            }
            Self::EntryCountUnrepresentable(count) => {
                write!(
                    formatter,
                    "state entry count {count} cannot fit the wire field"
                )
            }
            Self::KeyTooLarge { actual, maximum } => {
                write!(
                    formatter,
                    "state key has {actual} bytes but {maximum} are allowed"
                )
            }
            Self::KeyLengthUnrepresentable(length) => {
                write!(
                    formatter,
                    "state key length {length} cannot fit the wire field"
                )
            }
            Self::PayloadTooLarge { actual, maximum } => {
                write!(
                    formatter,
                    "state payload has {actual} bytes but {maximum} are allowed"
                )
            }
            Self::PayloadLengthUnrepresentable(length) => {
                write!(
                    formatter,
                    "state payload length {length} cannot fit the wire field"
                )
            }
            Self::NonFiniteFloat => formatter.write_str("state float must be finite"),
            Self::InvalidChoice => formatter.write_str("state choice identity is invalid"),
            Self::SizeOverflow => formatter.write_str("state encoded size overflowed"),
        }
    }
}

impl std::error::Error for StateEncodeError {}

/// Decode failure for untrusted state bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateDecodeError {
    /// Supplied limits were not representable by the wire format.
    InvalidLimits(StateEncodeError),
    /// Complete bytes exceeded the configured bound.
    TotalSizeTooLarge {
        /// Actual encoded byte length.
        actual: usize,
        /// Configured maximum byte length.
        maximum: usize,
    },
    /// Header or entry bytes ended early.
    Truncated {
        /// Bytes required by the next field.
        needed: usize,
        /// Bytes remaining in the input.
        remaining: usize,
    },
    /// Magic bytes did not identify a Chassis state document.
    InvalidMagic,
    /// Envelope version is unsupported.
    UnsupportedEnvelopeVersion(u16),
    /// Product identity exceeded its configured bound.
    ProductIdTooLarge {
        /// Actual byte length.
        actual: usize,
        /// Configured maximum byte length.
        maximum: usize,
    },
    /// Entry count exceeded its configured bound.
    EntryCountTooLarge {
        /// Actual entry count.
        actual: u32,
        /// Configured maximum entry count.
        maximum: u32,
    },
    /// Product identity was not UTF-8.
    InvalidProductIdUtf8(Utf8Error),
    /// Entry key was not UTF-8.
    InvalidKeyUtf8(Utf8Error),
    /// Text/choice value was not UTF-8.
    InvalidValueUtf8(Utf8Error),
    /// Reserved flags were not zero.
    NonZeroReservedFlags(u8),
    /// Entry key exceeded its configured bound.
    KeyTooLarge {
        /// Actual key byte length.
        actual: usize,
        /// Configured maximum key byte length.
        maximum: usize,
    },
    /// Value payload exceeded its configured bound.
    PayloadTooLarge {
        /// Actual payload byte length.
        actual: usize,
        /// Configured maximum payload byte length.
        maximum: usize,
    },
    /// A value tag was not defined by this envelope version.
    UnknownValueType(u8),
    /// Payload length did not match a fixed-width value type.
    InvalidPayloadLength {
        /// Value tag.
        value_type: u8,
        /// Actual payload length.
        actual: usize,
        /// Required payload length.
        expected: usize,
    },
    /// Boolean payload was not zero or one.
    InvalidBoolean(u8),
    /// Float value was not finite.
    NonFiniteFloat,
    /// Choice identity was empty or contained whitespace/control characters.
    InvalidChoice,
    /// An entry was structurally invalid.
    InvalidDocument(StateDocumentError),
    /// Bytes remained after the declared entries.
    TrailingBytes(usize),
    /// A validated wire length could not fit the host usize.
    LengthUnrepresentable,
}

impl fmt::Display for StateDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits(error) => write!(formatter, "invalid state limits: {error}"),
            Self::TotalSizeTooLarge { actual, maximum } => {
                write!(
                    formatter,
                    "state has {actual} bytes but {maximum} are allowed"
                )
            }
            Self::Truncated { needed, remaining } => {
                write!(
                    formatter,
                    "state is truncated: needs {needed} bytes, has {remaining}"
                )
            }
            Self::InvalidMagic => formatter.write_str("state magic is invalid"),
            Self::UnsupportedEnvelopeVersion(version) => {
                write!(formatter, "state envelope version {version} is unsupported")
            }
            Self::ProductIdTooLarge { actual, maximum } => {
                write!(
                    formatter,
                    "state product identity has {actual} bytes but {maximum} are allowed"
                )
            }
            Self::EntryCountTooLarge { actual, maximum } => {
                write!(
                    formatter,
                    "state has {actual} entries but {maximum} are allowed"
                )
            }
            Self::InvalidProductIdUtf8(error) => write!(
                formatter,
                "state product identity is invalid UTF-8: {error}"
            ),
            Self::InvalidKeyUtf8(error) => write!(formatter, "state key is invalid UTF-8: {error}"),
            Self::InvalidValueUtf8(error) => {
                write!(formatter, "state value is invalid UTF-8: {error}")
            }
            Self::NonZeroReservedFlags(flags) => {
                write!(formatter, "state reserved flags are {flags:#04x}")
            }
            Self::KeyTooLarge { actual, maximum } => {
                write!(
                    formatter,
                    "state key has {actual} bytes but {maximum} are allowed"
                )
            }
            Self::PayloadTooLarge { actual, maximum } => {
                write!(
                    formatter,
                    "state payload has {actual} bytes but {maximum} are allowed"
                )
            }
            Self::UnknownValueType(value_type) => {
                write!(formatter, "state value type {value_type} is unknown")
            }
            Self::InvalidPayloadLength {
                value_type,
                actual,
                expected,
            } => write!(
                formatter,
                "state value type {value_type} has {actual} payload bytes, expected {expected}"
            ),
            Self::InvalidBoolean(value) => {
                write!(formatter, "state boolean encoding {value} is invalid")
            }
            Self::NonFiniteFloat => formatter.write_str("state float must be finite"),
            Self::InvalidChoice => formatter.write_str("state choice identity is invalid"),
            Self::InvalidDocument(error) => write!(formatter, "invalid state document: {error}"),
            Self::TrailingBytes(count) => write!(formatter, "state has {count} trailing bytes"),
            Self::LengthUnrepresentable => formatter.write_str("state length is not representable"),
        }
    }
}

impl std::error::Error for StateDecodeError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::Infallible;

    struct RenameGain;

    impl StateMigration for RenameGain {
        type Error = Infallible;

        fn source_schema(&self) -> u32 {
            1
        }

        fn to_schema(&self) -> u32 {
            2
        }

        fn migrate(&self, document: StateDocument) -> Result<StateDocument, Self::Error> {
            let mut migrated = StateDocument::new(document.product_id().to_owned(), 2)
                .expect("source identity is valid");
            for entry in document.entries() {
                let key = if entry.key() == "parameter/old_gain" {
                    "parameter/gain"
                } else {
                    entry.key()
                };
                migrated
                    .insert(StateEntry::new(key, entry.value().clone()))
                    .expect("migration fixture keys are unique");
            }
            Ok(migrated)
        }
    }

    struct SkipsVersion;

    impl StateMigration for SkipsVersion {
        type Error = Infallible;

        fn source_schema(&self) -> u32 {
            1
        }

        fn to_schema(&self) -> u32 {
            3
        }

        fn migrate(&self, document: StateDocument) -> Result<StateDocument, Self::Error> {
            Ok(document)
        }
    }

    fn document_with_all_values() -> StateDocument {
        let mut document = StateDocument::new("com.example.test", 7).expect("identity is valid");
        document
            .insert(StateEntry::new("z.bytes", StateValue::Bytes(vec![1, 2, 3])))
            .expect("key is unique");
        document
            .insert(StateEntry::new("a.bool", StateValue::Boolean(true)))
            .expect("key is unique");
        document
            .insert(StateEntry::new("f.float", StateValue::Float(1.5)))
            .expect("key is unique");
        document
            .insert(StateEntry::new("i.signed", StateValue::Signed(-4)))
            .expect("key is unique");
        document
            .insert(StateEntry::new("u.unsigned", StateValue::Unsigned(9)))
            .expect("key is unique");
        document
            .insert(StateEntry::new("t.text", StateValue::Text("hello".into())))
            .expect("key is unique");
        document
            .insert(StateEntry::new(
                "c.choice",
                StateValue::Choice("clean".into()),
            ))
            .expect("key is unique");
        document
    }

    #[test]
    fn round_trip_supports_all_prototype_values() {
        let document = document_with_all_values();
        let encoded = document.encode().expect("document encodes");
        let decoded = StateDocument::decode(&encoded).expect("document decodes");
        assert_eq!(decoded, document_with_all_values_sorted());
    }

    #[test]
    fn golden_encoding_is_stable_for_a_boolean_entry() {
        let mut document = StateDocument::new("x", 2).expect("identity is valid");
        document
            .insert(StateEntry::new("a", StateValue::Boolean(true)))
            .expect("key is unique");

        assert_eq!(
            document.encode().expect("encodes"),
            vec![
                b'C', b'H', b'S', b'S', 1, 0, 1, 0, 2, 0, 0, 0, 1, 0, 0, 0, b'x', 1, 0, 0, 0, 1, 0,
                0, 0, b'a', 1,
            ]
        );
    }

    #[test]
    fn encoding_is_sorted_and_canonicalizes_negative_zero() {
        let mut first = StateDocument::new("com.example.test", 1).expect("identity is valid");
        first
            .insert(StateEntry::new("z", StateValue::Float(-0.0)))
            .expect("key is unique");
        first
            .insert(StateEntry::new("a", StateValue::Signed(2)))
            .expect("key is unique");

        let mut second = StateDocument::new("com.example.test", 1).expect("identity is valid");
        second
            .insert(StateEntry::new("a", StateValue::Signed(2)))
            .expect("key is unique");
        second
            .insert(StateEntry::new("z", StateValue::Float(0.0)))
            .expect("key is unique");

        assert_eq!(
            first.encode().expect("encodes"),
            second.encode().expect("encodes")
        );
    }

    fn document_with_all_values_sorted() -> StateDocument {
        let mut document = StateDocument::new("com.example.test", 7).expect("identity is valid");
        for entry in [
            StateEntry::new("a.bool", StateValue::Boolean(true)),
            StateEntry::new("c.choice", StateValue::Choice("clean".into())),
            StateEntry::new("f.float", StateValue::Float(1.5)),
            StateEntry::new("i.signed", StateValue::Signed(-4)),
            StateEntry::new("t.text", StateValue::Text("hello".into())),
            StateEntry::new("u.unsigned", StateValue::Unsigned(9)),
            StateEntry::new("z.bytes", StateValue::Bytes(vec![1, 2, 3])),
        ] {
            document.insert(entry).expect("key is unique");
        }
        document
    }

    #[test]
    fn rejects_duplicate_keys_without_partial_insertion() {
        let mut document = StateDocument::new("com.example.test", 1).expect("identity is valid");
        document
            .insert(StateEntry::new("gain", StateValue::Float(1.0)))
            .expect("first key is unique");
        assert_eq!(
            document.insert(StateEntry::new("gain", StateValue::Float(2.0))),
            Err(StateDocumentError::DuplicateKey("gain".into()))
        );
        assert_eq!(document.entries().len(), 1);
    }

    #[test]
    fn rejects_empty_keys_during_decode() {
        let encoded = vec![
            b'C', b'H', b'S', b'S', 1, 0, 1, 0, 2, 0, 0, 0, 1, 0, 0, 0, b'x', 0, 0, 0, 0, 1, 0, 0,
            0, 1,
        ];
        assert_eq!(
            StateDocument::decode(&encoded),
            Err(StateDecodeError::InvalidDocument(
                StateDocumentError::EmptyKey
            ))
        );
    }

    #[test]
    fn rejects_truncated_and_trailing_documents() {
        let encoded = document_with_all_values().encode().expect("encodes");
        assert!(matches!(
            StateDocument::decode(&encoded[..encoded.len() - 1]),
            Err(StateDecodeError::Truncated { .. })
        ));
        let mut trailing = encoded;
        trailing.push(0);
        assert_eq!(
            StateDocument::decode(&trailing),
            Err(StateDecodeError::TrailingBytes(1))
        );
    }

    #[test]
    fn rejects_invalid_tags_flags_booleans_and_floats() {
        let mut invalid_tag = document_with_all_values().encode().expect("encodes");
        let first_entry = HEADER_LEN + "com.example.test".len();
        invalid_tag[first_entry + 2] = 99;
        assert_eq!(
            StateDocument::decode(&invalid_tag),
            Err(StateDecodeError::UnknownValueType(99))
        );

        let mut invalid_flags = document_with_all_values().encode().expect("encodes");
        invalid_flags[first_entry + 3] = 1;
        assert_eq!(
            StateDocument::decode(&invalid_flags),
            Err(StateDecodeError::NonZeroReservedFlags(1))
        );

        let mut invalid_boolean = document_with_all_values().encode().expect("encodes");
        invalid_boolean[first_entry + 8 + "a.bool".len()] = 2;
        assert_eq!(
            StateDocument::decode(&invalid_boolean),
            Err(StateDecodeError::InvalidBoolean(2))
        );

        let mut invalid_float_bytes =
            StateDocument::new("com.example.test", 1).expect("identity is valid");
        invalid_float_bytes
            .insert(StateEntry::new("value", StateValue::Float(1.0)))
            .expect("key is unique");
        let mut invalid_float_bytes = invalid_float_bytes.encode().expect("encodes");
        let float_payload =
            HEADER_LEN + "com.example.test".len() + ENTRY_HEADER_LEN + "value".len();
        invalid_float_bytes[float_payload..float_payload + 8]
            .copy_from_slice(&f64::INFINITY.to_bits().to_le_bytes());
        assert_eq!(
            StateDocument::decode(&invalid_float_bytes),
            Err(StateDecodeError::NonFiniteFloat)
        );

        let mut invalid_choice =
            StateDocument::new("com.example.test", 1).expect("identity is valid");
        assert_eq!(
            invalid_choice.insert(StateEntry::new(
                "mode",
                StateValue::Choice("not valid".into()),
            )),
            Err(StateDocumentError::InvalidChoice)
        );

        let mut invalid_choice_bytes =
            StateDocument::new("com.example.test", 1).expect("identity is valid");
        invalid_choice_bytes
            .insert(StateEntry::new("mode", StateValue::Choice("clean".into())))
            .expect("key is unique");
        let mut invalid_choice_bytes = invalid_choice_bytes.encode().expect("encodes");
        let choice_payload =
            HEADER_LEN + "com.example.test".len() + ENTRY_HEADER_LEN + "mode".len();
        invalid_choice_bytes[choice_payload..choice_payload + 5].copy_from_slice(b"bad x");
        assert_eq!(
            StateDocument::decode(&invalid_choice_bytes),
            Err(StateDecodeError::InvalidChoice)
        );

        let mut invalid_float =
            StateDocument::new("com.example.test", 1).expect("identity is valid");
        invalid_float
            .insert(StateEntry::new("value", StateValue::Float(f64::NAN)))
            .expect("key is unique");
        assert_eq!(
            invalid_float.encode(),
            Err(StateEncodeError::NonFiniteFloat)
        );
    }

    #[test]
    fn migrates_adjacent_schema_and_preserves_unowned_entries() {
        let mut document = StateDocument::new("com.example.test", 1).expect("identity is valid");
        document
            .insert(StateEntry::new(
                "parameter/old_gain",
                StateValue::Float(0.5),
            ))
            .expect("key is unique");
        document
            .insert(StateEntry::new(
                "state/custom",
                StateValue::Bytes(vec![1, 2, 3]),
            ))
            .expect("key is unique");

        let migration = RenameGain;
        let migrated = document
            .migrate_to(2, &[&migration])
            .expect("adjacent migration succeeds");
        assert_eq!(migrated.product_schema(), 2);
        assert_eq!(migrated.product_id(), "com.example.test");
        assert_eq!(
            migrated.entries(),
            &[
                StateEntry::new("parameter/gain", StateValue::Float(0.5)),
                StateEntry::new("state/custom", StateValue::Bytes(vec![1, 2, 3])),
            ]
        );
    }

    #[test]
    fn migration_chain_rejects_missing_or_non_adjacent_steps() {
        let document = StateDocument::new("com.example.test", 1).expect("identity is valid");
        let migration = RenameGain;
        assert!(matches!(
            document.clone().migrate_to(3, &[&migration]),
            Err(StateMigrationError::MissingMigration { from: 2, target: 3 })
        ));
        assert!(matches!(
            document.migrate_to(0, &[&migration]),
            Err(StateMigrationError::SchemaTooNew {
                actual: 1,
                target: 0
            })
        ));

        let document = StateDocument::new("com.example.test", 1).expect("identity is valid");
        assert!(matches!(
            document.migrate_to(3, &[&SkipsVersion]),
            Err(StateMigrationError::InvalidStep { from: 1, to: 3 })
        ));
    }

    #[test]
    fn enforces_explicit_bounds_before_accepting_payloads() {
        let mut document = StateDocument::new("com.example.test", 1).expect("identity is valid");
        document
            .insert(StateEntry::new("value", StateValue::Bytes(vec![1, 2, 3])))
            .expect("key is unique");
        let limits = StateLimits {
            max_payload_bytes: 2,
            ..StateLimits::default()
        };
        assert!(matches!(
            document.encode_with_limits(limits),
            Err(StateEncodeError::PayloadTooLarge { .. })
        ));
    }
}
