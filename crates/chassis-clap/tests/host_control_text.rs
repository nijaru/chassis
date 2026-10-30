//! Actual-entry text failures and custom choice syntax at the CLAP boundary.

use chassis_clap::{ClapComponent, SingleComponentEntry};
use chassis_runtime::{
    parameters::{
        ChoiceOption, ParameterControlError, ParameterControlPolicy, ParameterDescriptor,
        ParameterKind, ParameterStore, ParameterValue,
    },
    process::{ActivationConfig, ProcessBlock},
    runtime::{Component, Process, Processor},
    schema::{ComponentId, ComponentSchema, ComponentSchemaError, StateSchemaVersion},
    state::StateEntry,
};
use clack_extensions::params::PluginParams;
use clack_host::prelude::*;
use core::{convert::Infallible, fmt};

#[derive(Debug, PartialEq)]
struct TextPolicy;
impl ParameterControlPolicy for TextPolicy {
    fn format(
        &self,
        _: &ParameterKind,
        value: &ParameterValue,
        writer: &mut dyn fmt::Write,
    ) -> fmt::Result {
        let text = if value == &ParameterValue::Float(0.0) {
            "µs"
        } else {
            "a\0b"
        };
        // An author accidentally swallowing an error must not make truncated or
        // NUL-containing text a successful CLAP conversion.
        let _ = writer.write_str(text);
        Ok(())
    }
}
#[derive(Debug, PartialEq)]
struct ChoicePolicy;
impl ParameterControlPolicy for ChoicePolicy {
    fn format(
        &self,
        _: &ParameterKind,
        value: &ParameterValue,
        writer: &mut dyn fmt::Write,
    ) -> fmt::Result {
        let ParameterValue::Choice(id) = value else {
            return Err(fmt::Error);
        };
        write!(writer, " [{}] ", id.as_str())
    }
    fn parse(
        &self,
        kind: &ParameterKind,
        text: &str,
    ) -> Result<ParameterValue, ParameterControlError> {
        let ParameterKind::Choice { options, .. } = kind else {
            return Err(ParameterControlError::InvalidText);
        };
        let id = text
            .strip_prefix(" [")
            .and_then(|text| text.strip_suffix("] "))
            .ok_or(ParameterControlError::InvalidText)?;
        let option = options
            .iter()
            .find(|option| option.id().as_str() == id)
            .ok_or(ParameterControlError::InvalidText)?;
        Ok(ParameterValue::Choice(option.id().clone()))
    }
}
#[derive(Default)]
struct Definition;
struct Dsp;
impl Processor for Dsp {}
impl Process<f32> for Dsp {
    fn process(
        &mut self,
        _: &mut ProcessBlock<'_, f32>,
    ) -> Result<(), chassis_runtime::runtime::ProcessError> {
        Ok(())
    }
}
impl Component for Definition {
    type Processor = Dsp;
    type ActivationError = Infallible;
    fn schema(&self) -> Result<ComponentSchema, ComponentSchemaError> {
        ComponentSchema::stereo_effect_with_state(
            ComponentId::new("test.control-text").unwrap(),
            StateSchemaVersion::new(1),
            vec![
                ParameterDescriptor::float("text", "Text", 0.0, 1.0, 0.0)
                    .unwrap()
                    .with_control_policy(TextPolicy)
                    .unwrap(),
                ParameterDescriptor::choice(
                    "mode",
                    "Mode",
                    vec![
                        ChoiceOption::new("first-id", "same\0name").unwrap(),
                        ChoiceOption::new("last-id", "same\0name").unwrap(),
                    ],
                    "first-id",
                )
                .unwrap()
                .with_control_policy(ChoicePolicy)
                .unwrap(),
            ],
        )
    }
    fn activate(
        &self,
        _: &ActivationConfig<'_>,
        _: &ParameterStore,
        _: &[StateEntry],
    ) -> Result<Dsp, Infallible> {
        Ok(Dsp)
    }
}
impl ClapComponent for Definition {
    const CLAP_DESCRIPTOR: chassis_clap::ClapDescriptor = chassis_clap::ClapDescriptor {
        features: &[c"audio-effect", c"stereo"],
        version: env!("CARGO_PKG_VERSION"),
        ..chassis_clap::ClapDescriptor::new("test.control-text", "Control Text")
    };
    const CLAP_PARAMETER_IDS: &'static [(&'static str, u32)] = &[("text", 1), ("mode", 2)];
    const CLAP_MAX_PARAMETER_EVENTS: u32 = 2;
}
struct Handlers;
impl SharedHandler<'_> for Handlers {
    fn request_restart(&self) {}
    fn request_process(&self) {}
    fn request_callback(&self) {}
}
impl MainThreadHandler<'_> for Handlers {}
impl AudioProcessorHandler<'_> for Handlers {}
impl HostHandlers for Handlers {
    type Shared<'a> = Self;
    type MainThread<'a> = Self;
    type AudioProcessor<'a> = Self;
}

#[test]
fn custom_text_is_faithful_or_rejected_never_silently_truncated() {
    let entry = PluginEntry::load_from_clack::<SingleComponentEntry<Definition>>(c"").unwrap();
    let host = HostInfo::new("test", "", "", "").unwrap();
    let mut plugin = PluginInstance::<Handlers>::new(
        |()| Handlers,
        |_| Handlers,
        &entry,
        c"test.control-text",
        &host,
    )
    .unwrap();
    let handle = plugin.plugin_handle();
    let params = handle.get_extension::<PluginParams>().unwrap();
    let mut buffer = [0; 32];
    assert_eq!(
        params
            .value_to_text(&handle, ClapId::new(1), 0.0, &mut buffer)
            .unwrap(),
        "µs".as_bytes()
    );
    for length in [0, 1, 2, 3] {
        assert!(
            params
                .value_to_text(&handle, ClapId::new(1), 0.0, &mut buffer[..length])
                .is_err()
        );
    }
    assert!(
        params
            .value_to_text(&handle, ClapId::new(1), 1.0, &mut buffer)
            .is_err()
    );
    for (plain, text) in [(0.9, c" [first-id] "), (1.0, c" [last-id] ")] {
        assert_eq!(
            params
                .value_to_text(&handle, ClapId::new(2), plain, &mut buffer)
                .unwrap(),
            text.to_bytes()
        );
        assert_eq!(
            params.text_to_value(&handle, ClapId::new(2), text),
            Some(plain.trunc())
        );
    }
    assert_eq!(
        params.text_to_value(&handle, ClapId::new(2), c"[first-id]"),
        None
    );
}
