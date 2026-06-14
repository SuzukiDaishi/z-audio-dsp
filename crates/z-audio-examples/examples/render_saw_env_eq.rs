//! Renders a Saw note with a short attack and medium release while manually
//! sweeping the EQ mid-band frequency from 300 Hz to 4 kHz, releasing the
//! note two-thirds of the way through.

use z_audio_dsp::{EnvelopeCurve, EnvelopeParams, GeneratorKind};
use z_audio_examples::wav_writer::{output_path, write_stereo_wav};
use z_audio_synth::{SimpleSynth, SimpleSynthConfig, midi};

const SAMPLE_RATE: f32 = 48_000.0;
const BLOCK_SIZE: usize = 128;
const DURATION_SECONDS: f32 = 3.0;
const SWEEP_START_HZ: f32 = 300.0;
const SWEEP_END_HZ: f32 = 4_000.0;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut synth = SimpleSynth::new(SimpleSynthConfig {
        sample_rate: SAMPLE_RATE,
        max_block_size: BLOCK_SIZE,
        max_polyphony: 4,
    });

    synth.set_generator_kind(GeneratorKind::Saw);
    synth.set_amp_envelope(EnvelopeParams {
        attack: 0.01,
        decay: 0.1,
        sustain: 0.8,
        release: 0.4,
        curve: EnvelopeCurve::Exponential,
    });

    let total_samples = (SAMPLE_RATE * DURATION_SECONDS) as usize;
    let note_off_sample = total_samples * 2 / 3;
    let mut left = vec![0.0_f32; total_samples];
    let mut right = vec![0.0_f32; total_samples];

    synth.note_on(midi::C4, 0.9);

    for (block_index, (l_chunk, r_chunk)) in left
        .chunks_mut(BLOCK_SIZE)
        .zip(right.chunks_mut(BLOCK_SIZE))
        .enumerate()
    {
        let block_start = block_index * BLOCK_SIZE;
        if block_start <= note_off_sample && note_off_sample < block_start + l_chunk.len() {
            synth.note_off(midi::C4);
        }

        let t = block_start as f32 / total_samples as f32;
        synth.eq_mut().mid.frequency_hz = SWEEP_START_HZ + (SWEEP_END_HZ - SWEEP_START_HZ) * t;

        synth.process(l_chunk, r_chunk);
    }

    let path = output_path("render_saw_env_eq.wav");
    write_stereo_wav(&path, SAMPLE_RATE as u32, &left, &right)?;
    println!("wrote {}", path.display());
    Ok(())
}
