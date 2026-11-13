use std::sync::{Arc, atomic::Ordering, mpsc::sync_channel};

use super::*;

#[test]
fn registered_loader_inherits_claim_before_new_delivery_or_scalar_edit() {
    let outbox = Arc::new(ControlOutbox::new(3, 3).unwrap());
    let delivery = outbox.try_claim().unwrap();
    let (acquired, ready) = sync_channel(1);
    let (release, resume) = sync_channel(1);
    std::thread::scope(|scope| {
        let waiting = Arc::clone(&outbox);
        let loader = scope.spawn(move || {
            let claim = waiting.claim_mutation().unwrap();
            acquired.send(()).unwrap();
            resume.recv().unwrap();
            drop(claim);
        });
        while outbox.claim_state.load(Ordering::Acquire) & MUTATION_WAITING == 0 {
            std::thread::yield_now();
        }
        assert!(outbox.try_claim().is_none());
        drop(delivery);
        ready.recv().unwrap();
        assert_eq!(outbox.claim_state.load(Ordering::Acquire), CLAIMED);
        assert!(outbox.try_claim().is_none());
        release.send(()).unwrap();
        loader.join().unwrap();
    });
    assert_eq!(outbox.claim_state.load(Ordering::Acquire), 0);
    assert!(outbox.try_claim().is_some());
}

#[test]
fn nested_output_scopes_reject_blocking_load_on_every_instance() {
    let a = ControlOutbox::new(3, 3).unwrap();
    let b = ControlOutbox::new(3, 3).unwrap();
    let a_claim = a.try_claim().unwrap();
    let a_output = HostOutputScope::enter();
    {
        let _b_claim = b.try_claim().unwrap();
        let _b_output = HostOutputScope::enter();
        // A -> B -> A must not lose the suspended A claim beneath B's scope.
        assert!(a.claim_mutation().is_none());
        assert!(b.claim_mutation().is_none());
    }
    assert!(a.claim_mutation().is_none());
    assert_eq!(a.claim_state.load(Ordering::Acquire), CLAIMED);
    drop(a_output);
    drop(a_claim);
    assert!(a.claim_mutation().is_some());
    assert!(b.claim_mutation().is_some());
}

#[test]
fn failure_closes_live_end_debt_after_all_queued_records_were_delivered() {
    let descriptors =
        [
            chassis_runtime::parameters::ParameterDescriptor::float("gain", "Gain", 0.0, 1.0, 0.5)
                .unwrap(),
        ];
    let parameters = ClapStateProjection::new(&descriptors, &[("gain", 1)]).unwrap();
    let outbox = ControlOutbox::new(3, 3).unwrap();
    let mut state = ControllerState {
        controller: (),
        protocol: GestureProtocol::new(1),
        reserved_ends: 0,
        disabled: false,
    };
    let mut callback = false;
    let token = {
        let mut controls = ClapControls {
            parameters: &parameters,
            outbox: &outbox,
            protocol: &mut state.protocol,
            reserved_ends: &mut state.reserved_ends,
            callback_requested: &mut callback,
            host_available: true,
            _main_thread: PhantomData,
        };
        controls.begin(ParameterIndex::new(0)).unwrap()
    };
    assert!(matches!(
        outbox.queue.peek(),
        Some(ControlNotice::Begin { .. })
    ));
    outbox.queue.pop();
    assert!(!outbox.has_pending());
    state.fail(&outbox);
    assert!(state.disabled);
    assert_eq!(state.reserved_ends, 0);
    assert_eq!(
        state.protocol.end(&token),
        Err(ControlError::InvalidGesture)
    );
    assert_eq!(
        outbox.queue.peek(),
        Some(ControlNotice::End {
            parameter: ParameterIndex::new(0)
        })
    );
    state.fail(&outbox);
    outbox.queue.pop();
    assert!(!outbox.has_pending());
}
