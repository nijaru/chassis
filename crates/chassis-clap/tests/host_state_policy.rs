//! Actual-entry admission of component-owned state policy and migration.

use chassis_clap::{ClapComponent, SingleComponentEntry};
use chassis_runtime::{
    parameters::{ParameterDescriptor, ParameterStore, ParameterValue},
    process::{ActivationConfig, ProcessBlock},
    runtime::{Component, ComponentStateError, Process, Processor},
    schema::{ComponentId, ComponentSchema, ComponentSchemaError, StateSchemaVersion},
    state::{StateDocument, StateEntry, StateLimits, StateMigration, StateValue},
};
use clack_extensions::state::PluginState;
use clack_host::{factory::plugin::PluginFactory, prelude::*};
use std::{
    io::Cursor,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

static DEFAULT_COUNT: AtomicUsize = AtomicUsize::new(0);
static SCHEMA_DRIFT: AtomicBool = AtomicBool::new(false);

struct PolicyProbe<const KIND: u8> {
    custom: bool,
}
impl<const KIND: u8> Default for PolicyProbe<KIND> {
    fn default() -> Self {
        Self {
            custom: matches!(KIND, 1 | 4)
                || (KIND == 2 && DEFAULT_COUNT.fetch_add(1, Ordering::Relaxed) > 0),
        }
    }
}

struct RenameGate;
impl StateMigration for RenameGate {
    type Error = ComponentStateError;
    fn source_schema(&self) -> u32 {
        1
    }
    fn to_schema(&self) -> u32 {
        2
    }
    fn migrate(&self, doc: StateDocument) -> Result<StateDocument, Self::Error> {
        let mut result =
            StateDocument::new(doc.component_id(), 2).map_err(ComponentStateError::new)?;
        for entry in doc.entries() {
            let key = if entry.key() == "parameter/old_gate" {
                "parameter/gate"
            } else {
                entry.key()
            };
            result
                .insert(StateEntry::new(key, entry.value().clone()))
                .map_err(ComponentStateError::new)?;
        }
        Ok(result)
    }
}
impl<const KIND: u8> Component for PolicyProbe<KIND> {
    type Processor = PolicyProcessor;
    type ActivationError = std::convert::Infallible;
    fn schema(&self) -> Result<ComponentSchema, ComponentSchemaError> {
        let default = if KIND == 3 && SCHEMA_DRIFT.load(Ordering::Relaxed) {
            0.25
        } else {
            0.0
        };
        ComponentSchema::stereo_effect_with_state(
            ComponentId::new("test.policy").unwrap(),
            StateSchemaVersion::new(2),
            vec![ParameterDescriptor::float("gate", "Gate", 0.0, 1.0, default).unwrap()],
        )
    }
    fn default_custom_state(&self) -> Vec<StateEntry> {
        if KIND == 4 {
            vec![StateEntry::new(
                format!(
                    "oversized/{}",
                    "x".repeat(StateLimits::default().max_key_bytes)
                ),
                StateValue::Boolean(true),
            )]
        } else if self.custom {
            vec![StateEntry::new("state/flag", StateValue::Boolean(true))]
        } else {
            vec![]
        }
    }
    fn validate_state(
        &self,
        params: &ParameterStore,
        custom: &[StateEntry],
    ) -> Result<(), ComponentStateError> {
        let Some(ParameterValue::Float(gate)) = params.get("gate") else {
            unreachable!("float schema");
        };
        let valid_custom = if KIND == 4 {
            matches!(custom, [entry] if entry.key().starts_with("oversized/") && entry.value() == &StateValue::Boolean(true))
        } else if self.custom {
            matches!(custom, [entry] if entry.key() == "state/flag" && entry.value() == &StateValue::Boolean(true))
        } else {
            custom.is_empty()
        };
        if *gate > 0.5 || !valid_custom {
            return Err(ComponentStateError::new(std::io::Error::other(
                "policy rejected candidate",
            )));
        }
        Ok(())
    }
    fn state_migrations(&self) -> &[&dyn StateMigration<Error = ComponentStateError>] {
        &[&RenameGate]
    }
    fn activate(
        &self,
        _: &ActivationConfig<'_>,
        _: &ParameterStore,
        _: &[StateEntry],
    ) -> Result<PolicyProcessor, Self::ActivationError> {
        Ok(PolicyProcessor)
    }
}
struct PolicyProcessor;
impl Processor for PolicyProcessor {}
impl Process<f32> for PolicyProcessor {
    fn process(
        &mut self,
        _: &mut ProcessBlock<'_, f32>,
    ) -> Result<(), chassis_runtime::runtime::ProcessError> {
        Ok(())
    }
}
impl<const KIND: u8> ClapComponent for PolicyProbe<KIND> {
    const CLAP_DESCRIPTOR: chassis_clap::ClapDescriptor = chassis_clap::ClapDescriptor {
        features: &[c"audio-effect", c"stereo"],
        version: env!("CARGO_PKG_VERSION"),
        ..chassis_clap::ClapDescriptor::new("test.policy-export", "State Policy")
    };
    const CLAP_PARAMETER_IDS: &'static [(&'static str, u32)] = &[("gate", 11)];
    const CLAP_MAX_PARAMETER_EVENTS: u32 = 8;
}

struct Handlers;
struct Shared;
struct Main;
struct Audio;
impl SharedHandler<'_> for Shared {
    fn request_restart(&self) {}
    fn request_process(&self) {}
    fn request_callback(&self) {}
}
impl MainThreadHandler<'_> for Main {}
impl AudioProcessorHandler<'_> for Audio {}
impl HostHandlers for Handlers {
    type Shared<'a> = Shared;
    type MainThread<'a> = Main;
    type AudioProcessor<'a> = Audio;
}
fn entry<const KIND: u8>() -> PluginEntry {
    PluginEntry::load_from_clack::<SingleComponentEntry<PolicyProbe<KIND>>>(c"").unwrap()
}
fn instance(entry: &PluginEntry) -> Result<PluginInstance<Handlers>, PluginInstanceError> {
    let descriptor = entry
        .get_factory::<PluginFactory>()
        .unwrap()
        .plugin_descriptor(0)
        .unwrap();
    PluginInstance::new(
        |()| Shared,
        |_| Main,
        entry,
        descriptor.id().unwrap(),
        &HostInfo::new("state-policy-test", "", "", "").unwrap(),
    )
}
fn document(schema: u32, key: &str, value: f64) -> Vec<u8> {
    let mut doc = StateDocument::new("test.policy", schema).unwrap();
    doc.insert(StateEntry::new(key, StateValue::Float(value)))
        .unwrap();
    doc.encode().unwrap()
}
fn save(plugin: &mut PluginInstance<Handlers>, extension: PluginState) -> Vec<u8> {
    let mut bytes = Vec::new();
    extension.save(&plugin.plugin_handle(), &mut bytes).unwrap();
    bytes
}

#[test]
fn load_uses_component_migrations_and_rejects_policy_failures_atomically() {
    let entry = entry::<0>();
    let mut plugin = instance(&entry).unwrap();
    let state = plugin
        .plugin_handle()
        .get_extension::<PluginState>()
        .unwrap();
    let processor = plugin
        .activate(
            |_, _| Audio,
            PluginAudioConfiguration {
                sample_rate: 48_000.0,
                min_frames_count: 1,
                max_frames_count: 64,
            },
        )
        .unwrap();
    state
        .load(
            &plugin.plugin_handle(),
            &mut Cursor::new(document(1, "parameter/old_gate", 0.25)),
        )
        .unwrap();
    let accepted = document(2, "parameter/gate", 0.25);
    assert_eq!(save(&mut plugin, state), accepted);
    assert!(
        state
            .load(
                &plugin.plugin_handle(),
                &mut Cursor::new(document(2, "parameter/gate", 0.75))
            )
            .is_err()
    );
    assert_eq!(save(&mut plugin, state), accepted);
    plugin.deactivate(processor);
}

#[test]
fn original_custom_defaults_are_preserved_and_retained_policy_is_enforced() {
    let mut plugin = instance(&entry::<1>()).unwrap();
    let state = plugin
        .plugin_handle()
        .get_extension::<PluginState>()
        .unwrap();
    let saved = StateDocument::decode(&save(&mut plugin, state)).unwrap();
    assert!(saved.entries().iter().any(|entry| entry.key() == "state/flag" && entry.value() == &StateValue::Boolean(true)));
    // The retained definition requires a field the original shared generation
    // lacks. Do not silently substitute its independently constructed defaults.
    DEFAULT_COUNT.store(0, Ordering::Relaxed);
    assert!(instance(&entry::<2>()).is_err());
}

#[test]
fn defaults_outside_the_deployment_wire_budget_are_rejected() {
    // Core policy accepts this definition; CLAP must not instantiate a state it
    // cannot save/reopen through its bounded stream.
    assert!(
        chassis_runtime::runtime::InstanceRuntime::for_component(&PolicyProbe::<4>::default())
            .is_ok()
    );
    assert!(instance(&entry::<4>()).is_err());
}

#[test]
fn component_schema_drift_rejects_load_without_changing_shared_values() {
    SCHEMA_DRIFT.store(false, Ordering::Relaxed);
    let entry = entry::<3>();
    let mut plugin = instance(&entry).unwrap();
    let state = plugin
        .plugin_handle()
        .get_extension::<PluginState>()
        .unwrap();
    let before = save(&mut plugin, state);
    SCHEMA_DRIFT.store(true, Ordering::Relaxed);
    assert!(
        state
            .load(
                &plugin.plugin_handle(),
                &mut Cursor::new(document(2, "parameter/gate", 0.5))
            )
            .is_err()
    );
    assert_eq!(save(&mut plugin, state), before);
    SCHEMA_DRIFT.store(false, Ordering::Relaxed);
}
