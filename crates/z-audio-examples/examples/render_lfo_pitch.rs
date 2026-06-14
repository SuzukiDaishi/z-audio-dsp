//! Renders a sustained Pulse note with an LFO-driven pitch vibrato,
//! demonstrating `LfoTarget::PitchSemitone` routing.

use z_audio_dsp::{GeneratorKind, LfoParams, LfoTarget, LfoWaveform};
use z_audio_examples::wav_writer::{output_path, write_stereo_wav};
use z_audio_synth::{SimpleSynth, SimpleSynthConfig, midi};

const SAMPLE_RATE: f32 = 48_000.0;
const BLOCK_SIZE: usize = 128;
const DURATION_SECONDS: f32 = 3.0;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut synth = SimpleSynth::new(SimpleSynthConfig {
        sample_rate: SAMPLE_RATE,
        max_block_size: BLOCK_SIZE,
        max_polyphony: 4,
    });

    synth.set_generator_kind(GeneratorKind::Pulse);
    synth.set_lfo(LfoParams {
        enabled: true,
        waveform: LfoWaveform::Sine,
        rate_hz: 5.0,
        amount: 0.5, // +/- 0.5 semitones
        target: LfoTarget::PitchSemitone,
        retrigger: true,
    });

    let total_samples = (SAMPLE_RATE * DURATION_SECONDS) as usize;
    let mut left = vec![0.0_f32; total_samples];
    let mut right = vec![0.0_f32; total_samples];

    synth.note_on(midi::A4, 0.9);

    for (l_chunk, r_chunk) in left
        .chunks_mut(BLOCK_SIZE)
        .zip(right.chunks_mut(BLOCK_SIZE))
    {
        synth.process(l_chunk, r_chunk);
    }

    let path = output_path("render_lfo_pitch.wav");
    write_stereo_wav(&path, SAMPLE_RATE as u32, &left, &right)?;
    println!("wrote {}", path.display());
    Ok(())
}
