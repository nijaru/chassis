//! In-process CLAP host coverage for product activation failure and retry.

use chassis_clap::{ClapStereoEffect, SingleComponentEntry};
use chassis_core::{
    process::{ActivationConfig, ProcessBlock, ProcessBufferSource},
    runtime::{Component, Process, Processor},
};
use clack_host::{factory::plugin::PluginFactory, prelude::*};

#[derive(Default)]
struct ActivationProbe;

impl Component for ActivationProbe {
    type Processor = ActivationProcessor;
    type ActivationError = ();

    fn schema(
        &self,
    ) -> Result<chassis_core::schema::ComponentSchema, chassis_core::schema::ComponentSchemaError>
    {
        chassis_core::schema::ComponentSchema::stereo_effect_with_state(
            chassis_core::schema::ComponentId::new("org.nijaru.chassis.semantic.crates.chassis-clap.tests.host-activation-failure.activationprobe")
                .expect("semantic component identity is valid"),
            chassis_core::schema::StateSchemaVersion::new(1),
            vec![],
        )
    }

    fn activate(
        &self,
        config: &ActivationConfig<'_>,
    ) -> Result<Self::Processor, Self::ActivationError> {
        if config.process().sample_rate() < 48_000.0 {
            Err(())
        } else {
            Ok(ActivationProcessor)
        }
    }
}

struct ActivationProcessor;

impl Processor for ActivationProcessor {}

impl Process<f32> for ActivationProcessor {
    fn process<B>(&mut self, _block: &mut ProcessBlock<'_, '_, '_, f32, B>)
    where
        B: ProcessBufferSource<f32> + ?Sized,
    {
    }
}

impl ClapStereoEffect for ActivationProbe {
    const CLAP_ID: &'static str = "org.nijaru.chassis.activation-probe";
    const CLAP_NAME: &'static str = "Chassis Activation Probe";
}

struct TestHostShared;
struct TestHostMainThread;
struct TestHostAudioProcessor;
struct TestHostHandlers;

impl SharedHandler<'_> for TestHostShared {
    fn request_restart(&self) {}
    fn request_process(&self) {}
    fn request_callback(&self) {}
}

impl MainThreadHandler<'_> for TestHostMainThread {}
impl AudioProcessorHandler<'_> for TestHostAudioProcessor {}

impl HostHandlers for TestHostHandlers {
    type Shared<'a> = TestHostShared;
    type MainThread<'a> = TestHostMainThread;
    type AudioProcessor<'a> = TestHostAudioProcessor;
}

#[test]
fn failed_product_activation_leaves_clap_instance_reusable() {
    let entry = PluginEntry::load_from_clack::<SingleComponentEntry<ActivationProbe>>(c"")
        .expect("static activation probe entry loads");
    let descriptor = entry
        .get_factory::<PluginFactory>()
        .expect("plugin factory exists")
        .plugin_descriptor(0)
        .expect("activation probe descriptor exists");
    let host_info = HostInfo::new("chassis-test", "", "", "").expect("host info is valid");
    let mut plugin = PluginInstance::<TestHostHandlers>::new(
        |()| TestHostShared,
        |_| TestHostMainThread,
        &entry,
        descriptor.id().expect("activation probe id is valid"),
        &host_info,
    )
    .expect("activation probe instantiates");

    assert!(
        plugin
            .activate(
                |_, _| TestHostAudioProcessor,
                PluginAudioConfiguration {
                    sample_rate: 44_100.0,
                    min_frames_count: 1,
                    max_frames_count: 128,
                },
            )
            .is_err()
    );
    assert!(!plugin.is_active());

    let processor = plugin
        .activate(
            |_, _| TestHostAudioProcessor,
            PluginAudioConfiguration {
                sample_rate: 48_000.0,
                min_frames_count: 1,
                max_frames_count: 256,
            },
        )
        .expect("same CLAP instance can activate after product failure");
    assert!(plugin.is_active());
    plugin.deactivate(processor);
    assert!(!plugin.is_active());
}
