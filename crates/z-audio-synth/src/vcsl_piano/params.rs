//! Realtime-controllable parameters for [`super::VcslPiano`].

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VcslPianoParams {
    pub master_gain_db: f32,
    pub tone: f32,
    pub velocity_curve: f32,
    pub release_level_db: f32,
    pub release_time_s: f32,
    pub stereo_width: f32,
}

impl Default for VcslPianoParams {
    fn default() -> Self {
        Self {
            master_gain_db: 0.0,
            tone: 1.0,
            velocity_curve: 0.5,
            release_level_db: 0.0,
            release_time_s: 0.35,
            stereo_width: 1.0,
        }
    }
}
