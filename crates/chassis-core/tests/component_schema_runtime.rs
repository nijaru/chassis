//! External component-schema authority tests through public Chassis APIs.

use std::{convert::Infallible, num::NonZeroU32};

use chassis_core::{
    audio::{
        AudioIoConfiguration, AudioPortDescriptor, AudioPortIndex, DEFAULT_EFFECT_CONFIGURATION,
        DEFAULT_EFFECT_PORTS, MAIN_INPUT, PortDirection, PortKey, PortRole,
    },
    process::{ActivationConfig, ProcessConfig},
    runtime::{ActivateError, Component, InstanceRuntime, Processor},
    schema::{ComponentId, ComponentSchema, StateSchemaVersion},
};

const OTHER_INPUT: PortKey = PortKey::new("audio.other.in");
const OTHER_OUTPUT: PortKey = PortKey::new("audio.other.out");
const OTHER_PORTS: &[AudioPortDescriptor] = &[
    AudioPortDescriptor {
        key: OTHER_INPUT,
        name: "Other Input",
        direction: PortDirection::Input,
        role: PortRole::Main,
        optional: false,
    },
    AudioPortDescriptor {
        key: OTHER_OUTPUT,
        name: "Other Output",
        direction: PortDirection::Output,
        role: PortRole::Main,
        optional: false,
    },
];

struct Probe {
    audio_ports: &'static [AudioPortDescriptor],
    identity: &'static str,
}

impl Probe {
    fn schema_value(&self) -> ComponentSchema {
        ComponentSchema::new(
            ComponentId::new(self.identity).expect("test identity is valid"),
            StateSchemaVersion::new(1),
            self.audio_ports.to_vec(),
            vec![],
            vec![],
        )
        .expect("test schema is valid")
    }
}

impl Component for Probe {
    type Processor = ProbeProcessor;
    type ActivationError = Infallible;

    fn audio_ports(&self) -> &[AudioPortDescriptor] {
        self.audio_ports
    }

    fn schema(&self) -> Result<ComponentSchema, chassis_core::schema::ComponentSchemaError> {
        Ok(self.schema_value())
    }

    fn activate(
        &self,
        _config: &ActivationConfig<'_>,
    ) -> Result<Self::Processor, Self::ActivationError> {
        Ok(ProbeProcessor)
    }
}

struct ProbeProcessor;
impl Processor for ProbeProcessor {}

fn process_config() -> ProcessConfig {
    ProcessConfig::new(
        48_000.0,
        NonZeroU32::new(1),
        NonZeroU32::new(64).expect("maximum is non-zero"),
        0,
    )
    .expect("test process config is valid")
}

#[test]
fn runtime_owns_complete_schema_and_dense_audio_lookup() {
    let component = Probe {
        audio_ports: &DEFAULT_EFFECT_PORTS,
        identity: "org.nijaru.schema-runtime",
    };
    let runtime = InstanceRuntime::<ProbeProcessor>::for_component(&component)
        .expect("component schema is valid");

    let identity = runtime
        .schema()
        .state_identity()
        .expect("identified schema is retained");
    assert_eq!(identity.component().as_str(), "org.nijaru.schema-runtime");
    assert_eq!(identity.schema(), StateSchemaVersion::new(1));
    assert_eq!(
        runtime.audio_port_index(MAIN_INPUT),
        Some(AudioPortIndex::new(0))
    );
    assert_eq!(runtime.audio_ports(), DEFAULT_EFFECT_PORTS);
}

#[test]
fn activation_rejects_audio_schema_drift() {
    let original = Probe {
        audio_ports: &DEFAULT_EFFECT_PORTS,
        identity: "org.nijaru.schema-runtime",
    };
    let changed = Probe {
        audio_ports: OTHER_PORTS,
        identity: "org.nijaru.schema-runtime",
    };
    let mut runtime = InstanceRuntime::<ProbeProcessor>::for_component(&original)
        .expect("original schema is valid");

    assert!(matches!(
        runtime.activate(&changed, process_config(), AudioIoConfiguration::new(&[])),
        Err(ActivateError::AudioPortSchemaMismatch)
    ));
}

#[test]
fn activation_rejects_semantic_state_identity_drift() {
    let original = Probe {
        audio_ports: &DEFAULT_EFFECT_PORTS,
        identity: "org.nijaru.schema-runtime",
    };
    let changed = Probe {
        audio_ports: &DEFAULT_EFFECT_PORTS,
        identity: "org.nijaru.other-runtime",
    };
    let mut runtime = InstanceRuntime::<ProbeProcessor>::for_component(&original)
        .expect("original schema is valid");

    assert!(matches!(
        runtime.activate(&changed, process_config(), DEFAULT_EFFECT_CONFIGURATION),
        Err(ActivateError::StateIdentityMismatch)
    ));
}
