//! Main/audio-safe CLAP tail projection, never borrowing the live processor.
//!
//! The pinned Clack tail callback borrows `AudioProcessor` even for main-thread
//! queries and notification re-entry. Keep this standard ABI shim local instead.
#![allow(unsafe_code)]

use std::{ffi::CStr, sync::atomic::Ordering};

use chassis_runtime::runtime::TailLength;
use clack_plugin::extensions::prelude::*;
use clap_sys::ext::tail::{CLAP_EXT_TAIL, clap_plugin_tail};

use crate::ChassisShared;

const INFINITE: u32 = i32::MAX as u32;

pub(crate) fn project(tail: TailLength) -> u32 {
    match tail {
        TailLength::FiniteSamples(samples) => {
            u32::try_from(samples).unwrap_or(INFINITE).min(INFINITE)
        }
        TailLength::Infinite => INFINITE,
    }
}

impl ChassisShared {
    pub(super) fn publish_tail_snapshot(&self, tail: TailLength) {
        let projected = project(tail);
        if self.tail.swap(projected, Ordering::Relaxed) != projected {
            // Notification debt belongs to the instance, not an activation that
            // may be deactivated before it ever reaches an audio callback.
            self.tail_notification_pending
                .store(true, Ordering::Relaxed);
        }
    }
}

/// Registration marker; plugin callbacks use only the shared scalar snapshot.
#[derive(Clone, Copy)]
pub(crate) struct PluginTailSnapshot;

// SAFETY: The identifier and implementation below use the standard clap_plugin_tail
// ABI. This private marker never exposes operations on a supplied extension pointer.
unsafe impl Extension for PluginTailSnapshot {
    const IDENTIFIERS: &[&CStr] = &[CLAP_EXT_TAIL];
    type ExtensionSide = PluginExtensionSide;

    unsafe fn from_raw(_raw: RawExtension<Self::ExtensionSide>) -> Self {
        Self
    }
}

// SAFETY: The static vtable has the standard repr(C) tail layout and callback ABI.
// Its callback accesses only Sync shared state; it never borrows either exclusive
// domain, including during a synchronous host notification -> get re-entry.
unsafe impl<P> ExtensionImplementation<P> for PluginTailSnapshot
where
    P: for<'a> Plugin<Shared<'a> = ChassisShared>,
{
    const IMPLEMENTATION: RawExtensionImplementation =
        RawExtensionImplementation::new(&clap_plugin_tail {
            get: Some(get::<P>),
        });
}

// SAFETY contract: CLAP supplies a live initialized plugin pointer of type P,
// on the main or audio thread. Clack validates initialized wrapper access and
// contains panics. shared() is cross-thread safe, unlike audio_processor().
unsafe extern "C" fn get<P>(plugin: *const clap_plugin) -> u32
where
    P: for<'a> Plugin<Shared<'a> = ChassisShared>,
{
    // SAFETY: pointer/lifetime obligations are those of the CLAP callback above.
    unsafe {
        PluginWrapper::<P>::handle(plugin, |plugin| {
            Ok(plugin.shared().tail.load(Ordering::Relaxed))
        })
    }
    .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finite_projection_never_wraps_or_understates_long_tails() {
        for (samples, expected) in [
            (0, 0),
            (1, 1),
            (2_147_483_646, 2_147_483_646),
            (2_147_483_647, INFINITE),
            (u64::from(u32::MAX), INFINITE),
            (u64::MAX, INFINITE),
        ] {
            assert_eq!(project(TailLength::FiniteSamples(samples)), expected);
        }
        assert_eq!(project(TailLength::Infinite), INFINITE);
    }
}
