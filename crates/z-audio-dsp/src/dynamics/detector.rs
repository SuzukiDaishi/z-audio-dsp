//! Peak/RMS level detector.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DetectorMode {
    #[default]
    Peak,
    Rms,
}

impl DetectorMode {
    pub const VARIANT_COUNT: u32 = 2;

    pub fn from_param_value(value: f32) -> Self {
        if value.round() >= 1.0 {
            Self::Rms
        } else {
            Self::Peak
        }
    }

    pub fn to_param_value(self) -> f32 {
        match self {
            Self::Peak => 0.0,
            Self::Rms => 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LevelDetector {
    mode: DetectorMode,
    sample_rate: f32,
    rms_ms: f32,
    rms_coeff: f32,
    rms2: f32,
}

impl Default for LevelDetector {
    fn default() -> Self {
        let mut detector = Self {
            mode: DetectorMode::Peak,
            sample_rate: 48_000.0,
            rms_ms: 25.0,
            rms_coeff: 0.0,
            rms2: 0.0,
        };
        detector.configure(48_000.0, DetectorMode::Peak, 25.0);
        detector
    }
}

impl LevelDetector {
    pub fn configure(&mut self, sample_rate: f32, mode: DetectorMode, rms_ms: f32) {
        self.sample_rate = sample_rate.max(1.0);
        self.mode = mode;
        self.rms_ms = rms_ms.max(0.1);
        self.rms_coeff = (-1.0 / (self.rms_ms * 0.001 * self.sample_rate)).exp();
    }

    pub fn reset(&mut self) {
        self.rms2 = 0.0;
    }

    pub fn process_stereo(&mut self, left: f32, right: f32, stereo_link: f32) -> (f32, f32) {
        let linked = left.abs().max(right.abs());
        let link = stereo_link.clamp(0.0, 1.0);
        let det_l = left.abs() * (1.0 - link) + linked * link;
        let det_r = right.abs() * (1.0 - link) + linked * link;

        match self.mode {
            DetectorMode::Peak => (det_l, det_r),
            DetectorMode::Rms => {
                let mono = det_l.max(det_r);
                self.rms2 = self.rms_coeff * self.rms2 + (1.0 - self.rms_coeff) * mono * mono;
                let rms = self.rms2.sqrt();
                (rms, rms)
            }
        }
    }
}
