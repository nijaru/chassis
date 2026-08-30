//! Golden compatibility fixtures for released Chassis state envelopes.

use chassis_core::state::{StateDocument, StateValue};

const STATE_V1: &[u8] = include_bytes!("fixtures/state-v1.chss");

#[test]
fn v1_fixture_decodes_and_reencodes_byte_identically() {
    let document = StateDocument::decode(STATE_V1).expect("v1 fixture decodes");

    assert_eq!(document.product_id(), "com.example.effect");
    assert_eq!(document.product_schema(), 1);
    assert_eq!(document.entries().len(), 1);
    assert_eq!(document.entries()[0].key(), "parameter/old_gain");
    assert_eq!(document.entries()[0].value(), &StateValue::Float(0.25));

    assert_eq!(
        document.encode().expect("v1 fixture reencodes").as_slice(),
        STATE_V1
    );
}
