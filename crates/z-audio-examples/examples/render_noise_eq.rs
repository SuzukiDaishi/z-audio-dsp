//! Renders white noise through the EQ's mid band-pass, whose center
//! frequency sweeps logarithmically from 100 Hz to 6 kHz.

use z_audio_dsp::GeneratorKind;
use z_audio_examples::wav_writer::{output_path, write_stereo_wav};
use z_audio_synth::{SimpleSynth, SimpleSynthConfig, midi};

const SAMPLE_RATE: f32 = 48_000.0;
const BLOCK_SIZE: usize = 128;
const DURATION_SECONDS: f32 = 4.0;
const SWEEP_START_HZ: f32 = 100.0;
const SWEEP_END_HZ: f32 = 6_000.0;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut synth = SimpleSynth::new(SimpleSynthConfig {
        sample_rate: SAMPLE_RATE,
        max_block_size: BLOCK_SIZE,
        max_polyphony: 4,
    });

    synth.set_generator_kind(GeneratorKind::Noise);
    synth.eq_mut().low.enabled = false;
    synth.eq_mut().high.enabled = false;
    synth.eq_mut().mid.q = 4.0;

    let total_samples = (SAMPLE_RATE * DURATION_SECONDS) as usize;
    let mut left = vec![0.0_f32; total_samples];
    let mut right = vec![0.0_f32; total_samples];

    synth.note_on(midi::C4, 0.9);

    let log_start = SWEEP_START_HZ.ln();
    let log_end = SWEEP_END_HZ.ln();
    for (block_index, (l_chunk, r_chunk)) in left
        .chunks_mut(BLOCK_SIZE)
        .zip(right.chunks_mut(BLOCK_SIZE))
        .enumerate()
    {
        let block_start = block_index * BLOCK_SIZE;
        let t = block_start as f32 / total_samples as f32;
        synth.eq_mut().mid.frequency_hz = (log_start + (log_end - log_start) * t).exp();

        synth.process(l_chunk, r_chunk);
    }

    let path = output_path("render_noise_eq.wav");
    write_stereo_wav(&path, SAMPLE_RATE as u32, &left, &right)?;
    println!("wrote {}", path.display());
    Ok(())
}
