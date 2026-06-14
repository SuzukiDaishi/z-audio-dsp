//! WAV file output helper (built on `hound`) for the render examples.

use std::path::{Path, PathBuf};

use hound::{SampleFormat, WavSpec, WavWriter};

/// Returns `<workspace_root>/target/render-output/<filename>`, creating the
/// directory if it doesn't exist yet.
pub fn output_path(filename: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/render-output");
    std::fs::create_dir_all(&dir).expect("failed to create render-output directory");
    dir.join(filename)
}

/// Writes interleaved stereo `f32` samples (clamped to `[-1.0, 1.0]`) to a
/// 16-bit PCM WAV file at `path`.
pub fn write_stereo_wav(
    path: impl AsRef<Path>,
    sample_rate: u32,
    left: &[f32],
    right: &[f32],
) -> hound::Result<()> {
    debug_assert_eq!(left.len(), right.len());

    let spec = WavSpec {
        channels: 2,
        sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(path, spec)?;
    for (&l, &r) in left.iter().zip(right.iter()) {
        writer.write_sample(to_i16(l))?;
        writer.write_sample(to_i16(r))?;
    }
    writer.finalize()
}

fn to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
}
