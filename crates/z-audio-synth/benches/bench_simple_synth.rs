//! Benchmark for `SimpleSynth::process` at full 16-voice polyphony
//! (48kHz / block 128 / 3-band EQ), per docs/09's realtime-margin target.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use z_audio_synth::{SimpleSynth, SimpleSynthConfig};

const SAMPLE_RATE: f32 = 48_000.0;
const BLOCK_SIZE: usize = 128;
const POLYPHONY: usize = 16;

fn bench_simple_synth_16voices(c: &mut Criterion) {
    let mut synth = SimpleSynth::new(SimpleSynthConfig {
        sample_rate: SAMPLE_RATE,
        max_block_size: BLOCK_SIZE,
        max_polyphony: POLYPHONY,
    });

    for i in 0..POLYPHONY {
        synth.note_on(48 + i as u8, 0.8);
    }

    let mut left = vec![0.0_f32; BLOCK_SIZE];
    let mut right = vec![0.0_f32; BLOCK_SIZE];

    c.bench_function("bench_simple_synth_16voices", |b| {
        b.iter(|| {
            synth.process(&mut left, &mut right);
            black_box((&left, &right));
        });
    });
}

criterion_group!(benches, bench_simple_synth_16voices);
criterion_main!(benches);
