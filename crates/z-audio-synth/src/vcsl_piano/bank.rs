//! Binary sample bank format for the VCSL sampler piano.
//!
//! A bank is a self-contained blob of preloaded PCM plus an SFZ-style region
//! map. It is produced offline (see `cargo xtask prepare-vcsl-piano` in the
//! root repository) and loaded at plugin init time via [`load_bank_bytes`];
//! nothing in this module is used from the audio thread.

use std::fmt;
use std::sync::Arc;

use z_audio_dsp::{SampleBuffer, SampleRegion, TriggerKind};

/// A loaded region map ready to hand to [`super::VcslPiano::load_bank`].
#[derive(Clone, Default)]
pub struct VcslSampleBank {
    pub regions: Vec<Arc<SampleRegion>>,
}

/// One region plus its raw PCM, used when building a bank from an offline
/// asset-preparation step. Each source owns its own [`SampleBuffer`]; banks
/// do not currently deduplicate identical samples.
pub struct VcslRegionSource {
    pub lokey: u8,
    pub hikey: u8,
    pub lovel: u8,
    pub hivel: u8,
    pub pitch_keycenter: u8,
    pub tune_cents: f32,
    pub volume_db: f32,
    pub amp_veltrack: f32,
    pub offset_frames: u32,
    pub trigger: TriggerKind,
    pub ampeg_attack: f32,
    pub ampeg_decay: f32,
    pub ampeg_sustain: f32,
    pub ampeg_release: f32,
    pub sample_rate: f32,
    pub channels: u8,
    /// Interleaved PCM if `channels == 2`, otherwise mono.
    pub pcm: Vec<f32>,
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
            BankError::Truncated => write!(f, "VCSL bank data is truncated"),
            BankError::BadMagic => write!(f, "not a VCSL bank (bad magic)"),
            BankError::UnsupportedVersion(v) => write!(f, "unsupported VCSL bank version {v}"),
        }
    }
}

impl std::error::Error for BankError {}

const MAGIC: &[u8; 8] = b"VCSLBANK";
const VERSION: u32 = 1;

/// Serializes `sources` into the on-disk bank format.
pub fn build_bank_bytes(sources: &[VcslRegionSource]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&(sources.len() as u32).to_le_bytes());
    out.extend_from_slice(&(sources.len() as u32).to_le_bytes());

    for src in sources {
        let channels = src.channels.max(1);
        let frame_count = src.pcm.len() / channels as usize;
        out.extend_from_slice(&src.sample_rate.to_le_bytes());
        out.push(channels);
        out.extend_from_slice(&(frame_count as u32).to_le_bytes());
        for s in &src.pcm {
            out.extend_from_slice(&s.to_le_bytes());
        }
    }

    for (index, src) in sources.iter().enumerate() {
        out.push(src.lokey);
        out.push(src.hikey);
        out.push(src.lovel);
        out.push(src.hivel);
        out.push(src.pitch_keycenter);
        out.push(if src.trigger == TriggerKind::Release {
            1
        } else {
            0
        });
        out.extend_from_slice(&src.tune_cents.to_le_bytes());
        out.extend_from_slice(&src.volume_db.to_le_bytes());
        out.extend_from_slice(&src.amp_veltrack.to_le_bytes());
        out.extend_from_slice(&src.offset_frames.to_le_bytes());
        out.extend_from_slice(&src.ampeg_attack.to_le_bytes());
        out.extend_from_slice(&src.ampeg_decay.to_le_bytes());
        out.extend_from_slice(&src.ampeg_sustain.to_le_bytes());
        out.extend_from_slice(&src.ampeg_release.to_le_bytes());
        out.extend_from_slice(&(index as u32).to_le_bytes());
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
}

/// Parses a bank previously written by [`build_bank_bytes`].
pub fn load_bank_bytes(bytes: &[u8]) -> Result<VcslSampleBank, BankError> {
    let mut cursor = Cursor { data: bytes, pos: 0 };
    if cursor.take(8)? != MAGIC {
        return Err(BankError::BadMagic);
    }
    let version = cursor.u32()?;
    if version != VERSION {
        return Err(BankError::UnsupportedVersion(version));
    }
    let sample_count = cursor.u32()? as usize;
    let region_count = cursor.u32()? as usize;

    let mut samples = Vec::with_capacity(sample_count);
    for _ in 0..sample_count {
        let sample_rate = cursor.f32()?;
        let channels = cursor.u8()?;
        let frame_count = cursor.u32()? as usize;
        let total = frame_count * channels.max(1) as usize;
        let pcm_bytes = cursor.take(total * 4)?;
        let mut pcm = Vec::with_capacity(total);
        for chunk in pcm_bytes.chunks_exact(4) {
            pcm.push(f32::from_le_bytes(chunk.try_into().unwrap()));
        }
        samples.push(SampleBuffer::new(sample_rate, channels, pcm));
    }

    let mut regions = Vec::with_capacity(region_count);
    for _ in 0..region_count {
        let lokey = cursor.u8()?;
        let hikey = cursor.u8()?;
        let lovel = cursor.u8()?;
        let hivel = cursor.u8()?;
        let pitch_keycenter = cursor.u8()?;
        let trigger = if cursor.u8()? == 1 {
            TriggerKind::Release
        } else {
            TriggerKind::Attack
        };
        let tune_cents = cursor.f32()?;
        let volume_db = cursor.f32()?;
        let amp_veltrack = cursor.f32()?;
        let offset_frames = cursor.u32()? as usize;
        let ampeg_attack = cursor.f32()?;
        let ampeg_decay = cursor.f32()?;
        let ampeg_sustain = cursor.f32()?;
        let ampeg_release = cursor.f32()?;
        let sample_index = cursor.u32()? as usize;
        let sample = samples
            .get(sample_index)
            .ok_or(BankError::Truncated)?
            .clone();
        regions.push(Arc::new(SampleRegion {
            lokey,
            hikey,
            lovel,
            hivel,
            pitch_keycenter,
            tune_cents,
            volume_db,
            amp_veltrack,
            offset_frames,
            trigger,
            ampeg_attack,
            ampeg_decay,
            ampeg_sustain,
            ampeg_release,
            sample,
        }));
    }

    Ok(VcslSampleBank { regions })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(trigger: TriggerKind) -> VcslRegionSource {
        VcslRegionSource {
            lokey: 60,
            hikey: 60,
            lovel: 0,
            hivel: 127,
            pitch_keycenter: 60,
            tune_cents: 1.5,
            volume_db: -3.0,
            amp_veltrack: 0.88,
            offset_frames: 12,
            trigger,
            ampeg_attack: 0.004,
            ampeg_decay: 0.0,
            ampeg_sustain: 1.0,
            ampeg_release: 0.4,
            sample_rate: 44_100.0,
            channels: 2,
            pcm: vec![0.1, -0.1, 0.2, -0.2, 0.3, -0.3],
        }
    }

    #[test]
    fn round_trips_regions_and_pcm() {
        let sources = vec![source(TriggerKind::Attack), source(TriggerKind::Release)];
        let bytes = build_bank_bytes(&sources);
        let bank = load_bank_bytes(&bytes).expect("bank should parse");
        assert_eq!(bank.regions.len(), 2);
        assert_eq!(bank.regions[0].trigger, TriggerKind::Attack);
        assert_eq!(bank.regions[1].trigger, TriggerKind::Release);
        assert_eq!(bank.regions[0].sample.frames(), 3);
        assert_eq!(bank.regions[0].sample.channels(), 2);
        assert!((bank.regions[0].volume_db - (-3.0)).abs() < 1.0e-6);
        assert_eq!(bank.regions[0].offset_frames, 12);
    }

    #[test]
    fn rejects_truncated_data() {
        let sources = vec![source(TriggerKind::Attack)];
        let mut bytes = build_bank_bytes(&sources);
        bytes.truncate(bytes.len() - 4);
        assert!(load_bank_bytes(&bytes).is_err());
    }

    #[test]
    fn rejects_bad_magic() {
        let bytes = vec![0u8; 32];
        assert!(matches!(load_bank_bytes(&bytes), Err(BankError::BadMagic)));
    }
}
