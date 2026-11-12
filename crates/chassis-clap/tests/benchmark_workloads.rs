//! Keep benchmark audio equivalence and unsafe host storage under normal tests.
#[path = "../benches/support/workload.rs"]
mod workload;

fn verify<S: workload::Sample>() {
    // Tiny blocks include equal-offset records at the declared event bound.
    // Each constructor checks six paths, first use, held controls and repetition.
    for events in [0, workload::SPARSE_EVENTS, workload::MAX_EVENTS] {
        assert_eq!(workload::runners::<S, 2, 2>(8, events, false).len(), 6);
        assert_eq!(workload::runners::<S, 2, 2>(8, events, true).len(), 6);
    }
    for events in [0, workload::MAX_EVENTS] {
        assert_eq!(workload::runners::<S, 1, 1>(8, events, false).len(), 6);
        assert_eq!(workload::runners::<S, 2, 1>(8, events, false).len(), 6);
        assert_eq!(workload::runners::<S, 2, 0>(8, events, false).len(), 6);
        assert_eq!(workload::runners::<S, 8, 2>(8, events, false).len(), 6);
    }
}
#[test]
fn f32_paths_match_independent_audio_reference_and_retain_controls() {
    verify::<f32>();
}
#[test]
fn f64_paths_match_independent_audio_reference_and_retain_controls() {
    verify::<f64>();
}
