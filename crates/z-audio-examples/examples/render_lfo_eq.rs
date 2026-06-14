//! Renders a sustained Saw note while voice 0's LFO sweeps the EQ low-band
//! cutoff frequency, demonstrating `LfoTarget::EqLowFreq` routing.

use z_audio_dsp::{GeneratorKind, LfoParams, LfoTarget, LfoWaveform};
use z_audio_examples::wav_writer::{output_path, write_stereo_wav};
use z_audio_synth::{SimpleSynth, SimpleSynthConfig, midi};

const SAMPLE_RATE: f32 = 48_000.0;
const BLOCK_SIZE: usize = 128;
const DURATION_SECONDS: f32 = 4.0;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut synth = SimpleSynth::new(SimpleSynthConfig {
        sample_rate: SAMPLE_RATE,
        max_block_size: BLOCK_SIZE,
        max_polyphony: 4,
    });

    synth.set_generator_kind(GeneratorKind::Saw);
    synth.set_lfo(LfoParams {
        enabled: true,
        waveform: LfoWaveform::Sine,
        rate_hz: 0.5,
        amount: 1.0, // +/- 1 octave around the EQ low band's 200 Hz default
        target: LfoTarget::EqLowFreq,
        retrigger: true,
    });
    synth.eq_mut().mid.enabled = false;
    synth.eq_mut().high.enabled = false;

    let total_samples = (SAMPLE_RATE * DURATION_SECONDS) as usize;
    let mut left = vec![0.0_f32; total_samples];
    let mut right = vec![0.0_f32; total_samples];

    synth.note_on(midi::C4 - 12, 0.9); // C3, rich in harmonics above 100 Hz

    for (l_chunk, r_chunk) in left
        .chunks_mut(BLOCK_SIZE)
        .zip(right.chunks_mut(BLOCK_SIZE))
    {
        synth.process(l_chunk, r_chunk);
    }

    let path = output_path("render_lfo_eq.wav");
    write_stereo_wav(&path, SAMPLE_RATE as u32, &left, &right)?;
    println!("wrote {}", path.display());
    Ok(())
}
