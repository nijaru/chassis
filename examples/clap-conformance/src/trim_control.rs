//! Product-owned percent display and a square-law framework control taper.
//! CLAP automation and persisted values remain plain linear gain.

use core::fmt;

use chassis_runtime::parameters::{
    FloatMapping, ParameterControlError, ParameterControlPolicy, ParameterKind, ParameterValue,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TrimControl {
    pub precision: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TrimMapping;
impl FloatMapping for TrimMapping {
    fn validate(&self, minimum: f64, maximum: f64) -> Result<(), ParameterControlError> {
        if matches!((minimum, maximum), (0.0, 1.0)) {
            Ok(())
        } else {
            Err(ParameterControlError::InvalidPolicy(
                "trim mapping requires 0..=1",
            ))
        }
    }
    fn normalized_to_plain(&self, normalized: f64) -> f64 {
        normalized * normalized
    }
    fn plain_to_normalized(&self, plain: f64) -> Result<f64, ParameterControlError> {
        Ok(plain.sqrt())
    }
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
