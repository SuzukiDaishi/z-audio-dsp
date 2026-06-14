//! Renders 1 second of white noise directly from [`NoiseGenerator`], with no
//! envelope or effects.

use z_audio_dsp::{Generator, NoiseGenerator, ProcessContext};
use z_audio_examples::wav_writer::{output_path, write_stereo_wav};

const SAMPLE_RATE: f32 = 48_000.0;
const BLOCK_SIZE: usize = 128;
const DURATION_SECONDS: f32 = 1.0;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut noise = NoiseGenerator::new(42);
    noise.prepare(SAMPLE_RATE, BLOCK_SIZE);

    let total_samples = (SAMPLE_RATE * DURATION_SECONDS) as usize;
    let mut mono = vec![0.0_f32; total_samples];

    let events = [];
    for chunk in mono.chunks_mut(BLOCK_SIZE) {
        let ctx = ProcessContext::new(SAMPLE_RATE, chunk.len(), 120.0, &events);
        noise.process_mono(&ctx, chunk);
    }

    let path = output_path("render_noise.wav");
    write_stereo_wav(&path, SAMPLE_RATE as u32, &mono, &mono)?;
    println!("wrote {}", path.display());
    Ok(())
}
