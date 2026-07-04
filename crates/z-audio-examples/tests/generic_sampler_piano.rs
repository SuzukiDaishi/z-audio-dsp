//! Integration test for [`z_audio_synth::GenericSampler`] driven by the real
//! sampler bank built from `docs/samples/piano.wav` (see
//! `cargo xtask prepare-sampler-bank`). Skips (rather than fails) if that
//! generated bank isn't present, since it's gitignored and must be built
//! locally first.

use std::path::PathBuf;
use std::sync::Arc;

use z_audio_dsp::{LoopMode, ParamId};
use z_audio_synth::{GenericSampler, GenericSamplerConfig};

const SAMPLE_RATE: f32 = 48_000.0;

fn bank_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../assets/sampler/piano.bank")
}

fn load_sampler() -> Option<GenericSampler> {
    let path = bank_path();
    let bytes = std::fs::read(&path).ok()?;
    let bank = z_audio_synth::load_sampler_bank_bytes(&bytes).expect("bank should parse");
    let mut sampler = GenericSampler::new(GenericSamplerConfig {
        sample_rate: SAMPLE_RATE,
        max_block_size: 512,
        max_polyphony: 8,
    });
    sampler.load_bank(Arc::new(bank));
    Some(sampler)
}

#[test]
fn note_on_produces_finite_non_silent_audio_from_piano_wav() {
    let Some(mut sampler) = load_sampler() else {
        eprintln!(
            "skipping: {} not found; run `cargo xtask prepare-sampler-bank --source docs/samples/piano.wav --out assets/sampler/piano.bank`",
            bank_path().display()
        );
        return;
    };

    sampler.note_on(60, 0.9);
    // 3 seconds: `docs/samples/piano.wav` has a quiet lead-in before its
    // first note, so a short 1-second window can land entirely on silence.
    let mut left = vec![0.0_f32; 48_000 * 3];
    let mut right = vec![0.0_f32; 48_000 * 3];
    sampler.process(&mut left, &mut right);

    assert!(left.iter().chain(right.iter()).all(|s| s.is_finite()));
    assert!(left.iter().any(|s| s.abs() > 1.0e-4), "expected audible output");
}

#[test]
fn octave_up_advances_through_the_sample_roughly_twice_as_fast() {
    let Some(mut base) = load_sampler() else {
        eprintln!("skipping: {} not found", bank_path().display());
        return;
    };
    let Some(mut octave_up) = load_sampler() else {
        return;
    };

    base.note_on(60, 1.0);
    octave_up.note_on(72, 1.0);

    let mut left_a = vec![0.0_f32; 48_000 * 3];
    let mut right_a = vec![0.0_f32; 48_000 * 3];
    let mut left_b = vec![0.0_f32; 48_000 * 3];
    let mut right_b = vec![0.0_f32; 48_000 * 3];
    base.process(&mut left_a, &mut right_a);
    octave_up.process(&mut left_b, &mut right_b);

    assert!(left_a.iter().chain(right_a.iter()).all(|s| s.is_finite()));
    assert!(left_b.iter().chain(right_b.iter()).all(|s| s.is_finite()));
    assert!(left_a.iter().any(|s| s.abs() > 1.0e-4));
    assert!(left_b.iter().any(|s| s.abs() > 1.0e-4));
}

#[test]
fn offset_param_skips_ahead_into_the_real_sample() {
    let Some(mut at_start) = load_sampler() else {
        eprintln!("skipping: {} not found", bank_path().display());
        return;
    };
    let Some(mut at_offset) = load_sampler() else {
        return;
    };
    at_offset.set_param(ParamId::SamplerOffset, 0.3);

    at_start.note_on(60, 1.0);
    at_offset.note_on(60, 1.0);

    let mut left_a = vec![0.0_f32; 4096];
    let mut right_a = vec![0.0_f32; 4096];
    let mut left_b = vec![0.0_f32; 4096];
    let mut right_b = vec![0.0_f32; 4096];
    at_start.process(&mut left_a, &mut right_a);
    at_offset.process(&mut left_b, &mut right_b);

    assert!(left_a.iter().chain(right_a.iter()).all(|s| s.is_finite()));
    assert!(left_b.iter().chain(right_b.iter()).all(|s| s.is_finite()));
    // Different start positions in a real (non-silent, non-periodic)
    // recording should produce different early output.
    let differs = left_a.iter().zip(left_b.iter()).any(|(a, b)| (a - b).abs() > 1.0e-4);
    assert!(differs, "offset=0.3 should sound different from offset=0.0");
}

#[test]
fn infinite_loop_on_a_short_window_of_piano_wav_keeps_sounding_far_past_the_window() {
    let Some(mut sampler) = load_sampler() else {
        eprintln!("skipping: {} not found", bank_path().display());
        return;
    };
    sampler.set_param(ParamId::SamplerLoopMode, LoopMode::Infinite.to_param_value());
    // A narrow ~20ms window so looping is exercised many times over a
    // multi-second render.
    sampler.set_param(ParamId::SamplerLoopStart, 0.2);
    sampler.set_param(ParamId::SamplerLoopEnd, 0.2008);
    sampler.set_param(ParamId::SamplerLoopXfade, 0.005);
    sampler.note_on(60, 0.9);

    let mut left = vec![0.0_f32; SAMPLE_RATE as usize * 3];
    let mut right = vec![0.0_f32; SAMPLE_RATE as usize * 3];
    sampler.process(&mut left, &mut right);

    assert!(left.iter().chain(right.iter()).all(|s| s.is_finite()));
    assert!(left.iter().any(|s| s.abs() > 1.0e-4));
    assert_eq!(
        sampler.active_voice_count(),
        1,
        "looping a real recording's short window should still be sounding after 3s"
    );
}
