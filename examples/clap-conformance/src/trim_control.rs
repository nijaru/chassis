//! Product-owned percent display and a square-law framework control taper.
//! CLAP automation and persisted values remain plain linear gain.

use core::fmt;

use chassis_runtime::parameters::{
    ParameterControlError, ParameterControlPolicy, ParameterKind, ParameterValue,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TrimControl {
    pub precision: u8,
}

impl ParameterControlPolicy for TrimControl {
    fn validate(&self, kind: &ParameterKind) -> Result<(), ParameterControlError> {
        if matches!(
            kind,
            ParameterKind::Float {
                minimum: 0.0,
                maximum: 1.0,
                ..
            }
        ) {
            Ok(())
        } else {
            Err(ParameterControlError::InvalidPolicy(
                "trim requires a 0..=1 float",
            ))
        }
    }

    fn value_to_normalized(
        &self,
        _: &ParameterKind,
        value: &ParameterValue,
    ) -> Result<f64, ParameterControlError> {
        let ParameterValue::Float(value) = value else {
            return Err(ParameterControlError::InvalidText);
        };
        Ok(value.sqrt())
    }

    fn normalized_to_value(
        &self,
        _: &ParameterKind,
        normalized: f64,
    ) -> Result<ParameterValue, ParameterControlError> {
        Ok(ParameterValue::Float(normalized * normalized))
    }

    fn format(
        &self,
        _: &ParameterKind,
        value: &ParameterValue,
        writer: &mut dyn fmt::Write,
    ) -> fmt::Result {
        let ParameterValue::Float(value) = value else {
            return Err(fmt::Error);
        };
        write!(
            writer,
            "{:.*} %",
            usize::from(self.precision),
            value * 100.0
        )
    }

    fn parse(
        &self,
        _: &ParameterKind,
        text: &str,
    ) -> Result<ParameterValue, ParameterControlError> {
        let value = text
            .trim()
            .strip_suffix('%')
            .ok_or(ParameterControlError::InvalidText)?
            .trim()
            .parse::<f64>()
            .map_err(|_| ParameterControlError::InvalidText)?;
        Ok(ParameterValue::Float(value / 100.0))
    }
}
