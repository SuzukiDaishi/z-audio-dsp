//! Benchmarks for audio-rate generators.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use z_audio_dsp::{AnalogGenerator, AnalogWaveform, Generator, NoiseGenerator, ProcessContext};

const SAMPLE_RATE: f32 = 48_000.0;
const BLOCK_SIZE: usize = 128;

fn bench_generator_sine(c: &mut Criterion) {
    let mut osc = AnalogGenerator::new(AnalogWaveform::Sine, 0.0, 0.5);
    osc.prepare(SAMPLE_RATE, BLOCK_SIZE);
    osc.set_frequency_hz(440.0);

    let events = [];
    let ctx = ProcessContext::new(SAMPLE_RATE, BLOCK_SIZE, 120.0, &events);
    let mut out = vec![0.0_f32; BLOCK_SIZE];

    c.bench_function("bench_generator_sine", |b| {
        b.iter(|| {
            osc.process_mono(&ctx, &mut out);
            black_box(&out);
        });
    });
}

fn bench_generator_saw(c: &mut Criterion) {
    let mut osc = AnalogGenerator::new(AnalogWaveform::Saw, 0.0, 0.5);
    osc.prepare(SAMPLE_RATE, BLOCK_SIZE);
    osc.set_frequency_hz(220.0);

    let events = [];
    let ctx = ProcessContext::new(SAMPLE_RATE, BLOCK_SIZE, 120.0, &events);
    let mut out = vec![0.0_f32; BLOCK_SIZE];

    c.bench_function("bench_generator_saw", |b| {
        b.iter(|| {
            osc.process_mono(&ctx, &mut out);
            black_box(&out);
        });
    });
}

fn bench_generator_noise(c: &mut Criterion) {
    let mut osc = NoiseGenerator::new(42);
    osc.prepare(SAMPLE_RATE, BLOCK_SIZE);

    let events = [];
    let ctx = ProcessContext::new(SAMPLE_RATE, BLOCK_SIZE, 120.0, &events);
    let mut out = vec![0.0_f32; BLOCK_SIZE];

    c.bench_function("bench_generator_noise", |b| {
        b.iter(|| {
            osc.process_mono(&ctx, &mut out);
            black_box(&out);
        });
    });
}

criterion_group!(
    benches,
    bench_generator_sine,
    bench_generator_saw,
    bench_generator_noise
);
criterion_main!(benches);
