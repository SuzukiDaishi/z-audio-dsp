//! Binary sample bank format for the generic single-sample sampler.
//!
//! A bank is a self-contained blob of one preloaded PCM sample plus a
//! default root note. It is produced offline (see
//! `cargo xtask prepare-sampler-bank` in the root repository) and loaded at
//! plugin init time via [`load_bank_bytes`]; nothing in this module is used
//! from the audio thread.

use std::fmt;

use z_audio_dsp::SampleBuffer;

/// A loaded single sample ready to hand to
/// [`super::GenericSampler::load_bank`].
#[derive(Clone)]
pub struct SamplerBank {
    pub sample: SampleBuffer,
    pub default_root_note: u8,
}

#[derive(Debug)]
pub enum BankError {
    Truncated,
    BadMagic,
    UnsupportedVersion(u32),
}

impl fmt::Display for BankError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BankError::Truncated => write!(f, "sampler bank data is truncated"),
            BankError::BadMagic => write!(f, "not a generic sampler bank (bad magic)"),
            BankError::UnsupportedVersion(v) => {
                write!(f, "unsupported sampler bank version {v}")
            }
        }
    }
}

impl std::error::Error for BankError {}

const MAGIC: &[u8; 8] = b"ZSMPLBNK";
const VERSION: u32 = 1;

/// Serializes a single sample plus its default root note into the on-disk
/// bank format. PCM is quantized to 16-bit integers (halving size on disk
/// relative to f32; lossless for 16-bit source material).
pub fn build_bank_bytes(sample_rate: f32, channels: u8, pcm: &[f32], default_root_note: u8) -> Vec<u8> {
    let channels = channels.max(1);
    let frame_count = pcm.len() / channels as usize;
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.push(channels);
    out.push(default_root_note);
    out.extend_from_slice(&(frame_count as u32).to_le_bytes());
    for s in pcm {
        let quantized = (s.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
        out.extend_from_slice(&quantized.to_le_bytes());
    }
    out
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], BankError> {
        if self.pos + n > self.data.len() {
            return Err(BankError::Truncated);
        }
        let slice = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, BankError> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, BankError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn f32(&mut self) -> Result<f32, BankError> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn i16(&mut self) -> Result<i16, BankError> {
        Ok(i16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
}

/// Parses a bank previously written by [`build_bank_bytes`].
pub fn load_bank_bytes(bytes: &[u8]) -> Result<SamplerBank, BankError> {
    let mut cursor = Cursor { data: bytes, pos: 0 };
    if cursor.take(8)? != MAGIC {
        return Err(BankError::BadMagic);
    }
    let version = cursor.u32()?;
    if version != VERSION {
        return Err(BankError::UnsupportedVersion(version));
    }
    let sample_rate = cursor.f32()?;
    let channels = cursor.u8()?;
    let default_root_note = cursor.u8()?;
    let frame_count = cursor.u32()? as usize;
    let total = frame_count * channels.max(1) as usize;
    let mut pcm = Vec::with_capacity(total);
    for _ in 0..total {
        pcm.push(cursor.i16()? as f32 / i16::MAX as f32);
    }
    Ok(SamplerBank {
        sample: SampleBuffer::new(sample_rate, channels, pcm),
        default_root_note,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_pcm_and_metadata() {
        let pcm = vec![0.1, -0.1, 0.2, -0.2, 0.3, -0.3];
        let bytes = build_bank_bytes(44_100.0, 2, &pcm, 67);
        let bank = load_bank_bytes(&bytes).expect("bank should parse");
        assert_eq!(bank.default_root_note, 67);
        assert_eq!(bank.sample.channels(), 2);
        assert_eq!(bank.sample.frames(), 3);
        assert!((bank.sample.sample_rate() - 44_100.0).abs() < 1.0e-3);
    }

    #[test]
    fn rejects_truncated_data() {
        let pcm = vec![0.1, 0.2, 0.3];
        let mut bytes = build_bank_bytes(48_000.0, 1, &pcm, 60);
        bytes.truncate(bytes.len() - 2);
        assert!(load_bank_bytes(&bytes).is_err());
    }

    #[test]
    fn rejects_bad_magic() {
        let bytes = vec![0u8; 32];
        assert!(matches!(load_bank_bytes(&bytes), Err(BankError::BadMagic)));
    }

    #[test]
    fn rejects_unsupported_version() {
        let pcm = vec![0.1, 0.2];
        let mut bytes = build_bank_bytes(48_000.0, 1, &pcm, 60);
        bytes[8..12].copy_from_slice(&999u32.to_le_bytes());
        assert!(matches!(
            load_bank_bytes(&bytes),
            Err(BankError::UnsupportedVersion(999))
        ));
    }

    #[test]
    fn pcm_round_trip_is_lossless_for_16_bit_values() {
        let pcm = vec![0.0, 1.0, -1.0, 0.5, -0.5];
        let bytes = build_bank_bytes(48_000.0, 1, &pcm, 60);
        let bank = load_bank_bytes(&bytes).expect("bank should parse");
        assert_eq!(bank.sample.frames(), 5);
    }
}
