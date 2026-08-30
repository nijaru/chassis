//! Adversarial conformance tests for bounded state decoding and publication.

use std::convert::Infallible;

use chassis_core::{
    parameters::{ParameterDescriptor, ParameterValue},
    runtime::{InstanceRuntime, Processor},
    state::{StateDocument, StateEntry, StateLimits, StateMigration, StateValue},
};

const PRODUCT_ID: &str = "com.example.adversarial";

fn rich_document() -> StateDocument {
    let mut document = StateDocument::new(PRODUCT_ID, 7).expect("product identity is valid");
    for entry in [
        StateEntry::new("state/boolean", StateValue::Boolean(true)),
        StateEntry::new("state/bytes", StateValue::Bytes(vec![1, 2, 3, 4, 5])),
        StateEntry::new("state/choice", StateValue::Choice("high".into())),
        StateEntry::new("state/float", StateValue::Float(0.25)),
        StateEntry::new("state/signed", StateValue::Signed(-42)),
        StateEntry::new("state/text", StateValue::Text("hello".into())),
        StateEntry::new("state/unsigned", StateValue::Unsigned(42)),
    ] {
        document.insert(entry).expect("fixture key is unique");
    }
    document
}

#[test]
fn every_proper_encoded_prefix_is_rejected() {
    let encoded = rich_document().encode().expect("fixture encodes");

    for length in 0..encoded.len() {
        assert!(
            StateDocument::decode(&encoded[..length]).is_err(),
            "decoder accepted truncated prefix of {length} bytes"
        );
    }

    assert_eq!(
        StateDocument::decode(&encoded).expect("complete fixture decodes"),
        rich_document()
    );
}

#[test]
fn configured_resource_limits_reject_the_same_valid_document() {
    let encoded = rich_document().encode().expect("fixture encodes");
    let defaults = StateLimits::default();

    let constrained = [
        StateLimits {
            max_total_bytes: encoded.len() - 1,
            ..defaults
        },
        StateLimits {
            max_product_id_bytes: PRODUCT_ID.len() - 1,
            ..defaults
        },
        StateLimits {
            max_entries: 0,
            ..defaults
        },
        StateLimits {
            max_key_bytes: 4,
            ..defaults
        },
        StateLimits {
            max_payload_bytes: 2,
            ..defaults
        },
        StateLimits {
            max_key_bytes: usize::from(u16::MAX) + 1,
            ..defaults
        },
    ];

    for limits in constrained {
        assert!(StateDocument::decode_with_limits(&encoded, limits).is_err());
    }
}

#[test]
fn extreme_declared_lengths_are_rejected_without_backing_bytes() {
    let encoded = rich_document().encode().expect("fixture encodes");

    let mut product_length = encoded.clone();
    product_length[6..8].copy_from_slice(&u16::MAX.to_le_bytes());
    assert!(StateDocument::decode(&product_length).is_err());

    let mut entry_count = encoded.clone();
    entry_count[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(StateDocument::decode(&entry_count).is_err());

    let first_entry = 16 + PRODUCT_ID.len();
    let mut key_length = encoded.clone();
    key_length[first_entry..first_entry + 2].copy_from_slice(&u16::MAX.to_le_bytes());
    assert!(StateDocument::decode(&key_length).is_err());

    let mut payload_length = encoded.clone();
    payload_length[first_entry + 4..first_entry + 8].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(StateDocument::decode(&payload_length).is_err());
}

#[test]
fn bounded_single_byte_corruption_corpus_never_panics() {
    let encoded = rich_document().encode().expect("fixture encodes");

    for index in 0..encoded.len() {
        for mask in [0x01_u8, 0x80, 0xff] {
            let mut corrupted = encoded.clone();
            corrupted[index] ^= mask;
            let _decode_result = StateDocument::decode(&corrupted);
        }
    }
}

struct NoopProcessor;

impl Processor for NoopProcessor {}

struct NeverMigration;

impl StateMigration for NeverMigration {
    type Error = Infallible;

    fn source_schema(&self) -> u32 {
        u32::MAX - 1
    }

    fn to_schema(&self) -> u32 {
        u32::MAX
    }

    fn migrate(&self, document: StateDocument) -> Result<StateDocument, Self::Error> {
        Ok(document)
    }
}

#[test]
fn every_truncated_runtime_load_is_failure_atomic() {
    let descriptor =
        ParameterDescriptor::float("gain", "Gain", 0.0, 2.0, 1.0).expect("test parameter is valid");
    let mut runtime =
        InstanceRuntime::<NoopProcessor>::new(&[descriptor]).expect("runtime schema is valid");
    runtime
        .parameters_mut()
        .set("gain", ParameterValue::Float(1.5))
        .expect("initial gain is valid");

    let mut replacement =
        StateDocument::new("com.example.effect", 1).expect("product identity is valid");
    replacement
        .insert(StateEntry::new("parameter/gain", StateValue::Float(0.25)))
        .expect("replacement parameter is unique");
    let encoded = replacement.encode().expect("replacement encodes");
    let migrations: [&NeverMigration; 0] = [];

    for length in 0..encoded.len() {
        assert!(
            runtime
                .apply_parameter_state_bytes(
                    &encoded[..length],
                    "com.example.effect",
                    1,
                    StateLimits::default(),
                    &migrations,
                )
                .is_err(),
            "runtime accepted truncated prefix of {length} bytes"
        );
        assert_eq!(
            runtime.parameters().get("gain"),
            Some(&ParameterValue::Float(1.5))
        );
    }

    runtime
        .apply_parameter_state_bytes(
            &encoded,
            "com.example.effect",
            1,
            StateLimits::default(),
            &migrations,
        )
        .expect("complete replacement applies");
    assert_eq!(
        runtime.parameters().get("gain"),
        Some(&ParameterValue::Float(0.25))
    );
}
