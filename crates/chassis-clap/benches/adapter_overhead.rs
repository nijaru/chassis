//! Useful-DSP cost comparisons. Run with `cargo bench -p chassis-clap --bench
//! adapter_overhead`; append `-- --verify-only` to check equivalence without timing.

#[path = "support/workload.rs"]
mod workload;

use std::{hint::black_box, time::Instant};
use workload::{MAX_EVENTS, SPARSE_EVENTS, Sample, runners};

const BATCHES: usize = 24;
const WARMUP: usize = 64;

fn percentile(sorted: &[f64], numerator: usize) -> f64 {
    sorted[(sorted.len() * numerator).div_ceil(100).saturating_sub(1)]
}

fn run<S: Sample, const N: usize, const SC: usize>(
    frames: u32,
    events: u32,
    inplace: bool,
    verify_only: bool,
) {
    let mut paths = runners::<S, N, SC>(frames, events, inplace);
    if verify_only {
        return;
    }
    let iterations = (262_144 / (N * usize::try_from(frames).unwrap())).clamp(64, 2_048);
    for runner in &mut paths {
        for _ in 0..WARMUP {
            runner.render(black_box(true));
        }
    }
    let mut samples: Vec<Vec<f64>> = vec![Vec::with_capacity(BATCHES); paths.len()];
    for batch in 0..BATCHES {
        // Every path/strategy occupies each order position four times. Do not
        // time all bare batches first and all CLAP batches on a warmer CPU.
        for offset in 0..paths.len() {
            let index = (batch + offset) % paths.len();
            let runner = &mut paths[index];
            let started = Instant::now();
            for _ in 0..iterations {
                runner.render(black_box(true));
            }
            let ns = started.elapsed().as_secs_f64() * 1e9
                / f64::from(u32::try_from(iterations).unwrap());
            samples[index].push(ns);
        }
    }
    for (runner, mut samples) in paths.into_iter().zip(samples) {
        samples.sort_by(f64::total_cmp);
        println!(
            "{},{frames},{N},{SC},{},{events},{},{},{BATCHES},{iterations},{:.2},{:.2},{:.2}",
            S::NAME,
            if inplace { "in-place" } else { "separate" },
            runner.path().name(),
            runner.strategy(),
            percentile(&samples, 50),
            percentile(&samples, 95),
            percentile(&samples, 99)
        );
    }
}

fn matrix<S: Sample>(verify_only: bool) {
    for frames in [32, 128, 512] {
        for events in [0, SPARSE_EVENTS, MAX_EVENTS] {
            run::<S, 2, 2>(frames, events, false, verify_only);
        }
    }
    // Focused topology/alias slices rather than a Cartesian-product catalog.
    for events in [0, MAX_EVENTS] {
        run::<S, 1, 1>(128, events, false, verify_only);
        run::<S, 2, 1>(128, events, false, verify_only);
        run::<S, 2, 0>(128, events, false, verify_only);
        run::<S, 8, 2>(128, events, false, verify_only);
        run::<S, 2, 2>(128, events, true, verify_only);
    }
}

fn main() {
    let mut verify_only = false;
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--bench" => {}
            "--verify-only" => verify_only = true,
            _ => panic!("unknown benchmark argument: {argument}"),
        }
    }
    println!("# useful gain/wet-dry/linked-duck DSP; three globally modulated floats; 48 kHz");
    println!(
        "# os={} arch={}; record revision, CPU, OS version, compiler, flags and operating conditions",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!(
        "# identical input refill, timed stepped-control evaluation, output consumption; setup/verification excluded"
    );
    println!(
        "# bare-trusted omits framework validation/publication; no-sidechain uses linked main input detector"
    );
    println!(
        "# percentiles are of {BATCHES} BATCH AVERAGES, not individual callback-tail latency; p99 is maximum batch average"
    );
    println!(
        "precision,frames,main_channels,sidechain_channels,relationship,events,path,evaluation,batches,iterations,batch_mean_p50_ns,batch_mean_p95_ns,batch_mean_p99_ns"
    );
    matrix::<f32>(verify_only);
    matrix::<f64>(verify_only);
    if verify_only {
        println!("# all 228 path/strategy/precision/workload preflights passed");
    }
}
