//! Factory ABI coverage for author-supplied CLAP metadata and its storage lifetime.

#![allow(unsafe_code)]

use std::ffi::{CStr, CString};

use chassis_clap::{ClapComponent, ClapDescriptor, F32AndF64, SingleComponentEntry};
use chassis_runtime::{
    parameters::ParameterStore,
    process::{ActivationConfig, ProcessBlock, ProcessBufferSource},
    runtime::{Component, Process, ProcessError, Processor},
    schema::{ComponentSchema, ComponentSchemaError},
    state::StateEntry,
};
use clack_host::{factory::plugin::PluginFactory, prelude::PluginEntry};

#[derive(Default)]
struct MetadataProbe<const FULL: bool>;

impl<const FULL: bool> Component for MetadataProbe<FULL> {
    type Processor = Self;
    type ActivationError = core::convert::Infallible;

    fn schema(&self) -> Result<ComponentSchema, ComponentSchemaError> {
        ComponentSchema::stereo_effect(vec![])
    }

    fn activate(
        &self,
        _: &ActivationConfig<'_>,
        _: &ParameterStore,
        _: &[StateEntry],
    ) -> Result<Self, Self::ActivationError> {
        Ok(Self)
    }
}

impl<const FULL: bool> Processor for MetadataProbe<FULL> {}

impl<S, const FULL: bool> Process<S> for MetadataProbe<FULL> {
    fn process<B>(&mut self, _: &mut ProcessBlock<'_, '_, '_, S, B>) -> Result<(), ProcessError>
    where
        B: ProcessBufferSource<S> + ?Sized,
    {
        Ok(())
    }
}

impl<const FULL: bool> ClapComponent for MetadataProbe<FULL> {
    const CLAP_DESCRIPTOR: ClapDescriptor = if FULL {
        ClapDescriptor {
            features: &[c"audio-effect", c"stereo", c"custom-metadata-probe"],
            vendor: "Descriptor Test Vendor",
            version: env!("CARGO_PKG_VERSION"),
            description: "Factory-owned metadata storage probe",
            url: "https://example.com/product",
            manual_url: "https://example.com/manual",
            support_url: "https://example.com/support",
            ..ClapDescriptor::new("test.descriptor", "Descriptor Probe")
        }
    } else {
        ClapDescriptor::new("test.descriptor-minimal", "Minimal Descriptor Probe")
    };
}

fn assert_factory_metadata(entry: &PluginEntry, expected: ClapDescriptor) {
    // Query through the actual CLAP factory callback, then release the temporary
    // factory borrow. Only the entry keeps the descriptor's C-string storage live.
    let raw = {
        let factory = entry.get_factory::<PluginFactory>().unwrap();
        assert_eq!(factory.plugin_count(), 1);
        assert!(factory.plugin_descriptor(1).is_none());
        std::ptr::from_ref(factory.plugin_descriptor(0).unwrap().as_raw())
    };

    // Construct and drop an independent owned descriptor, and retain unrelated
    // C-string allocations before reading the original callback result again.
    // This catches storage tied to a temporary descriptor or builder.
    let independent = <chassis_clap::ChassisPlugin<MetadataProbe<true>> as
        clack_plugin::entry::DefaultPluginFactory>::get_descriptor();
    drop(independent);
    let allocations: Vec<_> = (0..64)
        .map(|index| CString::new(format!("unrelated metadata allocation {index}")).unwrap())
        .collect();

    // SAFETY: The entry remains live, so its factory descriptor and every
    // non-null string/feature pointer returned by CLAP remain readable here.
    unsafe {
        let raw = &*raw;
        for (actual, expected) in [
            (raw.id, expected.id),
            (raw.name, expected.name),
            (raw.vendor, expected.vendor),
            (raw.version, expected.version),
            (raw.description, expected.description),
            (raw.url, expected.url),
            (raw.manual_url, expected.manual_url),
            (raw.support_url, expected.support_url),
        ] {
            let bytes = if actual.is_null() {
                &[][..]
            } else {
                CStr::from_ptr(actual).to_bytes()
            };
            assert_eq!(bytes, expected.as_bytes());
        }
        if raw.features.is_null() {
            assert_eq!(expected.features, &[] as &[&CStr]);
        } else {
            for (index, expected) in expected.features.iter().enumerate() {
                let feature = *raw.features.add(index);
                assert!(!feature.is_null());
                assert_eq!(CStr::from_ptr(feature), *expected);
            }
            assert!((*raw.features.add(expected.features.len())).is_null());
        }
    }
    let factory = entry.get_factory::<PluginFactory>().unwrap();
    assert_eq!(
        raw,
        std::ptr::from_ref(factory.plugin_descriptor(0).unwrap().as_raw())
    );
    drop(allocations);
}

#[test]
fn both_precision_factories_preserve_all_metadata_and_own_c_strings() {
    let f32 =
        PluginEntry::load_from_clack::<SingleComponentEntry<MetadataProbe<true>>>(c"").unwrap();
    let dual =
        PluginEntry::load_from_clack::<SingleComponentEntry<MetadataProbe<true>, F32AndF64>>(c"")
            .unwrap();
    for entry in [&f32, &dual] {
        assert_factory_metadata(entry, MetadataProbe::<true>::CLAP_DESCRIPTOR);
    }
}

#[test]
fn both_precision_factories_leave_optional_metadata_unset() {
    let f32 =
        PluginEntry::load_from_clack::<SingleComponentEntry<MetadataProbe<false>>>(c"").unwrap();
    let dual =
        PluginEntry::load_from_clack::<SingleComponentEntry<MetadataProbe<false>, F32AndF64>>(c"")
            .unwrap();
    for entry in [&f32, &dual] {
        assert_factory_metadata(entry, MetadataProbe::<false>::CLAP_DESCRIPTOR);
    }
}
