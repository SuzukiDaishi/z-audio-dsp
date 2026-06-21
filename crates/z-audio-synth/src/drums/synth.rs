//! Polyphonic formula drum set.

use z_audio_dsp::{
    Effect, EventKind, ParamId, ParametricReverb, ParametricReverbParams, ProcessContext,
    TimedEvent, db_to_linear,
};

use super::{DrumInstrument, DrumKitParams, DrumVoice};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FormulaDrumKitConfig {
    pub sample_rate: f32,
    pub max_block_size: usize,
    pub max_polyphony: usize,
}

impl Default for FormulaDrumKitConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000.0,
            max_block_size: 512,
            max_polyphony: 48,
        }
    }
}

pub struct FormulaDrumKit {
    sample_rate: f32,
    max_block_size: usize,
    voices: Vec<DrumVoice>,
    next_activation_id: u64,
    params: DrumKitParams,
    room: ParametricReverb,
}

impl FormulaDrumKit {
    pub fn new(config: FormulaDrumKitConfig) -> Self {
        let voices = (0..config.max_polyphony.max(1))
            .map(|i| DrumVoice::new(0x4452_554d_u32.wrapping_add(i as u32 * 97)))
            .collect();
        let mut kit = Self {
            sample_rate: config.sample_rate.max(1.0),
            max_block_size: config.max_block_size.max(1),
            voices,
            next_activation_id: 0,
            params: DrumKitParams::default(),
            room: ParametricReverb::default(),
        };
        kit.prepare();
        kit
    }

    fn prepare(&mut self) {
        for voice in &mut self.voices {
            voice.prepare(self.sample_rate);
        }
        self.room.prepare(self.sample_rate, self.max_block_size);
        self.refresh_room_params();
    }

    pub fn active_voice_count(&self) -> usize {
        self.voices.iter().filter(|v| v.is_active()).count()
    }

    pub fn set_param(&mut self, id: ParamId, value: f32) {
        let m = id.metadata();
        let clamped = value.clamp(m.min, m.max);
        match id {
            ParamId::DrumKickLevel => self.params.kick_level = clamped,
            ParamId::DrumSnareLevel => self.params.snare_level = clamped,
            ParamId::DrumTomLevel => self.params.tom_level = clamped,
            ParamId::DrumHatLevel => self.params.hat_level = clamped,
            ParamId::DrumCymbalLevel => self.params.cymbal_level = clamped,
            ParamId::DrumTuning => self.params.tuning_semitones = clamped,
            ParamId::DrumDecay => self.params.decay_scale = clamped,
            ParamId::DrumTone => self.params.tone = clamped,
            ParamId::DrumSnareWire => self.params.snare_wire = clamped,
            ParamId::DrumRoomAmount => self.params.room_amount = clamped,
            ParamId::DrumStereoWidth => self.params.stereo_width = clamped,
            ParamId::DrumMasterGain => self.params.master_gain_db = clamped,
            _ => {}
        }
        self.params = self.params.sanitized();
        self.refresh_room_params();
    }

    pub fn param_value(&self, id: ParamId) -> f32 {
        match id {
            ParamId::DrumKickLevel => self.params.kick_level,
            ParamId::DrumSnareLevel => self.params.snare_level,
            ParamId::DrumTomLevel => self.params.tom_level,
            ParamId::DrumHatLevel => self.params.hat_level,
            ParamId::DrumCymbalLevel => self.params.cymbal_level,
            ParamId::DrumTuning => self.params.tuning_semitones,
            ParamId::DrumDecay => self.params.decay_scale,
            ParamId::DrumTone => self.params.tone,
            ParamId::DrumSnareWire => self.params.snare_wire,
            ParamId::DrumRoomAmount => self.params.room_amount,
            ParamId::DrumStereoWidth => self.params.stereo_width,
            ParamId::DrumMasterGain => self.params.master_gain_db,
            _ => id.metadata().default,
        }
    }

    pub fn note_on(&mut self, note: u8, velocity: f32) {
        let instrument = DrumInstrument::from_midi_note(note);
        if matches!(
            instrument,
            DrumInstrument::ClosedHat | DrumInstrument::PedalHat
        ) {
            self.choke_open_hats();
        }
        let index = self.find_voice_for_note_on();
        self.next_activation_id = self.next_activation_id.wrapping_add(1);
        self.voices[index].note_on(note, velocity, self.params, self.next_activation_id);
    }

    pub fn note_off(&mut self, note: u8) {
        if DrumInstrument::from_midi_note(note).is_hat() {
            for voice in &mut self.voices {
                if voice.note() == note && voice.is_open_hat() {
                    voice.choke(0.35);
                }
            }
        }
    }

    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        let ctx = ProcessContext::new(self.sample_rate, left.len(), 120.0, &[]);
        self.process_with_context(&ctx, left, right);
    }

    pub fn process_with_context(
        &mut self,
        ctx: &ProcessContext,
        left: &mut [f32],
        right: &mut [f32],
    ) {
        debug_assert_eq!(left.len(), right.len());
        left.fill(0.0);
        right.fill(0.0);
        let mut event_index = 0;
        for i in 0..left.len() {
            while event_index < ctx.events.len() && ctx.events[event_index].sample_offset == i {
                self.handle_event(ctx.events[event_index]);
                event_index += 1;
            }
            for voice in &mut self.voices {
                let (vl, vr) = voice.next_sample();
                left[i] += vl;
                right[i] += vr;
            }
        }
        if self.params.room_amount > 0.001 {
            self.room.process_stereo(ctx, left, right);
        }
        let master = db_to_linear(self.params.master_gain_db);
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            *l *= master;
            *r *= master;
        }
    }

    fn refresh_room_params(&mut self) {
        let amount = self.params.room_amount.clamp(0.0, 1.0);
        self.room.set_params(ParametricReverbParams {
            mix: amount * 0.30,
            room_size: 0.22 + amount * 0.42,
            decay_time_sec: 0.38 + amount * 1.55,
            pre_delay_ms: 3.0 + amount * 13.0,
            diffusion: 0.42 + amount * 0.30,
            damping: 0.48,
            low_cut_hz: 120.0,
            high_cut_hz: 9_500.0,
            modulation_rate_hz: 0.0,
            modulation_depth: 0.0,
            width: self.params.stereo_width,
            early_late_mix: 0.48,
            output_gain_db: -1.5,
        });
    }

    fn choke_open_hats(&mut self) {
        for voice in &mut self.voices {
            if voice.is_open_hat() {
                voice.choke(0.08);
            }
        }
    }

    fn handle_event(&mut self, event: TimedEvent) {
        match event.kind {
            EventKind::NoteOn { note, velocity } => self.note_on(note, velocity),
            EventKind::NoteOff { note, .. } => self.note_off(note),
            EventKind::Param { id, value } => self.set_param(id, value),
        }
    }

    fn find_voice_for_note_on(&self) -> usize {
        if let Some(index) = self.voices.iter().position(|v| !v.is_active()) {
            return index;
        }
        self.voices
            .iter()
            .enumerate()
            .min_by_key(|(_, v)| v.activation_id())
            .map(|(index, _)| index)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_rms(kit: &mut FormulaDrumKit, blocks: usize) -> f32 {
        let mut left = [0.0_f32; 128];
        let mut right = [0.0_f32; 128];
        let mut sum = 0.0;
        let mut count = 0;
        for _ in 0..blocks {
            kit.process(&mut left, &mut right);
            sum += left.iter().map(|s| s * s).sum::<f32>();
            count += left.len();
        }
        (sum / count as f32).sqrt()
    }

    fn render_note_rms(note: u8, velocity: f32) -> f32 {
        let mut kit = FormulaDrumKit::new(FormulaDrumKitConfig::default());
        kit.note_on(note, velocity);
        render_rms(&mut kit, 32)
    }

    #[test]
    fn gm_drum_notes_emit_finite_output() {
        for note in [36, 38, 42, 46, 49, 51, 41, 50] {
            let mut kit = FormulaDrumKit::new(FormulaDrumKitConfig::default());
            kit.note_on(note, 0.9);
            let mut left = [0.0_f32; 128];
            let mut right = [0.0_f32; 128];
            kit.process(&mut left, &mut right);
            assert!(left.iter().all(|s| s.is_finite()), "note {note}");
            assert!(right.iter().all(|s| s.is_finite()), "note {note}");
            let peak = left
                .iter()
                .chain(right.iter())
                .map(|s| s.abs())
                .fold(0.0_f32, f32::max);
            assert!(peak > 1.0e-5, "note {note} peak={peak}");
        }
    }

    #[test]
    fn velocity_controls_loudness() {
        assert!(render_note_rms(36, 1.0) > render_note_rms(36, 0.35) * 1.8);
        assert!(render_note_rms(38, 1.0) > render_note_rms(38, 0.35) * 1.8);
    }

    #[test]
    fn group_level_can_mute_kick_without_muting_snare() {
        let mut kit = FormulaDrumKit::new(FormulaDrumKitConfig::default());
        kit.set_param(ParamId::DrumKickLevel, 0.0);
        kit.note_on(36, 1.0);
        let kick = render_rms(&mut kit, 24);

        let mut kit = FormulaDrumKit::new(FormulaDrumKitConfig::default());
        kit.set_param(ParamId::DrumKickLevel, 0.0);
        kit.note_on(38, 1.0);
        let snare = render_rms(&mut kit, 24);

        assert!(kick < 1.0e-4, "kick={kick}");
        assert!(snare > 1.0e-3, "snare={snare}");
    }

    #[test]
    fn closed_hat_chokes_open_hat() {
        let mut open = FormulaDrumKit::new(FormulaDrumKitConfig::default());
        open.note_on(46, 1.0);
        let _early = render_rms(&mut open, 10);
        let open_tail = render_rms(&mut open, 50);

        let mut choked = FormulaDrumKit::new(FormulaDrumKitConfig::default());
        choked.note_on(46, 1.0);
        let _early = render_rms(&mut choked, 10);
        choked.note_on(42, 0.9);
        let choked_tail = render_rms(&mut choked, 50);

        assert!(
            choked_tail < open_tail * 0.85,
            "choked_tail={choked_tail} open_tail={open_tail}"
        );
    }
}
