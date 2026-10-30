#![forbid(unsafe_code)]

//! Format-independent component definitions, instance lifecycle, and processing.
//!
//! Domain modules expose deliberate pre-alpha contracts without plugin-format
//! or platform dependencies. Durable state belongs to an instance runtime;
//! active DSP history belongs to its processor.

pub mod audio;
pub mod automation;
pub mod buffer;
pub mod events;
pub mod midi;
pub mod modulation;
pub mod parameters;
pub mod process;
pub mod runtime;
pub mod schema;
pub mod state;
pub mod telemetry;
