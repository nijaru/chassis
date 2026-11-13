//! Optional main-thread product control and bounded host delivery.

use std::{
    any::Any,
    cell::Cell,
    fmt,
    marker::PhantomData,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
    sync::atomic::{AtomicBool, AtomicU8, Ordering},
};

use chassis_runtime::{
    controls::{
        ControlError, ControlNotification, ControlObservation, ControlValue, GestureProtocol,
        GestureToken,
    },
    parameters::ParameterIndex,
};
use clack_plugin::{
    events::{
        Pckn,
        event_types::{ParamGestureBeginEvent, ParamGestureEndEvent, ParamValueEvent},
        io::OutputEvents,
    },
    host::HostSharedHandle,
    plugin::PluginError,
};

use crate::{ClapComponent, parameters::ClapStateProjection};

mod queue;
#[cfg(test)]
mod tests;
use queue::{ControlNotice, NotificationQueue};

/// Optional product-owned main-thread controller for a CLAP deployment.
///
/// Select it as the third type argument of [`crate::SingleComponentEntry`]. The
/// definition and DSP remain independent of this controller. No window, worker,
/// timer, cross-thread endpoint or host handle is created implicitly.
///
/// Construction happens once during initialization, without a submission
/// context. The host then services a requested main-thread callback. Subsequent
/// callbacks can be requested explicitly through [`ClapControls::request_callback`].
/// Host scheduling has no fixed latency guarantee.
pub trait ClapController<C: ClapComponent>: Sized + 'static {
    /// Essential notification capacity, including reserved future End records.
    ///
    /// An enabled controller needs at least three records for one discrete edit.
    /// Zero disables the controller, including construction and gesture storage.
    const MAX_PENDING_NOTIFICATIONS: u32;

    /// Maximum records inspected or offered to the host per process/flush call.
    ///
    /// Superseded Values count against this bound too. Must be positive when
    /// enabled; host refusal stops the attempt without dropping the suffix.
    const MAX_DELIVERIES_PER_CALLBACK: u32 = Self::MAX_PENDING_NOTIFICATIONS;

    /// Prepare product-owned control state on the main thread.
    ///
    /// # Errors
    /// Return a setup error without publishing a partially initialized controller.
    fn new(component: &C) -> Result<Self, PluginError>;

    /// Service product intent using a scoped, main-thread-only capability.
    ///
    /// Handle admission errors explicitly; successful submission is not a host
    /// delivery receipt. A panic disables this controller and closes its gestures;
    /// it does not reset or resume the controller's potentially damaged history.
    fn on_main_thread(&mut self, controls: &mut ClapControls<'_>);
}

/// Default deployment capability: no product controller or notification queue.
pub struct NoController;

impl<C: ClapComponent> ClapController<C> for NoController {
    const MAX_PENDING_NOTIFICATIONS: u32 = 0;

    fn new(_component: &C) -> Result<Self, PluginError> {
        Ok(Self)
    }
    fn on_main_thread(&mut self, _controls: &mut ClapControls<'_>) {}
}

/// Submission failed before changing canonical state, gesture authority or output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClapControlError {
    /// Parameter, token, value or normalized mapping failed core admission.
    Control(ControlError),
    /// Capacity is occupied or reserved for accepted gestures' future Ends.
    Full,
    /// A host output attempt or another main-thread mutation is in flight.
    Busy,
    /// This host does not provide parameter-flush requests.
    HostUnavailable,
    /// The instance's canonical publication protocol is terminally faulted.
    InstanceFaulted,
}

impl fmt::Display for ClapControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Control(error) => error.fmt(formatter),
            Self::Full => formatter.write_str("control notification capacity is reserved or full"),
            Self::Busy => formatter.write_str("control delivery or mutation is in flight"),
            Self::HostUnavailable => {
                formatter.write_str("host parameter-flush requests are unavailable")
            }
            Self::InstanceFaulted => {
                formatter.write_str("control publication is faulted; replace this instance")
            }
        }
    }
}
impl std::error::Error for ClapControlError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Control(error) => Some(error),
            _ => None,
        }
    }
}
impl From<ControlError> for ClapControlError {
    fn from(error: ControlError) -> Self {
        Self::Control(error)
    }
}

/// Scoped NON-REALTIME product submission into one canonical desired projection.
///
/// All effects are queued by the adapter. Never deliver core effects separately.
/// Capacity is reserved before acceptance; accepted Begin always retains End
/// credit. Local Value history remains ordered, while later complete loads or
/// external host edits can supersede undelivered Values without losing closure.
///
/// This capability cannot leave the main thread or outlive its callback. State
/// loading reentered during this session fails Busy rather than borrowing gesture
/// metadata twice. A load inside any nested same-thread host-output callback also
/// fails before stream I/O; ordinary loads may wait off the audio thread for the
/// in-flight output attempt. Canonical edits can fail Busy during a host output attempt;
/// callers may retry explicitly. Queries observe latest desired, not modulated or
/// frozen consumed DSP, state. End and close do not allocate value revisions.
///
/// ```compile_fail
/// fn cannot_move_to_worker(controls: &mut chassis_clap::ClapControls<'_>) {
///     std::thread::scope(|scope| {
///         scope.spawn(move || controls.close());
///     });
/// }
/// ```
pub struct ClapControls<'a> {
    parameters: &'a ClapStateProjection,
    outbox: &'a ControlOutbox,
    protocol: &'a mut GestureProtocol,
    reserved_ends: &'a mut usize,
    callback_requested: &'a mut bool,
    host_available: bool,
    _main_thread: PhantomData<Rc<()>>,
}

impl ClapControls<'_> {
    /// Resolve a stable parameter key against this instance's immutable schema.
    #[must_use]
    pub fn parameter_index(&self, key: &str) -> Option<ParameterIndex> {
        self.parameters
            .index_for_key(key)
            .map(|index| self.parameters.bindings()[index].index())
    }

    /// Observe latest coherent desired base/provenance and gesture authority.
    ///
    /// # Errors
    /// Rejects an unknown index or a terminally faulted publication protocol.
    pub fn observe(
        &self,
        parameter: ParameterIndex,
    ) -> Result<ControlObservation, ClapControlError> {
        self.healthy()?;
        let gesture = self.protocol.status(parameter)?;
        let index = usize::try_from(parameter.get())
            .map_err(|_| ControlError::UnknownParameter(parameter))?;
        let snapshot = self
            .parameters
            .control_snapshot(index)
            .map_err(|_| ClapControlError::InstanceFaulted)?;
        self.healthy()?;
        let value = self.parameters.bindings()[index]
            .process_value(snapshot.plain)
            .ok_or(ClapControlError::InstanceFaulted)?;
        Ok(ControlObservation::new(
            parameter,
            value,
            snapshot.coordinate,
            gesture,
        ))
    }

    /// Accept a local Begin, reserving both its record and its future End.
    ///
    /// # Errors
    /// Rejects invalid/open parameters, unavailable delivery, full capacity,
    /// exhausted serials or a faulted instance without opening a gesture.
    pub fn begin(&mut self, parameter: ParameterIndex) -> Result<GestureToken, ClapControlError> {
        self.protocol.ensure_idle(parameter)?;
        self.admit(2)?;
        let (token, notice) = self.protocol.begin(parameter)?;
        *self.reserved_ends += 1;
        self.outbox.queue.push_reserved(metadata_notice(notice));
        Ok(token)
    }

    /// Accept a gesture Value and queue its validated plain CLAP projection.
    ///
    /// # Errors
    /// Invalid/stale tokens, invalid values, unavailable/full delivery, Busy or
    /// terminal publication leave state, authority and notification history intact.
    pub fn change(
        &mut self,
        token: &GestureToken,
        value: ControlValue,
    ) -> Result<(), ClapControlError> {
        let parameter = self.protocol.change_parameter(token)?;
        self.admit(1)?;
        let value = self.parameters.prepare_control(parameter, value)?;
        let _claim = self.outbox.try_claim().ok_or(ClapControlError::Busy)?;
        let revision = self
            .parameters
            .publish_control(parameter, value)
            .map_err(|_| ClapControlError::InstanceFaulted)?;
        self.outbox.queue.push_reserved(ControlNotice::Value {
            parameter,
            plain: value.plain,
            revision,
        });
        Ok(())
    }

    /// Close a matching gesture, even after replacement or value-revision failure.
    ///
    /// Ordinary queue fullness and an in-flight delivery cannot consume its
    /// reserved credit. This changes no canonical value and needs no delivery claim.
    ///
    /// # Errors
    /// Rejects foreign, ended or already-closed tokens without emitting an End.
    pub fn end(&mut self, token: &GestureToken) -> Result<(), ClapControlError> {
        let notice = self.protocol.end(token)?;
        queue_end_notice(self.reserved_ends, self.outbox, notice);
        Ok(())
    }

    /// Accept one discrete local edit with an atomic ordered Begin/Value/End batch.
    ///
    /// # Errors
    /// Rejects unknown/open parameters, invalid values, unavailable/full delivery,
    /// Busy or terminal publication before accepting any part of the operation.
    pub fn edit(
        &mut self,
        parameter: ParameterIndex,
        value: ControlValue,
    ) -> Result<(), ClapControlError> {
        self.protocol.ensure_idle(parameter)?;
        self.admit(3)?;
        let value = self.parameters.prepare_control(parameter, value)?;
        let _claim = self.outbox.try_claim().ok_or(ClapControlError::Busy)?;
        let revision = self
            .parameters
            .publish_control(parameter, value)
            .map_err(|_| ClapControlError::InstanceFaulted)?;
        self.outbox
            .queue
            .push_reserved(ControlNotice::Begin { parameter });
        self.outbox.queue.push_reserved(ControlNotice::Value {
            parameter,
            plain: value.plain,
            revision,
        });
        self.outbox
            .queue
            .push_reserved(ControlNotice::End { parameter });
        Ok(())
    }

    /// Explicitly close every live, stale or abandoned gesture, off the audio callback.
    ///
    /// Uses existing closure credits, even when ordinary admission is full or
    /// value publication has failed. Returns the number of queued Ends. No token
    /// destructor or future audio callback is responsible for this operation.
    pub fn close(&mut self) -> usize {
        close_protocol(self.protocol, self.reserved_ends, self.outbox)
    }

    /// Request another host-scheduled main-thread callback after this borrow ends.
    pub fn request_callback(&mut self) {
        *self.callback_requested = true;
    }

    fn healthy(&self) -> Result<(), ClapControlError> {
        self.parameters
            .check_healthy()
            .map_err(|_| ClapControlError::InstanceFaulted)
    }

    fn admit(&self, records: usize) -> Result<(), ClapControlError> {
        self.healthy()?;
        if !self.host_available {
            return Err(ClapControlError::HostUnavailable);
        }
        let needed = self
            .reserved_ends
            .checked_add(records)
            .ok_or(ClapControlError::Full)?;
        if self.outbox.queue.available() < needed {
            return Err(ClapControlError::Full);
        }
        Ok(())
    }
}

fn metadata_notice(notice: ControlNotification) -> ControlNotice {
    match notice {
        ControlNotification::Begin { parameter } => ControlNotice::Begin { parameter },
        ControlNotification::End { parameter } => ControlNotice::End { parameter },
        ControlNotification::Value { .. } => unreachable!("gesture metadata produces no Value"),
    }
}

fn close_protocol(
    protocol: &mut GestureProtocol,
    reserved: &mut usize,
    outbox: &ControlOutbox,
) -> usize {
    let notices = protocol.close();
    assert_eq!(
        notices.len(),
        *reserved,
        "gesture and closure-credit ownership agree"
    );
    for notice in &notices {
        queue_end_notice(reserved, outbox, *notice);
    }
    notices.len()
}

fn queue_end_notice(reserved: &mut usize, outbox: &ControlOutbox, notice: ControlNotification) {
    assert!(*reserved > 0, "accepted gesture retains End credit");
    *reserved -= 1;
    outbox.queue.push_reserved(metadata_notice(notice));
}

pub(crate) struct ControllerState<D> {
    controller: D,
    pub(crate) protocol: GestureProtocol,
    reserved_ends: usize,
    disabled: bool,
}

impl<D> ControllerState<D> {
    pub(crate) fn new<C: ClapComponent>(
        component: &C,
        parameter_count: usize,
    ) -> Result<Self, PluginError>
    where
        D: ClapController<C>,
    {
        Ok(Self {
            controller: D::new(component)?,
            protocol: GestureProtocol::new(parameter_count),
            reserved_ends: 0,
            disabled: false,
        })
    }

    pub(crate) fn run<C: ClapComponent>(
        &mut self,
        parameters: &ClapStateProjection,
        outbox: &ControlOutbox,
        host_available: bool,
    ) -> (bool, Option<Box<dyn Any + Send>>)
    where
        D: ClapController<C>,
    {
        if self.disabled {
            return (false, None);
        }
        if parameters.check_healthy().is_err() {
            self.fail(outbox);
            return (false, None);
        }
        let mut callback_requested = false;
        let mut controls = ClapControls {
            parameters,
            outbox,
            protocol: &mut self.protocol,
            reserved_ends: &mut self.reserved_ends,
            callback_requested: &mut callback_requested,
            host_available,
            _main_thread: PhantomData,
        };
        let result = catch_unwind(AssertUnwindSafe(|| {
            self.controller.on_main_thread(&mut controls);
        }));
        if let Err(payload) = result {
            self.fail(outbox);
            return (callback_requested, Some(payload));
        }
        if parameters.check_healthy().is_err() {
            self.fail(outbox);
        }
        (callback_requested, None)
    }

    pub(crate) fn fail(&mut self, outbox: &ControlOutbox) {
        self.disabled = true;
        close_protocol(&mut self.protocol, &mut self.reserved_ends, outbox);
    }
}

// Reject blocking mutation from ANY nested same-thread output callback. An
// instance pointer would miss A -> B -> A reentry and wait on A's suspended push.
thread_local! {
    static IN_HOST_OUTPUT: Cell<bool> = const { Cell::new(false) };
}

struct HostOutputScope(bool);
impl HostOutputScope {
    fn enter() -> Self {
        Self(IN_HOST_OUTPUT.with(|active| active.replace(true)))
    }
}
impl Drop for HostOutputScope {
    fn drop(&mut self) {
        IN_HOST_OUTPUT.with(|active| active.set(self.0));
    }
}

const CLAIMED: u8 = 1;
const MUTATION_WAITING: u8 = 2;

pub(crate) struct ControlOutbox {
    queue: NotificationQueue,
    claim_state: AtomicU8,
    callback_pending: AtomicBool,
    maximum_attempts: u32,
}

pub(crate) struct DeliveryClaim<'a>(&'a AtomicU8);
impl Drop for DeliveryClaim<'_> {
    fn drop(&mut self) {
        // A pending non-RT loader inherits the next turn, without exposing a
        // free state that the audio consumer could repeatedly reacquire.
        self.0.fetch_and(!CLAIMED, Ordering::Release);
    }
}

impl ControlOutbox {
    pub(crate) fn new(capacity: u32, maximum_attempts: u32) -> Result<Self, PluginError> {
        if capacity < 3 || maximum_attempts == 0 {
            return Err(PluginError::Message(
                "CLAP controller requires three notification credits and a positive delivery bound",
            ));
        }
        Ok(Self {
            queue: NotificationQueue::new(capacity).map_err(PluginError::from)?,
            claim_state: AtomicU8::new(0),
            callback_pending: AtomicBool::new(false),
            maximum_attempts,
        })
    }

    pub(crate) fn try_claim(&self) -> Option<DeliveryClaim<'_>> {
        self.claim_state
            .compare_exchange(0, CLAIMED, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| DeliveryClaim(&self.claim_state))
    }

    /// NON-REALTIME only. The caller first borrows its main-thread protocol, so
    /// a reentrant controller/load cannot register a second waiter on itself.
    /// Waiting depends on the in-flight host callback returning, not a deadline.
    pub(crate) fn claim_mutation(&self) -> Option<DeliveryClaim<'_>> {
        if IN_HOST_OUTPUT.with(Cell::get) {
            return None;
        }
        if self
            .claim_state
            .fetch_or(MUTATION_WAITING, Ordering::AcqRel)
            & MUTATION_WAITING
            != 0
        {
            return None;
        }
        loop {
            if self
                .claim_state
                .compare_exchange(
                    MUTATION_WAITING,
                    CLAIMED,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                // The successful handoff clears our waiter bit atomically. No
                // fallible work or callback occurs between registration and here.
                return Some(DeliveryClaim(&self.claim_state));
            }
            std::thread::yield_now();
        }
    }

    pub(crate) fn has_pending(&self) -> bool {
        !self.queue.is_empty()
    }

    pub(crate) fn begin_service(&self) {
        self.callback_pending.store(false, Ordering::Release);
    }

    pub(crate) fn request_callback(&self, host: HostSharedHandle<'_>) {
        if !self.callback_pending.swap(true, Ordering::AcqRel) {
            host.request_callback();
        }
    }

    pub(crate) fn drain(&self, parameters: &ClapStateProjection, output: &mut OutputEvents<'_>) {
        for _ in 0..self.maximum_attempts {
            let Some(_claim) = self.try_claim() else {
                return;
            };
            let Some(notice) = self.queue.peek() else {
                return;
            };
            let parameter = match notice {
                ControlNotice::Begin { parameter }
                | ControlNotice::Value { parameter, .. }
                | ControlNotice::End { parameter } => parameter,
            };
            if let ControlNotice::Value { revision, .. } = notice
                && parameters.notification_superseded(parameter, revision)
            {
                self.queue.pop();
                continue;
            }
            let index = usize::try_from(parameter.get()).expect("queued parameter fits usize");
            let id = parameters.bindings()[index].id();
            let _output_scope = HostOutputScope::enter();
            let result = match notice {
                ControlNotice::Begin { .. } => output.try_push(ParamGestureBeginEvent::new(0, id)),
                ControlNotice::Value { plain, .. } => {
                    output.try_push(ParamValueEvent::new(0, id, Pckn::match_all(), plain))
                }
                ControlNotice::End { .. } => output.try_push(ParamGestureEndEvent::new(0, id)),
            };
            if result.is_err() {
                return;
            }
            self.queue.pop();
        }
    }
}
