//! Weak-memory model of the CLAP composed desired-state publication protocol.
//!
//! This must mirror the production fields and memory orders: independent
//! root/ingress seqlocks, Release/Acquire atomic payloads, and one `AcqRel` ticket
//! allocator shared by both sources. Integer values stand in for f64 bits. Root
//! writers are control-serialized; process/flush serialize ingress writers under
//! CLAP. The competing completion test deliberately exercises their exact-token
//! CAS boundary, not permission for concurrent CLAP callbacks.
//!
//! Reads make one attempt (production makes eight bounded attempts). No read
//! advances epochs, allocates tickets, or compacts ingress. Root acquisition is
//! also one attempt: these tests have no competing root writers. Exhaustion and
//! source-guard fault handling belong to production unit tests, not this model.

use loom::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    thread,
};

const INITIAL_VALUES: [u64; 2] = [10, 20];
const ROOT_VALUES: [u64; 2] = [11, 21];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Generation {
    root: u64,
    ingress: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Snapshot {
    generation: Generation,
    custom_generation: u64,
    values: [u64; 2],
}

struct PublicationModel {
    root_seq: AtomicU64,
    root_values: [AtomicU64; 2],
    root_ticket: AtomicU64,
    custom_generation: AtomicU64,
    ingress_seq: AtomicU64,
    ingress_values: [AtomicU64; 2],
    ingress_tickets: [AtomicU64; 2],
    operation_ticket: AtomicU64,
}

impl PublicationModel {
    fn new() -> Self {
        Self {
            root_seq: AtomicU64::new(0),
            root_values: INITIAL_VALUES.map(AtomicU64::new),
            root_ticket: AtomicU64::new(0),
            custom_generation: AtomicU64::new(0),
            ingress_seq: AtomicU64::new(0),
            ingress_values: INITIAL_VALUES.map(AtomicU64::new),
            ingress_tickets: [AtomicU64::new(0), AtomicU64::new(0)],
            // Zero denotes no operation. Every reservation is after source CAS.
            operation_ticket: AtomicU64::new(0),
        }
    }

    fn try_begin_source(seq: &AtomicU64, expected: u64) -> bool {
        expected.is_multiple_of(2)
            && seq
                .compare_exchange(expected, expected + 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
    }

    fn reserve_ticket(&self) -> u64 {
        self.operation_ticket.fetch_add(2, Ordering::AcqRel) + 2
    }

    // Split at real instruction boundaries to force adversarial interleavings
    // without adding synchronization that would mask weak-memory behavior.
    fn begin_root(&self) -> Option<(u64, u64)> {
        let old = self.root_seq.load(Ordering::Acquire);
        Self::try_begin_source(&self.root_seq, old).then(|| (old, self.reserve_ticket()))
    }

    fn finish_root(&self, old: u64, ticket: u64, values: [u64; 2], custom_changed: bool) {
        for (slot, value) in self.root_values.iter().zip(values) {
            slot.store(value, Ordering::Release);
        }
        self.root_ticket.store(ticket, Ordering::Release);
        if custom_changed {
            self.custom_generation.store(ticket, Ordering::Release);
        }
        self.root_seq.store(old + 2, Ordering::Release);
    }

    fn publish_root(&self, values: [u64; 2], custom_changed: bool) -> u64 {
        let (old, ticket) = self.begin_root().expect("root writers are serialized");
        self.finish_root(old, ticket, values, custom_changed);
        ticket
    }

    fn try_flush(&self, index: usize, value: u64) -> Option<u64> {
        let old = self.ingress_seq.load(Ordering::Acquire);
        if !Self::try_begin_source(&self.ingress_seq, old) {
            return None;
        }
        let ticket = self.reserve_ticket();
        self.ingress_values[index].store(value, Ordering::Release);
        self.ingress_tickets[index].store(ticket, Ordering::Release);
        self.ingress_seq.store(old + 2, Ordering::Release);
        Some(ticket)
    }

    fn completed_generation(&self) -> Option<Generation> {
        let root = self.root_seq.load(Ordering::Acquire);
        let ingress = self.ingress_seq.load(Ordering::Acquire);
        (ingress.is_multiple_of(2) && root.is_multiple_of(2))
            .then_some(Generation { root, ingress })
    }

    fn try_snapshot(&self) -> Option<Snapshot> {
        let ingress = self.ingress_seq.load(Ordering::Acquire);
        let root = self.root_seq.load(Ordering::Acquire);
        if !ingress.is_multiple_of(2) || !root.is_multiple_of(2) {
            return None;
        }
        let cutoff = self.root_ticket.load(Ordering::Acquire);
        let mut values = [0; 2];
        for (index, value) in values.iter_mut().enumerate() {
            *value = self.root_values[index].load(Ordering::Acquire);
            let ingress_value = self.ingress_values[index].load(Ordering::Acquire);
            let ticket = self.ingress_tickets[index].load(Ordering::Acquire);
            if ticket > cutoff {
                *value = ingress_value;
            }
        }
        let custom_generation = self.custom_generation.load(Ordering::Acquire);
        (self.root_seq.load(Ordering::Acquire) == root
            && self.ingress_seq.load(Ordering::Acquire) == ingress)
            .then_some(Snapshot {
                generation: Generation { root, ingress },
                custom_generation,
                values,
            })
    }

    fn synchronize(&self, cached: &mut Snapshot) -> Option<Generation> {
        if self.completed_generation() == Some(cached.generation) {
            return Some(cached.generation);
        }
        // Do not partially update a local baseline if either source is busy.
        let snapshot = self.try_snapshot()?;
        *cached = snapshot;
        Some(snapshot.generation)
    }

    fn begin_completion(&self, expected: Generation) -> bool {
        if !Self::try_begin_source(&self.ingress_seq, expected.ingress) {
            return false;
        }
        if self.root_seq.load(Ordering::Acquire) != expected.root
            || !expected.root.is_multiple_of(2)
        {
            self.ingress_seq.store(expected.ingress, Ordering::Release);
            return false;
        }
        true
    }

    fn reserve_completion(&self, expected: Generation) -> Option<u64> {
        let ticket = self.reserve_ticket();
        // Essential: a load can start after the first root check but reserve
        // its ticket before this completion. No payload may change on failure.
        if self.root_seq.load(Ordering::Acquire) != expected.root {
            self.ingress_seq.store(expected.ingress, Ordering::Release);
            return None;
        }
        Some(ticket)
    }

    fn finish_completion(
        &self,
        expected: Generation,
        ticket: u64,
        values: [u64; 2],
    ) -> Option<Generation> {
        for (index, value) in values.into_iter().enumerate() {
            self.ingress_values[index].store(value, Ordering::Release);
            self.ingress_tickets[index].store(ticket, Ordering::Release);
        }
        let ingress = expected.ingress + 2;
        self.ingress_seq.store(ingress, Ordering::Release);
        // Payload with an older ticket is harmless, but cannot certify a cache
        // containing these completed DSP values after the root has changed.
        (self.root_seq.load(Ordering::Acquire) == expected.root).then_some(Generation {
            root: expected.root,
            ingress,
        })
    }

    fn try_complete(&self, expected: Generation, values: [u64; 2]) -> Option<Generation> {
        if !self.begin_completion(expected) {
            return None;
        }
        let ticket = self.reserve_completion(expected)?;
        self.finish_completion(expected, ticket, values)
    }
}

#[test]
fn accepted_root_snapshot_never_mixes_values_and_custom_epoch() {
    loom::model(|| {
        let publication = Arc::new(PublicationModel::new());
        let writer = {
            let publication = Arc::clone(&publication);
            thread::spawn(move || publication.publish_root(ROOT_VALUES, true))
        };
        let snapshot = publication.try_snapshot();
        let ticket = writer.join().expect("root writer must not panic");
        if let Some(snapshot) = snapshot {
            match snapshot.generation {
                Generation {
                    root: 0,
                    ingress: 0,
                } => {
                    assert_eq!(snapshot.values, INITIAL_VALUES);
                    assert_eq!(snapshot.custom_generation, 0);
                }
                Generation {
                    root: 2,
                    ingress: 0,
                } => {
                    assert_eq!(snapshot.values, ROOT_VALUES);
                    assert_eq!(snapshot.custom_generation, ticket);
                }
                generation => panic!("unexpected snapshot generation {generation:?}"),
            }
        }
    });
}

#[test]
fn concurrent_root_flush_and_snapshot_follow_exact_ticket_cutoff() {
    // Two writers plus a reader otherwise multiply all payload interleavings.
    // Bound scheduling preemptions, not branches/iterations/time: every execution
    // within this bound is explored, including weak-memory read alternatives.
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(2);
    builder.check(|| {
        let publication = Arc::new(PublicationModel::new());
        let root = {
            let publication = Arc::clone(&publication);
            thread::spawn(move || publication.publish_root(ROOT_VALUES, true))
        };
        let flush = {
            let publication = Arc::clone(&publication);
            thread::spawn(move || publication.try_flush(0, 12).expect("sole ingress writer"))
        };
        let snapshot = publication.try_snapshot();
        let root_ticket = root.join().expect("root writer must not panic");
        let flush_ticket = flush.join().expect("flush writer must not panic");
        assert_ne!(root_ticket, flush_ticket);

        let check = |snapshot: Snapshot| {
            let has_root = snapshot.generation.root == 2;
            let has_flush = snapshot.generation.ingress == 2;
            assert!([0, 2].contains(&snapshot.generation.root));
            assert!([0, 2].contains(&snapshot.generation.ingress));
            let mut expected = if has_root {
                ROOT_VALUES
            } else {
                INITIAL_VALUES
            };
            if has_flush && (!has_root || flush_ticket > root_ticket) {
                expected[0] = 12;
            }
            assert_eq!(snapshot.values, expected);
            assert_eq!(
                snapshot.custom_generation,
                if has_root { root_ticket } else { 0 }
            );
        };
        if let Some(snapshot) = snapshot {
            check(snapshot);
        }
        let final_snapshot = publication.try_snapshot().expect("both writers completed");
        assert_eq!(
            final_snapshot.generation,
            Generation {
                root: 2,
                ingress: 2
            }
        );
        check(final_snapshot);
    });
}

#[test]
fn flush_during_odd_root_is_retained_per_point_until_a_newer_load() {
    loom::model(|| {
        let publication = PublicationModel::new();
        let mut cached = publication.try_snapshot().expect("initial baseline");
        let original = cached;
        let (old, root_ticket) = publication.begin_root().expect("sole root writer");
        let flush_ticket = publication
            .try_flush(0, 12)
            .expect("root does not gate flush");
        assert!(flush_ticket > root_ticket);
        assert_eq!(publication.synchronize(&mut cached), None);
        assert_eq!(
            cached, original,
            "incomplete baseline must not mutate local values"
        );
        publication.finish_root(old, root_ticket, ROOT_VALUES, true);
        let first_point = publication.try_snapshot().expect("both sources stable");
        assert_eq!(first_point.values, [12, 21]);
        assert_eq!(first_point.custom_generation, root_ticket);

        // A flush list is not one atomic batch: a load can fall between points.
        let cutoff = publication.publish_root([13, 23], false);
        assert!(cutoff > flush_ticket);
        let replaced = publication.try_snapshot().expect("load completed");
        assert_eq!(
            replaced.values,
            [13, 23],
            "pre-load ingress cannot resurrect"
        );
        assert_eq!(
            replaced.custom_generation, root_ticket,
            "scalar root preserves custom epoch"
        );
        assert_eq!(
            publication.ingress_tickets[0].load(Ordering::Acquire),
            flush_ticket
        );
        assert!(publication.try_flush(1, 24).expect("next individual point") > cutoff);
        let last_point = publication.try_snapshot().expect("second point completed");
        assert_eq!(last_point.values, [13, 24]);
        assert_eq!(
            publication.try_snapshot(),
            Some(last_point),
            "reads do not retoken or compact"
        );
        assert_eq!(
            publication.ingress_tickets[0].load(Ordering::Acquire),
            flush_ticket
        );
    });
}

#[test]
fn load_between_first_root_check_and_dsp_ticket_cancels_without_payload_writes() {
    loom::model(|| {
        let publication = PublicationModel::new();
        publication.try_flush(1, 22).expect("seed ingress payload");
        let baseline = publication.try_snapshot().expect("completed baseline");
        assert!(publication.begin_completion(baseline.generation));
        let root_ticket = publication.publish_root(ROOT_VALUES, true);
        assert_eq!(publication.reserve_completion(baseline.generation), None);
        assert_eq!(
            publication.ingress_seq.load(Ordering::Acquire),
            baseline.generation.ingress
        );
        assert_eq!(
            publication.ingress_values[0].load(Ordering::Acquire),
            INITIAL_VALUES[0]
        );
        assert_eq!(publication.ingress_values[1].load(Ordering::Acquire), 22);
        assert_eq!(publication.ingress_tickets[0].load(Ordering::Acquire), 0);
        assert!(publication.ingress_tickets[1].load(Ordering::Acquire) < root_ticket);
        let snapshot = publication.try_snapshot().expect("canceled gate restored");
        assert_eq!(snapshot.values, ROOT_VALUES);
        assert_eq!(snapshot.custom_generation, root_ticket);
        // An orphan reservation is allowed; the next real point remains newer.
        assert!(
            publication
                .try_flush(0, 14)
                .expect("cancellation releases ingress")
                > root_ticket
        );
        assert_eq!(
            publication.try_snapshot().expect("flush completed").values,
            [14, 21]
        );
    });
}

#[test]
fn root_after_dsp_ticket_wins_and_completion_cannot_certify_stale_cache() {
    loom::model(|| {
        let publication = PublicationModel::new();
        let baseline = publication.try_snapshot().expect("initial baseline");
        assert!(publication.begin_completion(baseline.generation));
        let dsp_ticket = publication
            .reserve_completion(baseline.generation)
            .expect("unchanged root");
        let root_ticket = publication.publish_root(ROOT_VALUES, true);
        assert!(root_ticket > dsp_ticket);
        assert_eq!(
            publication.finish_completion(baseline.generation, dsp_ticket, [12, 22]),
            None
        );
        let snapshot = publication.try_snapshot().expect("both sources stable");
        assert_eq!(snapshot.values, ROOT_VALUES);
        assert_eq!(snapshot.custom_generation, root_ticket);
        assert_eq!(
            snapshot.generation,
            Generation {
                root: 2,
                ingress: 2
            }
        );
    });
}

#[test]
fn concurrent_load_defeats_old_dsp_completion_under_weak_memory() {
    loom::model(|| {
        let publication = Arc::new(PublicationModel::new());
        let baseline = publication.try_snapshot().expect("initial baseline");
        let root = {
            let publication = Arc::clone(&publication);
            thread::spawn(move || publication.publish_root(ROOT_VALUES, true))
        };
        // Unlike the staged schedules above, there is no synchronization
        // between these writers except the modeled production atomics. If a
        // root reservation precedes DSP's reservation, the AcqRel ticket chain
        // must make the odd/changed root visible to the second root check.
        let completed = publication.try_complete(baseline.generation, [12, 22]);
        let root_ticket = root.join().expect("root writer must not panic");
        let snapshot = publication.try_snapshot().expect("both sources stable");
        assert_eq!(snapshot.values, ROOT_VALUES);
        assert_eq!(snapshot.custom_generation, root_ticket);
        assert_eq!(snapshot.generation.root, 2);
        if let Some(generation) = completed {
            assert_eq!(
                generation,
                Generation {
                    root: 0,
                    ingress: 2
                }
            );
            assert_eq!(snapshot.generation.ingress, generation.ingress);
        }
    });
}

#[test]
fn cached_combined_generation_retokens_and_preserves_the_whole_baseline() {
    // Exercise each source independently: comparing just one counter would
    // incorrectly reuse the cached sibling value and permit stale completion.
    for root_writer in [false, true] {
        loom::model(move || {
            let publication = Arc::new(PublicationModel::new());
            publication
                .try_flush(1, 22)
                .expect("seed a noninitial cached generation");
            let mut cached = publication
                .try_snapshot()
                .expect("initial composed baseline");
            let original = cached;
            let writer = {
                let publication = Arc::clone(&publication);
                thread::spawn(move || {
                    if root_writer {
                        publication.publish_root(ROOT_VALUES, false);
                    } else {
                        publication.try_flush(1, 23).expect("sole ingress writer");
                    }
                })
            };
            let synchronized = publication.synchronize(&mut cached);
            writer.join().expect("source writer must not panic");
            if let Some(expected) = synchronized {
                let current = publication.try_snapshot().expect("source writer completed");
                if expected == original.generation {
                    assert_eq!(cached, original);
                } else {
                    assert_eq!(
                        cached, current,
                        "new values require the actual composed token"
                    );
                }
                cached.values[0] = 14; // DSP completes only this parameter.
                let published = publication.try_complete(expected, cached.values);
                assert_eq!(published.is_some(), expected == current.generation);
                let after = publication
                    .try_snapshot()
                    .expect("completion released its gate");
                assert_eq!(after.values[1], if root_writer { 21 } else { 23 });
                if let Some(generation) = published {
                    assert_eq!(generation, after.generation);
                    assert_eq!(after.values, cached.values);
                } else {
                    assert_eq!(after, current);
                }
            } else {
                assert_eq!(
                    cached, original,
                    "failed snapshot cannot partially replace baseline"
                );
            }
        });
    }
}

#[test]
fn same_ingress_generation_completions_have_exactly_one_winner() {
    loom::model(|| {
        let publication = Arc::new(PublicationModel::new());
        let expected = publication
            .try_snapshot()
            .expect("initial baseline")
            .generation;
        let first = {
            let publication = Arc::clone(&publication);
            thread::spawn(move || publication.try_complete(expected, [11, 21]))
        };
        let second = {
            let publication = Arc::clone(&publication);
            thread::spawn(move || publication.try_complete(expected, [12, 22]))
        };
        let first = first.join().expect("first completion must not panic");
        let second = second.join().expect("second completion must not panic");
        assert_ne!(first.is_some(), second.is_some());
        let snapshot = publication.try_snapshot().expect("winner completed");
        assert_eq!(snapshot.generation, first.or(second).expect("one winner"));
        assert_eq!(
            snapshot.generation,
            Generation {
                root: 0,
                ingress: 2
            }
        );
        assert_eq!(
            snapshot.values,
            if first.is_some() { [11, 21] } else { [12, 22] }
        );
        assert_eq!(snapshot.custom_generation, 0);
    });
}
