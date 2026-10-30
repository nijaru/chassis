//! In-process CLAP host coverage for product activation failure and retry.

use chassis_clap::{ClapComponent, SingleComponentEntry};
use chassis_runtime::{
    process::{ActivationConfig, ProcessBlock},
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
    ) -> Result<
        chassis_runtime::schema::ComponentSchema,
        chassis_runtime::schema::ComponentSchemaError,
    > {
        chassis_runtime::schema::ComponentSchema::stereo_effect_with_state(
            chassis_runtime::schema::ComponentId::new("org.nijaru.chassis.semantic.crates.chassis-clap.tests.host-activation-failure.activationprobe")
                .expect("semantic component identity is valid"),
            chassis_runtime::schema::StateSchemaVersion::new(1),
            vec![],
        )
    }

    fn activate(
        &self,
        config: &ActivationConfig<'_>,
        _parameters: &chassis_runtime::parameters::ParameterStore,
        _custom_state: &[chassis_runtime::state::StateEntry],
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
    fn process(
        &mut self,
        _block: &mut ProcessBlock<'_, f32>,
    ) -> Result<(), chassis_runtime::runtime::ProcessError> {
        Ok(())
    }
}

impl ClapComponent for ActivationProbe {
    const CLAP_DESCRIPTOR: chassis_clap::ClapDescriptor = chassis_clap::ClapDescriptor {
        features: &[c"audio-effect", c"stereo"],
        version: env!("CARGO_PKG_VERSION"),
        ..chassis_clap::ClapDescriptor::new(
            "org.nijaru.chassis.activation-probe",
            "Chassis Activation Probe",
        )
    };
}

#[derive(Default)]
struct EventProbe;

impl Component for EventProbe {
    type Processor = ActivationProcessor;
    type ActivationError = ();

    fn schema(
        &self,
    ) -> Result<
        chassis_runtime::schema::ComponentSchema,
        chassis_runtime::schema::ComponentSchemaError,
    > {
        use chassis_runtime::{
            audio::AudioIoPolicy,
            events::{EventDialect, EventPortDescriptor, EventPortDirection, EventPortKey},
            schema::{ComponentId, ComponentSchema, StateSchemaVersion},
        };
        ComponentSchema::new(
            ComponentId::new("org.nijaru.chassis.event-probe").expect("valid identity"),
            StateSchemaVersion::new(1),
            vec![],
            AudioIoPolicy::AnyStructurallyValid,
            vec![EventPortDescriptor::new(
                EventPortKey::new("notes.in"),
                "Notes",
                EventPortDirection::Input,
                &[EventDialect::Notes],
            )],
            vec![],
        )
    }

    fn activate(
        &self,
        _: &ActivationConfig<'_>,
        _parameters: &chassis_runtime::parameters::ParameterStore,
        _custom_state: &[chassis_runtime::state::StateEntry],
    ) -> Result<Self::Processor, Self::ActivationError> {
        Ok(ActivationProcessor)
    }
}

impl ClapComponent for EventProbe {
    const CLAP_DESCRIPTOR: chassis_clap::ClapDescriptor = chassis_clap::ClapDescriptor {
        features: &[c"note-effect"],
        version: env!("CARGO_PKG_VERSION"),
        ..chassis_clap::ClapDescriptor::new("org.nijaru.chassis.event-probe", "Chassis Event Probe")
    };
    const CLAP_AUDIO_PORTS: &'static [chassis_clap::ClapAudioPort] = &[];
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
fn clap_preserves_descriptor_features_but_rejects_unprojected_event_ports() {
    let entry =
        PluginEntry::load_from_clack::<SingleComponentEntry<EventProbe>>(c"").expect("entry loads");
    let descriptor = entry
        .get_factory::<PluginFactory>()
        .expect("factory exists")
        .plugin_descriptor(0)
        .expect("descriptor exists");
    assert_eq!(descriptor.features().collect::<Vec<_>>(), [c"note-effect"]);
    let host_info = HostInfo::new("chassis-test", "", "", "").expect("host info is valid");
    assert!(
        PluginInstance::<TestHostHandlers>::new(
            |()| TestHostShared,
            |_| TestHostMainThread,
            &entry,
            descriptor.id().expect("id exists"),
            &host_info,
        )
        .is_err()
    );
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
