//! Renders a short demo through [`SimpleSynth`]'s full signal chain
//! (`VoiceManager -> Voice Sum -> 3-band EQ -> master gain`), covering the
//! Phase 1 MVP completion checks: a single note, a chord, and a rapid note
//! sequence.

use z_audio_dsp::{EventKind, ProcessContext, TimedEvent};
use z_audio_examples::wav_writer::{output_path, write_stereo_wav};
use z_audio_synth::{SimpleSynth, SimpleSynthConfig, midi};

const SAMPLE_RATE: f32 = 48_000.0;
const BLOCK_SIZE: usize = 128;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut synth = SimpleSynth::new(SimpleSynthConfig {
        sample_rate: SAMPLE_RATE,
        max_block_size: BLOCK_SIZE,
        max_polyphony: 8,
    });

    let sr = SAMPLE_RATE as usize;
    let mut events = Vec::new();

    // Segment 1 (0.0s - 1.0s): a single note.
    events.push((
        0,
        EventKind::NoteOn {
            note: midi::C4,
            velocity: 0.9,
        },
    ));
    events.push((
        sr * 8 / 10,
        EventKind::NoteOff {
            note: midi::C4,
            velocity: 0.0,
        },
    ));

    // Segment 2 (1.0s - 2.0s): a major chord.
    let chord = [midi::C4, midi::C4 + 4, midi::C4 + 7];
    for &note in &chord {
        events.push((
            sr,
            EventKind::NoteOn {
                note,
                velocity: 0.8,
            },
        ));
    }
    for &note in &chord {
        events.push((
            sr * 2 - sr / 5,
            EventKind::NoteOff {
                note,
                velocity: 0.0,
            },
        ));
    }

    // Segment 3 (2.0s - 3.0s): a rapid ascending note sequence.
    let run = [
        midi::C4,
        midi::C4 + 2,
        midi::C4 + 4,
        midi::C4 + 5,
        midi::C4 + 7,
        midi::C4 + 9,
        midi::C4 + 11,
        midi::C4 + 12,
    ];
    let step = sr / 8; // ~125ms per note
    for (i, &note) in run.iter().enumerate() {
        let on = sr * 2 + i * step;
        let off = on + step * 9 / 10;
        events.push((
            on,
            EventKind::NoteOn {
                note,
                velocity: 0.9,
            },
        ));
        events.push((
            off,
            EventKind::NoteOff {
                note,
                velocity: 0.0,
            },
        ));
    }

    events.sort_by_key(|(sample, _)| *sample);

    // Total render length includes a 1-second tail for the final release.
    let total_samples = sr * 4;
    let mut left = vec![0.0_f32; total_samples];
    let mut right = vec![0.0_f32; total_samples];

    let mut event_cursor = 0;
    let mut block_events = Vec::new();
    for block_start in (0..total_samples).step_by(BLOCK_SIZE) {
        let block_len = BLOCK_SIZE.min(total_samples - block_start);

        block_events.clear();
        while event_cursor < events.len() && events[event_cursor].0 < block_start + block_len {
            let (sample, kind) = events[event_cursor];
            block_events.push(TimedEvent {
                sample_offset: sample - block_start,
                kind,
            });
            event_cursor += 1;
        }

        let ctx = ProcessContext::new(SAMPLE_RATE, block_len, 120.0, &block_events);
        synth.process_with_context(
            &ctx,
            &mut left[block_start..block_start + block_len],
            &mut right[block_start..block_start + block_len],
        );
    }

    let path = output_path("render_simple_synth.wav");
    write_stereo_wav(&path, SAMPLE_RATE as u32, &left, &right)?;
    println!("wrote {}", path.display());
    Ok(())
}
