//! Benchmark for the 3-band Butterworth EQ.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use z_audio_dsp::{Effect, ProcessContext, ThreeBandButterworthEq};

const SAMPLE_RATE: f32 = 48_000.0;
const BLOCK_SIZE: usize = 128;

fn bench_eq_3band(c: &mut Criterion) {
    let mut eq = ThreeBandButterworthEq::new();
    eq.prepare(SAMPLE_RATE, BLOCK_SIZE);

    let events = [];
    let ctx = ProcessContext::new(SAMPLE_RATE, BLOCK_SIZE, 120.0, &events);

    let mut left = vec![0.0_f32; BLOCK_SIZE];
    let mut right = vec![0.0_f32; BLOCK_SIZE];
    for (i, (l, r)) in left.iter_mut().zip(right.iter_mut()).enumerate() {
        let phase = i as f32 / BLOCK_SIZE as f32;
        *l = (phase * core::f32::consts::TAU).sin();
        *r = *l;
    }

    c.bench_function("bench_eq_3band", |b| {
        b.iter(|| {
            eq.process_stereo(&ctx, &mut left, &mut right);
            black_box((&left, &right));
        });
    });
}

criterion_group!(benches, bench_eq_3band);
criterion_main!(benches);
