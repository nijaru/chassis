#![forbid(unsafe_code)]

//! Format-independent semantic contracts for Chassis.
//!
//! Public modules are deliberate pre-alpha API surfaces. Chassis does not
//! glob-re-export them here: callers should name the domain they are using
//! while the framework contracts are still being proven.

pub mod audio;
pub mod buffer;
pub mod parameters;
pub mod process;
pub mod runtime;
pub mod state;
