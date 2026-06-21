//! Piano runtime parameters.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PianoParams {
    pub tone: f32,
    pub brightness: f32,
    pub hammer_hardness: f32,
    pub hammer_noise: f32,
    pub inharmonicity: f32,
    pub decay: f32,
    pub release: f32,
    pub body_amount: f32,
    pub stereo_width: f32,
    pub sympathetic_amount: f32,
    pub pedal_resonance: f32,
    pub master_gain_db: f32,
}

impl Default for PianoParams {
    fn default() -> Self {
        Self {
            tone: 0.5,
            brightness: 0.55,
            hammer_hardness: 0.55,
            hammer_noise: 0.08,
            inharmonicity: 0.45,
            decay: 2.4,
            release: 0.8,
            body_amount: 0.08,
            stereo_width: 0.75,
            sympathetic_amount: 0.0,
            pedal_resonance: 0.0,
            master_gain_db: -6.0,
        }
    }
}

impl PianoParams {
    pub fn sanitized(self) -> Self {
        Self {
            tone: self.tone.clamp(0.0, 1.0),
            brightness: self.brightness.clamp(0.0, 1.0),
            hammer_hardness: self.hammer_hardness.clamp(0.0, 1.0),
            hammer_noise: self.hammer_noise.clamp(0.0, 1.0),
            inharmonicity: self.inharmonicity.clamp(0.0, 1.0),
            decay: self.decay.clamp(0.2, 8.0),
            release: self.release.clamp(0.05, 5.0),
            body_amount: self.body_amount.clamp(0.0, 1.0),
            stereo_width: self.stereo_width.clamp(0.0, 1.0),
            sympathetic_amount: self.sympathetic_amount.clamp(0.0, 1.0),
            pedal_resonance: self.pedal_resonance.clamp(0.0, 1.0),
            master_gain_db: self.master_gain_db.clamp(-24.0, 12.0),
        }
    }
}
