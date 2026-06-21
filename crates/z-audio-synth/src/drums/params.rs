//! Global drum kit controls.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrumKitParams {
    pub kick_level: f32,
    pub snare_level: f32,
    pub tom_level: f32,
    pub hat_level: f32,
    pub cymbal_level: f32,
    pub tuning_semitones: f32,
    pub decay_scale: f32,
    pub tone: f32,
    pub snare_wire: f32,
    pub room_amount: f32,
    pub stereo_width: f32,
    pub master_gain_db: f32,
}

impl Default for DrumKitParams {
    fn default() -> Self {
        Self {
            kick_level: 0.90,
            snare_level: 0.84,
            tom_level: 0.78,
            hat_level: 0.55,
            cymbal_level: 0.62,
            tuning_semitones: 0.0,
            decay_scale: 1.0,
            tone: 0.55,
            snare_wire: 0.70,
            room_amount: 0.18,
            stereo_width: 0.72,
            master_gain_db: -9.0,
        }
    }
}

impl DrumKitParams {
    pub fn sanitized(self) -> Self {
        Self {
            kick_level: self.kick_level.clamp(0.0, 1.0),
            snare_level: self.snare_level.clamp(0.0, 1.0),
            tom_level: self.tom_level.clamp(0.0, 1.0),
            hat_level: self.hat_level.clamp(0.0, 1.0),
            cymbal_level: self.cymbal_level.clamp(0.0, 1.0),
            tuning_semitones: self.tuning_semitones.clamp(-12.0, 12.0),
            decay_scale: self.decay_scale.clamp(0.25, 2.50),
            tone: self.tone.clamp(0.0, 1.0),
            snare_wire: self.snare_wire.clamp(0.0, 1.0),
            room_amount: self.room_amount.clamp(0.0, 1.0),
            stereo_width: self.stereo_width.clamp(0.0, 1.0),
            master_gain_db: self.master_gain_db.clamp(-24.0, 12.0),
        }
    }
}
