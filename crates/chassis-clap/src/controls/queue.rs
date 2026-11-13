//! Bounded, ordered product-to-host notices; never a latest-value mailbox.
//!
//! The main-thread owner serializes producer operations (including reentrancy)
//! and reserves End credits before accepting canonical edits. A delivery claim
//! serializes the consumer across process and active/inactive flush callbacks.
//! This queue does not implement those protocols or validate parameter domains.

use std::{
    collections::TryReserveError,
    error::Error,
    fmt,
    sync::atomic::{AtomicU8, AtomicU32, AtomicU64, AtomicUsize, Ordering},
};

use chassis_runtime::parameters::ParameterIndex;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ControlNotice {
    Begin {
        parameter: ParameterIndex,
    },
    Value {
        parameter: ParameterIndex,
        plain: f64,
        revision: u64,
    },
    End {
        parameter: ParameterIndex,
    },
}

#[derive(Debug)]
pub(crate) enum QueueSetupError {
    ZeroCapacity,
    CapacityUnrepresentable,
    Allocation(TryReserveError),
}

impl fmt::Display for QueueSetupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroCapacity => {
                formatter.write_str("notification queue capacity must be nonzero")
            }
            Self::CapacityUnrepresentable => {
                formatter.write_str("notification queue capacity plus sentinel does not fit usize")
            }
            Self::Allocation(error) => {
                write!(formatter, "notification queue allocation failed: {error}")
            }
        }
    }
}

impl Error for QueueSetupError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Allocation(error) => Some(error),
            Self::ZeroCapacity | Self::CapacityUnrepresentable => None,
        }
    }
}

const EMPTY: u8 = 0;
const BEGIN: u8 = 1;
const VALUE: u8 = 2;
const END: u8 = 3;

struct Slot {
    kind: AtomicU8,
    parameter: AtomicU32,
    plain: AtomicU64,
    revision: AtomicU64,
}

impl Slot {
    fn new() -> Self {
        Self {
            kind: AtomicU8::new(EMPTY),
            parameter: AtomicU32::new(0),
            plain: AtomicU64::new(0),
            revision: AtomicU64::new(0),
        }
    }

    // Only the producer writes a free slot; head Release publishes all fields.
    fn write(&self, notice: ControlNotice) {
        let (kind, parameter) = match notice {
            ControlNotice::Begin { parameter } => (BEGIN, parameter),
            ControlNotice::Value {
                parameter,
                plain,
                revision,
            } => {
                self.plain.store(plain.to_bits(), Ordering::Relaxed);
                self.revision.store(revision, Ordering::Relaxed);
                (VALUE, parameter)
            }
            ControlNotice::End { parameter } => (END, parameter),
        };
        self.parameter.store(parameter.get(), Ordering::Relaxed);
        self.kind.store(kind, Ordering::Relaxed);
    }

    // Only the consumer reads a published slot, after head Acquire and before
    // tail Release. EMPTY is initialization only and can never be published.
    fn read(&self) -> ControlNotice {
        let parameter = ParameterIndex::new(self.parameter.load(Ordering::Relaxed));
        match self.kind.load(Ordering::Relaxed) {
            BEGIN => ControlNotice::Begin { parameter },
            VALUE => ControlNotice::Value {
                parameter,
                plain: f64::from_bits(self.plain.load(Ordering::Relaxed)),
                revision: self.revision.load(Ordering::Relaxed),
            },
            END => ControlNotice::End { parameter },
            _ => unreachable!("a published notification slot must have a notice kind"),
        }
    }
}

/// One serialized producer and one serialized consumer, with setup-owned slots.
///
/// `head` is the next producer slot; `tail` is the retained consumer front.
/// Indices wrap within the allocation, with one slot always left empty: equal
/// indices mean empty, and advancing head to tail would mean full. Neither
/// index is a lifetime counter, so reuse never exhausts a serial number.
///
/// Payload writes precede head Release; head Acquire precedes payload reads.
/// The consumer leaves tail unchanged while inspecting a notice or calling the
/// host, so the producer cannot overwrite that slot even on host refusal. Only
/// after copying/using the notice does pop publish tail Release. The producer's
/// tail Acquire then permits reuse. This also orders old reads before the next
/// writes to that slot. Relaxed atomic payloads are coherent under this ownership
/// protocol; they do not make concurrent producers or consumers supported.
///
/// Consumer migration between active/inactive callbacks must establish the same
/// happens-before relationship as any other transfer of consumer ownership.
/// Push, peek, pop and credit checks do not allocate, deallocate or retry, and
/// notices own no resources. Queue construction/destruction belong off callback.
pub(crate) struct NotificationQueue {
    slots: Vec<Slot>,
    head: AtomicUsize,
    tail: AtomicUsize,
}

impl NotificationQueue {
    /// Allocate capacity actual records plus one sentinel slot, off callback.
    pub(crate) fn new(capacity: u32) -> Result<Self, QueueSetupError> {
        if capacity == 0 {
            return Err(QueueSetupError::ZeroCapacity);
        }
        let slot_count = usize::try_from(capacity)
            .ok()
            .and_then(|capacity| capacity.checked_add(1))
            .ok_or(QueueSetupError::CapacityUnrepresentable)?;
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(slot_count)
            .map_err(QueueSetupError::Allocation)?;
        slots.resize_with(slot_count, Slot::new);
        Ok(Self {
            slots,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        })
    }

    /// Producer-only free record count; does not subtract caller-owned End debt.
    ///
    /// A stale tail can only undercount free records. The serialized producer
    /// must check `available >= reserved_ends + needed` before accepting edits.
    /// Consumer progress can only add credits between this check and pushes.
    pub(crate) fn available(&self) -> usize {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        if tail <= head {
            (self.slots.len() - 1) - (head - tail)
        } else {
            tail - head - 1
        }
    }

    /// Producer-only, infallible after reserving a free record before acceptance.
    ///
    /// Panics on a broken internal credit/serialization invariant, never returns
    /// a normal full-queue error after canonical state has already changed.
    pub(crate) fn push_reserved(&self, notice: ControlNotice) {
        let head = self.head.load(Ordering::Relaxed);
        let next = self.next(head);
        assert_ne!(
            next,
            self.tail.load(Ordering::Acquire),
            "notification push requires a previously reserved free record"
        );
        self.slots[head].write(notice);
        self.head.store(next, Ordering::Release);
    }

    /// Consumer-only copied front; retains the slot and consumes no credit.
    ///
    /// Do not pop on host refusal. Repeated peeks return the same notice until
    /// the sole consumer explicitly removes it, regardless of producer appends.
    pub(crate) fn peek(&self) -> Option<ControlNotice> {
        let tail = self.tail.load(Ordering::Relaxed);
        if tail == self.head.load(Ordering::Acquire) {
            None
        } else {
            Some(self.slots[tail].read())
        }
    }

    /// Consumer-only release after a successful peek and completed notice use.
    /// Stale notices may also be explicitly discarded by their delivery owner.
    pub(crate) fn pop(&self) {
        let tail = self.tail.load(Ordering::Relaxed);
        assert_ne!(
            tail,
            self.head.load(Ordering::Acquire),
            "notification pop requires a retained front notice"
        );
        // Do not clear payload/kind: releasing tail is the sole ownership change.
        self.tail.store(self.next(tail), Ordering::Release);
    }

    /// A work hint, not a reservation or permission to inspect a payload.
    pub(crate) fn is_empty(&self) -> bool {
        self.tail.load(Ordering::Acquire) == self.head.load(Ordering::Acquire)
    }

    fn next(&self, index: usize) -> usize {
        // index < slots.len(), so index + 1 cannot overflow even at usize::MAX.
        let next = index + 1;
        if next == self.slots.len() { 0 } else { next }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{panic::catch_unwind, thread};

    const A: ParameterIndex = ParameterIndex::new(0);
    const B: ParameterIndex = ParameterIndex::new(1);

    fn value(parameter: ParameterIndex, plain: f64, revision: u64) -> ControlNotice {
        ControlNotice::Value {
            parameter,
            plain,
            revision,
        }
    }

    fn take(queue: &NotificationQueue) -> ControlNotice {
        let notice = queue.peek().expect("a queued notice");
        queue.pop();
        notice
    }

    #[test]
    fn setup_rejects_zero_but_supports_one_actual_record() {
        let error = NotificationQueue::new(0).err().expect("zero is invalid");
        assert!(matches!(error, QueueSetupError::ZeroCapacity));
        let queue = NotificationQueue::new(1).unwrap();
        assert_eq!(queue.available(), 1);
        assert_eq!(queue.peek(), None);
        queue.push_reserved(ControlNotice::Begin { parameter: A });
        assert_eq!(queue.available(), 0);
        assert_eq!(take(&queue), ControlNotice::Begin { parameter: A });
        assert_eq!(queue.available(), 1);
    }

    #[test]
    fn mixed_parameters_and_equal_values_preserve_every_notice_in_source_order() {
        let notices = [
            ControlNotice::Begin { parameter: A },
            value(A, 0.25, 13),
            ControlNotice::Begin { parameter: B },
            value(B, 0.5, 14),
            value(A, 0.25, 15),
            ControlNotice::End { parameter: B },
            value(A, -1.0, 16),
            ControlNotice::End { parameter: A },
        ];
        let queue = NotificationQueue::new(u32::try_from(notices.len()).unwrap()).unwrap();
        for notice in notices {
            queue.push_reserved(notice);
        }
        assert_eq!(queue.available(), 0);
        for expected in notices {
            assert_eq!(take(&queue), expected);
        }
        assert_eq!(queue.peek(), None);
        assert_eq!(queue.available(), notices.len());
    }

    #[test]
    fn overbooking_panics_before_overwriting_any_queued_record() {
        let queue = NotificationQueue::new(2).unwrap();
        queue.push_reserved(value(A, 1.0, 1));
        queue.push_reserved(value(B, 2.0, 2));
        assert_eq!(queue.available(), 0);
        assert!(catch_unwind(|| queue.push_reserved(value(A, 3.0, 3))).is_err());
        assert_eq!(queue.available(), 0);
        assert_eq!(take(&queue), value(A, 1.0, 1));
        assert_eq!(take(&queue), value(B, 2.0, 2));
        assert_eq!(queue.peek(), None);
        assert_eq!(queue.available(), 2);
    }

    #[test]
    fn refused_front_and_its_suffix_survive_peeks_appends_and_wrap() {
        let queue = NotificationQueue::new(3).unwrap();
        queue.push_reserved(ControlNotice::Begin { parameter: A });
        queue.push_reserved(value(A, 0.1, 1));
        for _ in 0..8 {
            assert_eq!(queue.peek(), Some(ControlNotice::Begin { parameter: A }));
            assert_eq!(queue.available(), 1);
        }
        queue.pop(); // Host finally accepted Begin.
        queue.push_reserved(value(B, 0.2, 2));
        queue.push_reserved(ControlNotice::End { parameter: A }); // Head wraps.
        for _ in 0..8 {
            assert_eq!(queue.peek(), Some(value(A, 0.1, 1)));
            assert_eq!(queue.available(), 0);
        }
        assert_eq!(take(&queue), value(A, 0.1, 1));
        assert_eq!(take(&queue), value(B, 0.2, 2));
        assert_eq!(take(&queue), ControlNotice::End { parameter: A });
        assert_eq!(queue.peek(), None);
    }

    #[test]
    fn value_payload_preserves_all_bits_without_domain_validation() {
        let queue = NotificationQueue::new(1).unwrap();
        for bits in [0, 1, 0x8000_0000_0000_0000, 0x7ff8_1234_5678_9abc, u64::MAX] {
            queue.push_reserved(value(
                ParameterIndex::new(u32::MAX),
                f64::from_bits(bits),
                u64::MAX,
            ));
            let ControlNotice::Value {
                parameter,
                plain,
                revision,
            } = take(&queue)
            else {
                panic!("a value notice must retain its kind");
            };
            assert_eq!(parameter.get(), u32::MAX);
            assert_eq!(plain.to_bits(), bits);
            assert_eq!(revision, u64::MAX);
        }
    }

    #[test]
    fn indices_wrap_with_partially_occupied_and_full_rings_many_times() {
        for capacity in [1, 2, 3, 7] {
            let queue = NotificationQueue::new(capacity).unwrap();
            for cycle in 0..10_000_u64 {
                for offset in 0..capacity {
                    queue.push_reserved(value(A, f64::from(offset), cycle));
                    assert_eq!(
                        queue.available(),
                        usize::try_from(capacity - offset - 1).unwrap()
                    );
                }
                for offset in 0..capacity {
                    assert_eq!(take(&queue), value(A, f64::from(offset), cycle));
                }
                // A full batch plus two individual records advances by one
                // modulo capacity + 1, visiting every ring position over time.
                for _ in 0..2 {
                    queue.push_reserved(ControlNotice::End { parameter: B });
                    assert_eq!(take(&queue), ControlNotice::End { parameter: B });
                }
                assert_eq!(queue.peek(), None);
                assert_eq!(queue.available(), usize::try_from(capacity).unwrap());
            }
        }
    }

    // Deliberately tiny test-only credit owner, not a second production protocol.
    // Rejected admissions must not change canonical values or the gesture ledger.
    struct Producer {
        queue: NotificationQueue,
        reserved_ends: usize,
        open: [bool; 2],
        canonical: [f64; 2],
        revision: u64,
    }

    impl Producer {
        fn new(capacity: u32) -> Self {
            Self {
                queue: NotificationQueue::new(capacity).unwrap(),
                reserved_ends: 0,
                open: [false; 2],
                canonical: [0.0; 2],
                revision: 0,
            }
        }

        fn begin(&mut self, parameter: ParameterIndex) -> bool {
            let index = usize::try_from(parameter.get()).unwrap();
            if self.open[index] || self.queue.available() < self.reserved_ends + 2 {
                return false;
            }
            self.open[index] = true;
            self.reserved_ends += 1;
            self.queue.push_reserved(ControlNotice::Begin { parameter });
            true
        }

        fn change(&mut self, parameter: ParameterIndex, plain: f64) -> bool {
            let index = usize::try_from(parameter.get()).unwrap();
            if !self.open[index] || self.queue.available() < self.reserved_ends + 1 {
                return false;
            }
            self.canonical[index] = plain;
            self.revision += 1;
            self.queue
                .push_reserved(value(parameter, plain, self.revision));
            true
        }

        fn edit(&mut self, parameter: ParameterIndex, plain: f64) -> bool {
            let index = usize::try_from(parameter.get()).unwrap();
            if self.open[index] || self.queue.available() < self.reserved_ends + 3 {
                return false;
            }
            self.canonical[index] = plain;
            self.revision += 1;
            self.queue.push_reserved(ControlNotice::Begin { parameter });
            self.queue
                .push_reserved(value(parameter, plain, self.revision));
            self.queue.push_reserved(ControlNotice::End { parameter });
            true
        }

        fn end(&mut self, parameter: ParameterIndex) {
            let index = usize::try_from(parameter.get()).unwrap();
            assert!(self.open[index]);
            self.open[index] = false;
            self.reserved_ends -= 1;
            self.queue.push_reserved(ControlNotice::End { parameter });
        }

        fn close(&mut self) {
            for parameter in [A, B] {
                if self.open[usize::try_from(parameter.get()).unwrap()] {
                    self.end(parameter);
                }
            }
        }
    }

    #[test]
    fn admission_reserves_end_credit_before_changing_canonical_state() {
        let mut producer = Producer::new(3);
        assert!(producer.begin(A));
        assert!(producer.change(A, 0.5));
        assert!(!producer.change(A, 0.75));
        assert!(!producer.begin(B));
        assert!(!producer.edit(B, 1.0));
        assert_eq!(
            producer.canonical.map(f64::to_bits),
            [0.5_f64.to_bits(), 0.0_f64.to_bits()]
        );
        assert_eq!(producer.open, [true, false]);
        assert_eq!(producer.revision, 1);
        assert_eq!(producer.reserved_ends, 1);
        assert_eq!(producer.queue.available(), 1); // Reserved, not ordinary credit.
        producer.end(A); // Guaranteed even with no ordinary credit left.
        assert_eq!(producer.queue.available(), 0);
        assert_eq!(producer.reserved_ends, 0);
        assert_eq!(take(&producer.queue), ControlNotice::Begin { parameter: A });
        assert_eq!(take(&producer.queue), value(A, 0.5, 1));
        assert_eq!(take(&producer.queue), ControlNotice::End { parameter: A });
        assert!(producer.edit(B, 1.0));
        assert_eq!(take(&producer.queue), ControlNotice::Begin { parameter: B });
        assert_eq!(take(&producer.queue), value(B, 1.0, 2));
        assert_eq!(take(&producer.queue), ControlNotice::End { parameter: B });
    }

    #[test]
    fn close_consumes_all_reserved_ends_when_ordinary_credit_is_exhausted() {
        let mut producer = Producer::new(4);
        assert!(producer.begin(A));
        assert!(producer.begin(B));
        assert!(!producer.change(A, 1.0));
        assert!(!producer.change(B, 2.0));
        assert_eq!(producer.queue.available(), producer.reserved_ends);
        producer.close();
        assert_eq!(producer.queue.available(), 0);
        assert_eq!(producer.reserved_ends, 0);
        assert_eq!(producer.open, [false; 2]);
        for expected in [
            ControlNotice::Begin { parameter: A },
            ControlNotice::Begin { parameter: B },
            ControlNotice::End { parameter: A },
            ControlNotice::End { parameter: B },
        ] {
            assert_eq!(take(&producer.queue), expected);
        }
        assert_eq!(producer.queue.peek(), None);
    }

    fn concurrent_notice(sequence: u32) -> ControlNotice {
        let parameter = ParameterIndex::new(sequence.wrapping_mul(0x9e37_79b9));
        match sequence % 3 {
            0 => ControlNotice::Begin { parameter },
            1 => value(
                parameter,
                f64::from_bits(
                    0x3ff0_0000_0000_0000
                        | (u64::from(sequence).wrapping_mul(0x1234_5678_9abc)
                            & 0x000f_ffff_ffff_ffff),
                ),
                u64::from(sequence).rotate_left(32) ^ 0xa5a5_5a5a_1234_5678,
            ),
            _ => ControlNotice::End { parameter },
        }
    }

    #[test]
    fn concurrent_producer_consumer_preserve_order_and_coherent_payloads() {
        const COUNT: u32 = 50_000;
        const MAX_POLLS: usize = 5_000_000;
        for capacity in [1, 7] {
            let queue = NotificationQueue::new(capacity).unwrap();
            thread::scope(|scope| {
                let producer = scope.spawn(|| {
                    let mut sent = 0;
                    for _ in 0..MAX_POLLS {
                        if queue.available() != 0 {
                            queue.push_reserved(concurrent_notice(sent));
                            sent += 1;
                            if sent == COUNT {
                                return;
                            }
                        } else {
                            thread::yield_now();
                        }
                    }
                    panic!("producer did not finish within the bounded polling budget: {sent}");
                });
                let mut received = 0;
                for _ in 0..MAX_POLLS {
                    if let Some(notice) = queue.peek() {
                        assert_eq!(notice, concurrent_notice(received));
                        // Simulate a refusal first; even concurrent appends cannot
                        // replace the retained notice or release its credit.
                        assert_eq!(queue.peek(), Some(notice));
                        queue.pop();
                        received += 1;
                        if received == COUNT {
                            break;
                        }
                    } else {
                        thread::yield_now();
                    }
                }
                assert_eq!(received, COUNT, "consumer exhausted its polling budget");
                producer
                    .join()
                    .expect("producer must complete without panic");
            });
            assert_eq!(queue.peek(), None);
            assert_eq!(queue.available(), usize::try_from(capacity).unwrap());
        }
    }
}
