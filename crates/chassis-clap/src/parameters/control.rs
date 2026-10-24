//! Main-thread control projection and CLAP text-buffer validation.

use core::fmt;

use chassis_runtime::parameters::{ParameterKind, ParameterValue};
use clack_extensions::params::ParamDisplayWriter;

use super::ClapParameterBinding;

impl ClapParameterBinding {
    pub(crate) fn format_control(
        &self,
        value: f64,
        writer: &mut ParamDisplayWriter<'_>,
    ) -> fmt::Result {
        // CLAP plain stepped doubles truncate before semantic presentation.
        let value = self
            .descriptor
            .value_from_event(self.event_value(value).ok_or(fmt::Error)?)
            .map_err(|_| fmt::Error)?;
        self.descriptor
            .format_value(&value, &mut HostTextWriter(writer))
            .map_err(|_| fmt::Error)
    }

    pub(crate) fn parse_control(&self, text: &str) -> Option<f64> {
        // Do not trim or otherwise interpret product syntax at the adapter.
        let value = self.descriptor.parse_value(text).ok()?;
        self.plain_value(self.descriptor.event_from_value(&value).ok()?)
    }

    pub(super) fn choice_text_representable(&self) -> bool {
        let ParameterKind::Choice { options, .. } = self.descriptor.kind() else {
            return true;
        };
        options.iter().all(|option| {
            let value = ParameterValue::Choice(option.id().clone());
            let mut text = String::new();
            self.descriptor.format_value(&value, &mut text).is_ok()
                && !text.is_empty()
                && !text.contains('\0')
                && self
                    .descriptor
                    .parse_value(&text)
                    .is_ok_and(|parsed| parsed == value)
        })
    }
}

// Pinned Clack silently truncates output, including within a UTF-8 code point.
// Reject unrepresentable writes before forwarding; the semantic formatter's
// sticky writer-error tracking also catches policies that swallow fmt::Error.
struct HostTextWriter<'a, 'b>(&'a mut ParamDisplayWriter<'b>);
impl fmt::Write for HostTextWriter<'_, '_> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        if text.contains('\0') || text.len() > self.0.remaining_len() {
            return Err(fmt::Error);
        }
        self.0.write_str(text)
    }
}
