//! Finite weak-memory qualification of `controls/queue.rs` and the delivery
//! claim in `controls.rs`, not a replacement implementation or a queue quota test.
//!
//! Queue: one main-thread-serialized producer and one CLAP-serialized consumer,
//! independent of which callback role supplies that consumer. Capacity is one or
//! two records plus an empty sentinel. Three producer attempts and two consumer
//! attempts never poll/retry on full/empty; sequence numbers advance only on
//! successful admission. Fixed yields between attempts expose reuse within the
//! scheduling bound without adding happens-before. Joining transfers consumer
//! ownership before final drain.
//! Slots contain only copied notice fields, not resources requiring reclamation.
//!
//! Claim: one non-reentrant main-thread loader and one single-try RT consumer;
//! a failed loader take joins the current holder before one final take, replacing
//! the production NON-RT wait loop with a finite handoff. The consumer retains
//! its claim through the simulated host offer. Only one queued local Value at
//! revision zero and one accepted full load at cutoff one are modeled. Later
//! local edits are NOT superseding operations; external points, publication
//! seqlocks, ticket allocation/exhaustion, and product validation are not modeled.
//!
//! All concurrent explorations have a two-preemption bound, with no iteration,
//! branch, or time truncation. These are bounded safety checks, not fairness,
//! starvation, native allocation, or format-conformance proofs. Main-thread
//! metadata borrowing and `HostOutputScope` reject reentrant/nested loads before
//! I/O in production; std TLS is deliberately left to native/unit qualification.

use loom::{
    sync::{
        Arc,
        atomic::{AtomicU8, AtomicU32, AtomicU64, AtomicUsize, Ordering},
    },
    thread,
};
use std::sync::{
    Arc as WitnessArc,
    atomic::{AtomicBool as WitnessBool, Ordering as WitnessOrdering},
};

const BEGIN: u8 = 1;
const VALUE: u8 = 2;
const END: u8 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Notice {
    Begin(u32),
    Value(u32, u64, u64),
    End(u32),
}

fn notice(sequence: usize) -> Notice {
    let sequence = u32::try_from(sequence).expect("finite attempt count fits u32");
    let parameter = sequence.wrapping_mul(0x9e37_79b9) ^ 0xabcd_1234;
    if sequence % 2 == 1 {
        Notice::End(parameter)
    } else {
        Notice::Value(
            parameter,
            0x3ff0_0000_0000_0000 ^ (u64::from(sequence) << 20),
            0xa5a5_1234_5678_0000 ^ u64::from(sequence),
        )
    }
}

struct Slot {
    kind: AtomicU8,
    parameter: AtomicU32,
    plain: AtomicU64,
    revision: AtomicU64,
}

impl Slot {
    fn new() -> Self {
        Self {
            kind: AtomicU8::new(0),
            parameter: AtomicU32::new(0),
            plain: AtomicU64::new(0),
            revision: AtomicU64::new(0),
        }
    }

    fn write(&self, notice: Notice) {
        let (kind, parameter) = match notice {
            Notice::Begin(parameter) => (BEGIN, parameter),
            Notice::Value(parameter, plain, revision) => {
                self.plain.store(plain, Ordering::Relaxed);
                self.revision.store(revision, Ordering::Relaxed);
                (VALUE, parameter)
            }
            Notice::End(parameter) => (END, parameter),
        };
        self.parameter.store(parameter, Ordering::Relaxed);
        self.kind.store(kind, Ordering::Relaxed);
    }

    fn read(&self) -> Notice {
        // Production reads parameter BEFORE kind, then Value payload fields.
        let parameter = self.parameter.load(Ordering::Relaxed);
        match self.kind.load(Ordering::Relaxed) {
            BEGIN => Notice::Begin(parameter),
            VALUE => Notice::Value(
                parameter,
                self.plain.load(Ordering::Relaxed),
                self.revision.load(Ordering::Relaxed),
            ),
            END => Notice::End(parameter),
            kind => panic!("published notice has invalid kind {kind}"),
        }
    }
}

struct Queue {
    slots: Vec<Slot>,
    head: AtomicUsize,
    tail: AtomicUsize,
}

impl Queue {
    fn new(capacity: usize) -> Self {
        Self {
            slots: (0..=capacity).map(|_| Slot::new()).collect(),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    fn next(&self, index: usize) -> usize {
        let next = index + 1;
        if next == self.slots.len() { 0 } else { next }
    }

    fn try_push(&self, notice: Notice) -> bool {
        // Mirror available() admission, then push_reserved(), including its
        // second tail Acquire. Full is an allowed rejected model operation;
        // production rejects admission BEFORE accepting a canonical edit.
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        let available = if tail <= head {
            (self.slots.len() - 1) - (head - tail)
        } else {
            tail - head - 1
        };
        if available == 0 {
            return false;
        }
        let head = self.head.load(Ordering::Relaxed);
        let next = self.next(head);
        assert_ne!(next, self.tail.load(Ordering::Acquire));
        self.slots[head].write(notice);
        self.head.store(next, Ordering::Release);
        true
    }

    fn peek(&self) -> Option<Notice> {
        let tail = self.tail.load(Ordering::Relaxed);
        if tail == self.head.load(Ordering::Acquire) {
            None
        } else {
            Some(self.slots[tail].read())
        }
    }

    fn pop(&self) {
        let tail = self.tail.load(Ordering::Relaxed);
        assert_ne!(tail, self.head.load(Ordering::Acquire));
        self.tail.store(self.next(tail), Ordering::Release);
    }
}

fn bounded_model() -> loom::model::Builder {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(2);
    builder
}

#[test]
fn spsc_reuse_preserves_order_and_every_copied_field() {
    for begin_first in [false, true] {
        // Outside-Loom witnesses are written only AFTER joining the consumer.
        // They record traces, never synchronize the modeled actors.
        let reused = WitnessArc::new(WitnessBool::new(false));
        let witness = WitnessArc::clone(&reused);
        bounded_model().check(move || {
            let queue = Arc::new(Queue::new(1)); // Two physical slots, one empty.
            // Test-only relaxed evidence of completed copies before pop. This
            // adds no synchronization: the tail handoff must make it visible
            // before reuse. Atomic payloads alone need not expose a missing
            // reads-before-next-writes edge as a torn value in Loom.
            let copies_completed = Arc::new(AtomicUsize::new(0));
            let mut records = [notice(0), notice(1), notice(2)];
            if begin_first {
                records[0] = Notice::Begin(0x1234_5678);
            }
            let worker = {
                let queue = Arc::clone(&queue);
                let copies_completed = Arc::clone(&copies_completed);
                thread::spawn(move || {
                    let mut consumed = Vec::new();
                    for _ in 0..2 {
                        if let Some(front) = queue.peek() {
                            assert_eq!(front, records[consumed.len()]);
                            // Inspection retains ownership until acceptance.
                            assert_eq!(queue.peek(), Some(front));
                            consumed.push(front);
                            copies_completed.store(consumed.len(), Ordering::Relaxed);
                            queue.pop();
                        }
                        thread::yield_now();
                    }
                    consumed
                })
            };
            let mut accepted = Vec::new();
            for _ in 0..3 {
                let next = records[accepted.len()];
                if queue.try_push(next) {
                    accepted.push(next);
                    if accepted.len() == 3 {
                        assert_eq!(
                            copies_completed.load(Ordering::Relaxed),
                            2,
                            "tail handoff must publish completed copies before reuse"
                        );
                    }
                }
                thread::yield_now();
            }
            let consumed = worker.join().expect("sole consumer must finish");
            if accepted.len() == 3 {
                // Slots 0, 1, 0: either Value -> distinct Value (all fields),
                // or Begin -> Value (kind changes), not just index wrap.
                witness.store(true, WitnessOrdering::Relaxed);
            }
            check_history(&queue, &accepted, &consumed);
        });
        assert!(
            reused.load(WitnessOrdering::Relaxed),
            "exploration must exercise slot reuse"
        );
    }
}

fn check_history(queue: &Queue, accepted: &[Notice], consumed: &[Notice]) {
    assert!(consumed.len() <= accepted.len());
    assert_eq!(consumed, &accepted[..consumed.len()]);
    let mut remaining = Vec::new();
    // Bound exhaustion by allocation size, NOT cyclic head/tail values. This
    // runs only after join, with the consumer role transferred to this thread.
    for _ in 0..queue.slots.len() {
        if let Some(front) = queue.peek() {
            remaining.push(front);
            queue.pop();
        } else {
            break;
        }
    }
    assert_eq!(queue.peek(), None, "finite drain must exhaust the queue");
    assert_eq!(remaining, &accepted[consumed.len()..]);
}

#[test]
fn refused_front_survives_concurrent_appends_without_coalescing() {
    bounded_model().check(|| {
        let queue = Arc::new(Queue::new(2)); // Three physical slots.
        let first = Notice::Begin(0x1234_5678);
        assert!(queue.try_push(first));
        let worker = {
            let queue = Arc::clone(&queue);
            thread::spawn(move || {
                let mut consumed = Vec::new();
                for attempt in 0..2 {
                    assert_eq!(queue.peek(), Some(first));
                    if attempt == 1 {
                        consumed.push(first);
                        queue.pop(); // First offer refused; second accepted.
                    }
                }
                consumed
            })
        };
        let mut accepted = vec![first];
        for _ in 0..3 {
            let next = notice(accepted.len() - 1);
            if queue.try_push(next) {
                accepted.push(next);
            }
        }
        let consumed = worker.join().expect("sole consumer must finish");
        check_history(&queue, &accepted, &consumed);
    });
}

const CLAIMED: u8 = 1;
const WAITING: u8 = 2;

struct ClaimState(AtomicU8);
struct Claim<'a>(&'a AtomicU8);

impl Drop for Claim<'_> {
    fn drop(&mut self) {
        self.0.fetch_and(!CLAIMED, Ordering::Release);
    }
}

impl ClaimState {
    fn new() -> Self {
        Self(AtomicU8::new(0))
    }

    fn try_claim(&self) -> Option<Claim<'_>> {
        self.0
            .compare_exchange(0, CLAIMED, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Claim(&self.0))
    }

    fn register_mutation(&self) -> bool {
        self.0.fetch_or(WAITING, Ordering::AcqRel) & WAITING == 0
    }

    fn try_take_mutation(&self) -> Option<Claim<'_>> {
        self.0
            .compare_exchange(WAITING, CLAIMED, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Claim(&self.0))
    }
}

#[derive(Debug)]
enum DeliveryAttempt {
    Busy,
    Superseded,
    Offered(usize),
}

struct Delivery {
    claim: ClaimState,
    cutoff: AtomicU64,
    // Oracle ONLY: Relaxed RMW modification order records acceptance vs offer
    // without adding happens-before to cutoff publication or claim acquisition.
    history: AtomicUsize,
}

impl Delivery {
    fn accept_load(&self) -> usize {
        self.cutoff.store(1, Ordering::Release);
        self.history.fetch_add(1, Ordering::Relaxed)
    }

    fn deliver(&self) -> DeliveryAttempt {
        let Some(_claim) = self.claim.try_claim() else {
            return DeliveryAttempt::Busy;
        };
        if self.cutoff.load(Ordering::Acquire) > 0 {
            return DeliveryAttempt::Superseded; // Discard revision-zero Value.
        }
        // Actual host try_push is inside the claim, not after guard drop.
        DeliveryAttempt::Offered(self.history.fetch_add(1, Ordering::Relaxed))
    }
}

#[test]
fn accepted_load_before_delivery_claim_prevents_stale_host_offer() {
    let load_first = WitnessArc::new(WitnessBool::new(false));
    let delivery_first = WitnessArc::new(WitnessBool::new(false));
    let saw_load_first = WitnessArc::clone(&load_first);
    let saw_delivery_first = WitnessArc::clone(&delivery_first);
    bounded_model().check(move || {
        let delivery = Arc::new(Delivery {
            claim: ClaimState::new(),
            cutoff: AtomicU64::new(0),
            history: AtomicUsize::new(0),
        });
        let consumer = {
            let delivery = Arc::clone(&delivery);
            thread::spawn(move || delivery.deliver())
        };
        assert!(delivery.claim.register_mutation());
        let (accepted, offered) = if let Some(claim) = delivery.claim.try_take_mutation() {
            let accepted = delivery.accept_load();
            drop(claim);
            // No join/extra synchronization precedes the consumer's cutoff read
            // in the important load-first schedule: only the real claim does.
            (accepted, consumer.join().expect("RT attempt must finish"))
        } else {
            let offered = consumer.join().expect("current RT holder must finish");
            let claim = delivery
                .claim
                .try_take_mutation()
                .expect("holder released to waiter");
            let accepted = delivery.accept_load();
            drop(claim);
            (accepted, offered)
        };
        match offered {
            DeliveryAttempt::Offered(offered) => {
                assert!(
                    offered < accepted,
                    "stale Value offered after an accepted full load"
                );
                saw_delivery_first.store(true, WitnessOrdering::Relaxed);
            }
            DeliveryAttempt::Superseded => {
                saw_load_first.store(true, WitnessOrdering::Relaxed);
            }
            DeliveryAttempt::Busy => {}
        }
    });
    assert!(
        load_first.load(WitnessOrdering::Relaxed),
        "explore load then acquired consumer"
    );
    assert!(
        delivery_first.load(WitnessOrdering::Relaxed),
        "explore consumer before load"
    );
}

#[test]
fn waiting_mutation_inherits_release_without_an_rt_reacquisition_gap() {
    bounded_model().check(|| {
        let state = Arc::new(ClaimState::new());
        let rt_claim = state.try_claim().expect("initial RT acquisition");
        let registration = {
            let state = Arc::clone(&state);
            thread::spawn(move || {
                assert!(state.register_mutation());
                assert!(state.try_take_mutation().is_none());
                // A second loader is Busy, and obtains NO release authority.
                assert!(!state.register_mutation());
            })
        };
        // Exact finite handoff: join only the registration, NOT the RT holder.
        // It transfers the registered loader role back to this thread.
        registration
            .join()
            .expect("waiter registration must finish");
        assert!(state.try_claim().is_none());
        drop(rt_claim);
        assert_eq!(state.0.load(Ordering::Relaxed), WAITING);
        assert!(
            state.try_claim().is_none(),
            "RT cannot steal the waiter's turn"
        );
        let mutation = state
            .try_take_mutation()
            .expect("waiter inherits released claim");
        assert_eq!(
            state.0.load(Ordering::Relaxed),
            CLAIMED,
            "take clears WAITING"
        );
        assert!(state.try_claim().is_none());
        drop(mutation);
        assert!(state.try_claim().is_some());
    });
}
