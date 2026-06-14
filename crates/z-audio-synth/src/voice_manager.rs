//! Fixed-size voice pool with note-on/off dispatch and voice stealing.

use z_audio_dsp::{EnvelopeParams, GeneratorParams, LfoParams};

use crate::voice::Voice;

/// Policy for choosing which voice to steal when all voices are active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VoiceStealPolicy {
    /// Always steal the oldest active voice (by activation order), whether
    /// or not it is currently releasing.
    Oldest,
    /// Prefer the oldest *releasing* voice; if none are releasing, fall back
    /// to the oldest active voice.
    #[default]
    ReleasedFirst,
}

/// A fixed-size pool of [`Voice`]s, allocated once in [`VoiceManager::new`]
/// and never resized.
pub struct VoiceManager {
    voices: Vec<Voice>,
    steal_policy: VoiceStealPolicy,
    next_activation_id: u64,
}

impl VoiceManager {
    /// Creates a pool of `max_polyphony` voices, each seeded from `seed`.
    pub fn new(max_polyphony: usize, seed: u64) -> Self {
        debug_assert!(max_polyphony > 0);
        Self {
            voices: (0..max_polyphony)
                .map(|i| Voice::new(seed.wrapping_add(i as u64)))
                .collect(),
            steal_policy: VoiceStealPolicy::default(),
            next_activation_id: 0,
        }
    }

    /// Prepares every voice for `sample_rate`/`max_block_size`.
    pub fn prepare(&mut self, sample_rate: f32, max_block_size: usize) {
        for voice in &mut self.voices {
            voice.prepare(sample_rate, max_block_size);
        }
    }

    /// Returns the configured voice-stealing policy.
    pub fn steal_policy(&self) -> VoiceStealPolicy {
        self.steal_policy
    }

    /// Sets the voice-stealing policy used when [`Self::note_on`] is called
    /// with no idle voices remaining.
    pub fn set_steal_policy(&mut self, policy: VoiceStealPolicy) {
        self.steal_policy = policy;
    }

    /// Returns the voice at `index`.
    pub fn voice(&self, index: usize) -> &Voice {
        &self.voices[index]
    }

    /// Returns all voices for per-sample processing.
    pub fn voices_mut(&mut self) -> &mut [Voice] {
        &mut self.voices
    }

    /// Returns the number of currently-active (non-idle) voices.
    pub fn active_count(&self) -> usize {
        self.voices.iter().filter(|v| v.is_active()).count()
    }

    /// Returns the fixed size of the voice pool, as passed to [`Self::new`].
    pub fn max_polyphony(&self) -> usize {
        self.voices.len()
    }

    /// Triggers `note`, allocating a free voice or stealing one if the pool
    /// is full (oldest releasing voice, then oldest active voice).
    pub fn note_on(
        &mut self,
        note: u8,
        velocity: f32,
        generator_params: &GeneratorParams,
        env_params: &EnvelopeParams,
        lfo_params: &LfoParams,
    ) {
        let index = self.find_voice_for_note_on();
        self.next_activation_id += 1;
        self.voices[index].note_on(
            note,
            velocity,
            generator_params,
            env_params,
            lfo_params,
            self.next_activation_id,
        );
    }

    /// Releases every active voice currently playing `note`.
    pub fn note_off(&mut self, note: u8) {
        for voice in &mut self.voices {
            voice.note_off(note);
        }
    }

    /// Finds the voice to (re)trigger: the first idle voice, or — if all
    /// voices are active — the voice selected by [`Self::steal_policy`].
    fn find_voice_for_note_on(&self) -> usize {
        if let Some(index) = self.voices.iter().position(|v| !v.is_active()) {
            return index;
        }

        match self.steal_policy {
            VoiceStealPolicy::Oldest => self.oldest_active_voice_index(),
            VoiceStealPolicy::ReleasedFirst => self
                .voices
                .iter()
                .enumerate()
                .filter(|(_, v)| v.is_releasing())
                .min_by_key(|(_, v)| v.activation_id())
                .map(|(index, _)| index)
                .unwrap_or_else(|| self.oldest_active_voice_index()),
        }
    }

    /// Returns the index of the voice with the smallest `activation_id`
    /// (i.e. the longest-running voice). Panics if the pool is empty, which
    /// cannot happen since [`Self::new`] requires `max_polyphony > 0`.
    fn oldest_active_voice_index(&self) -> usize {
        self.voices
            .iter()
            .enumerate()
            .min_by_key(|(_, v)| v.activation_id())
            .map(|(index, _)| index)
            .expect("voice pool is never empty")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> (GeneratorParams, EnvelopeParams, LfoParams) {
        (
            GeneratorParams::default(),
            EnvelopeParams::default(),
            LfoParams::default(),
        )
    }

    #[test]
    fn max_polyphony_matches_pool_size() {
        let vm = VoiceManager::new(7, 0);
        assert_eq!(vm.max_polyphony(), 7);
    }

    #[test]
    fn note_on_activates_one_voice() {
        let (g, e, l) = params();
        let mut vm = VoiceManager::new(4, 0);
        vm.prepare(48_000.0, 128);
        vm.note_on(60, 1.0, &g, &e, &l);
        assert_eq!(vm.active_count(), 1);
    }

    #[test]
    fn note_off_releases_matching_voice() {
        let (g, e, l) = params();
        let mut vm = VoiceManager::new(4, 0);
        vm.prepare(48_000.0, 128);
        vm.note_on(60, 1.0, &g, &e, &l);
        vm.note_off(60);

        let releasing = vm.voices_mut().iter().filter(|v| v.is_releasing()).count();
        assert_eq!(releasing, 1);
    }

    #[test]
    fn voice_stealing_prefers_releasing_then_oldest() {
        let (g, e, l) = params();
        // Fast envelopes so voices reach Sustain/Release quickly.
        let e = EnvelopeParams {
            attack: 0.0,
            decay: 0.0,
            sustain: 0.5,
            release: 10.0, // long release so it stays "Releasing" for the test
            ..e
        };
        let mut vm = VoiceManager::new(2, 0);
        vm.prepare(48_000.0, 128);

        vm.note_on(60, 1.0, &g, &e, &l); // voice 0
        vm.note_on(61, 1.0, &g, &e, &l); // voice 1
        assert_eq!(vm.active_count(), 2);

        // Release voice 0; both voices remain "active" (Releasing counts).
        vm.note_off(60);

        // Pool is full (no idle voices) - the releasing voice (index 0) should
        // be stolen for the new note.
        vm.note_on(62, 1.0, &g, &e, &l);
        assert_eq!(vm.voice(0).note(), 62);
        assert_eq!(vm.voice(1).note(), 61);
    }

    #[test]
    fn voice_stealing_falls_back_to_oldest_active() {
        let (g, e, l) = params();
        let e = EnvelopeParams {
            attack: 0.0,
            decay: 0.0,
            sustain: 0.5,
            release: 10.0,
            ..e
        };
        let mut vm = VoiceManager::new(2, 0);
        vm.prepare(48_000.0, 128);

        vm.note_on(60, 1.0, &g, &e, &l); // voice 0 (oldest)
        vm.note_on(61, 1.0, &g, &e, &l); // voice 1 (newest)
        // Neither is releasing - stealing should target voice 0 (oldest).
        vm.note_on(62, 1.0, &g, &e, &l);
        assert_eq!(vm.voice(0).note(), 62);
        assert_eq!(vm.voice(1).note(), 61);
    }

    #[test]
    fn steal_policy_defaults_to_released_first() {
        let vm = VoiceManager::new(2, 0);
        assert_eq!(vm.steal_policy(), VoiceStealPolicy::ReleasedFirst);
    }

    #[test]
    fn set_steal_policy_round_trips() {
        let mut vm = VoiceManager::new(2, 0);
        vm.set_steal_policy(VoiceStealPolicy::Oldest);
        assert_eq!(vm.steal_policy(), VoiceStealPolicy::Oldest);

        vm.set_steal_policy(VoiceStealPolicy::ReleasedFirst);
        assert_eq!(vm.steal_policy(), VoiceStealPolicy::ReleasedFirst);
    }

    /// Builds a pool where voice 0 is the oldest and remains active (not
    /// releasing) while voice 1 is newer and releasing - the scenario where
    /// `Oldest` and `ReleasedFirst` diverge.
    fn pool_with_oldest_active_and_newest_releasing()
    -> (VoiceManager, GeneratorParams, EnvelopeParams, LfoParams) {
        let (g, e, l) = params();
        let e = EnvelopeParams {
            attack: 0.0,
            decay: 0.0,
            sustain: 0.5,
            release: 10.0, // long release so it stays "Releasing" for the test
            ..e
        };
        let mut vm = VoiceManager::new(2, 0);
        vm.prepare(48_000.0, 128);

        vm.note_on(60, 1.0, &g, &e, &l); // voice 0 (oldest, stays active)
        vm.note_on(61, 1.0, &g, &e, &l); // voice 1 (newer)
        vm.note_off(61); // voice 1 starts releasing

        (vm, g, e, l)
    }

    #[test]
    fn released_first_policy_steals_releasing_voice_even_if_newer() {
        let (mut vm, g, e, l) = pool_with_oldest_active_and_newest_releasing();
        vm.set_steal_policy(VoiceStealPolicy::ReleasedFirst);

        vm.note_on(62, 1.0, &g, &e, &l);
        assert_eq!(vm.voice(0).note(), 60, "oldest active voice is preserved");
        assert_eq!(
            vm.voice(1).note(),
            62,
            "newer releasing voice is stolen first"
        );
    }

    #[test]
    fn oldest_policy_steals_oldest_active_voice_even_if_a_newer_voice_is_releasing() {
        let (mut vm, g, e, l) = pool_with_oldest_active_and_newest_releasing();
        vm.set_steal_policy(VoiceStealPolicy::Oldest);

        vm.note_on(62, 1.0, &g, &e, &l);
        assert_eq!(
            vm.voice(0).note(),
            62,
            "oldest voice is stolen regardless of its release state"
        );
        assert_eq!(vm.voice(1).note(), 61, "newer releasing voice is preserved");
    }
}
