use loom::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    thread,
};

const INITIAL_VALUES: [u64; 2] = [10, 20];

/// Loom-only mirror of the adapter-local scalar publication protocol.
///
/// The production representation stores `f64` bit patterns in the payload
/// atomics. These tokens preserve the same atomic and memory-ordering behavior
/// while keeping the model state small. Terminal-generation arithmetic remains
/// covered by the production unit tests rather than Loom.
struct PublicationModel {
    values: [AtomicU64; 2],
    generation: AtomicU64,
}

impl PublicationModel {
    fn new() -> Self {
        Self {
            values: [
                AtomicU64::new(INITIAL_VALUES[0]),
                AtomicU64::new(INITIAL_VALUES[1]),
            ],
            generation: AtomicU64::new(0),
        }
    }

    fn try_begin_write(&self, expected: u64) -> Option<u64> {
        if !expected.is_multiple_of(2) {
            return None;
        }

        let completed = expected + 2;
        self.generation
            .compare_exchange(expected, expected + 1, Ordering::AcqRel, Ordering::Acquire)
            .ok()?;
        Some(completed)
    }

    fn finish_write(&self, completed: u64) {
        self.generation.store(completed, Ordering::Release);
    }

    fn try_publish_values_from(&self, expected: u64, values: [u64; 2]) -> bool {
        let Some(completed) = self.try_begin_write(expected) else {
            return false;
        };

        self.values[0].store(values[0], Ordering::Release);
        self.values[1].store(values[1], Ordering::Release);
        self.finish_write(completed);
        true
    }

    fn try_publish_values(&self, values: [u64; 2]) -> bool {
        let expected = self.generation.load(Ordering::Acquire);
        self.try_publish_values_from(expected, values)
    }

    fn try_snapshot(&self) -> Option<(u64, [u64; 2])> {
        let start = self.generation.load(Ordering::Acquire);
        if !start.is_multiple_of(2) {
            return None;
        }

        let values = [
            self.values[0].load(Ordering::Acquire),
            self.values[1].load(Ordering::Acquire),
        ];
        let end = self.generation.load(Ordering::Acquire);

        (start == end && end.is_multiple_of(2)).then_some((end, values))
    }

    fn values(&self) -> [u64; 2] {
        [
            self.values[0].load(Ordering::Acquire),
            self.values[1].load(Ordering::Acquire),
        ]
    }
}

#[test]
fn same_generation_writers_are_serialized() {
    loom::model(|| {
        let publication = Arc::new(PublicationModel::new());

        let first = {
            let publication = Arc::clone(&publication);
            thread::spawn(move || publication.try_publish_values_from(0, [11, 21]))
        };
        let second = {
            let publication = Arc::clone(&publication);
            thread::spawn(move || publication.try_publish_values_from(0, [12, 22]))
        };

        let first_won = first.join().expect("first model writer must not panic");
        let second_won = second.join().expect("second model writer must not panic");

        assert_ne!(first_won, second_won);
        assert_eq!(publication.generation.load(Ordering::Acquire), 2);
        assert_eq!(
            publication.values(),
            if first_won { [11, 21] } else { [12, 22] }
        );
    });
}

#[test]
fn accepted_snapshot_never_mixes_generations() {
    loom::model(|| {
        let publication = Arc::new(PublicationModel::new());

        let writer = {
            let publication = Arc::clone(&publication);
            thread::spawn(move || {
                assert!(publication.try_publish_values_from(0, [11, 21]));
            })
        };
        let reader = {
            let publication = Arc::clone(&publication);
            thread::spawn(move || publication.try_snapshot())
        };

        writer.join().expect("model writer must not panic");
        let snapshot = reader.join().expect("model reader must not panic");

        if let Some((generation, values)) = snapshot {
            match generation {
                0 => assert_eq!(values, INITIAL_VALUES),
                2 => assert_eq!(values, [11, 21]),
                _ => panic!("accepted an unexpected generation {generation}"),
            }
        }
    });
}

#[test]
fn stale_realtime_publication_cannot_overwrite_newer_control_state() {
    loom::model(|| {
        let publication = PublicationModel::new();
        let stale_generation = publication.generation.load(Ordering::Acquire);

        assert!(publication.try_publish_values_from(stale_generation, [11, 21]));
        assert!(!publication.try_publish_values_from(stale_generation, [12, 22]));
        assert_eq!(publication.values(), [11, 21]);
        assert_eq!(publication.generation.load(Ordering::Acquire), 2);
    });
}

#[test]
fn realtime_attempt_fails_without_waiting_for_active_writer() {
    loom::model(|| {
        let publication = PublicationModel::new();
        publication.generation.store(1, Ordering::Release);

        assert!(!publication.try_publish_values([11, 21]));
        assert!(publication.try_snapshot().is_none());
        assert_eq!(publication.values(), INITIAL_VALUES);
    });
}
