//! Renders a short demo through [`GenericSampler`] using the real sampler
//! bank built from `docs/samples/piano.wav` (via
//! `cargo xtask prepare-sampler-bank`), covering root-note pitch tracking,
//! tune, offset, and release across a few notes so the result can be
//! listened to directly.
//!
//! Run `cargo xtask prepare-sampler-bank --source docs/samples/piano.wav
//! --out assets/sampler/piano.bank` first, then
//! `cargo run -p z-audio-examples --example render_generic_sampler`.

use z_audio_dsp::{EventKind, ParamId, ProcessContext, TimedEvent};
use z_audio_examples::wav_writer::{output_path, write_stereo_wav};
use z_audio_synth::{GenericSampler, GenericSamplerConfig};

const SAMPLE_RATE: f32 = 48_000.0;
const BLOCK_SIZE: usize = 128;

fn bank_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../assets/sampler/piano.bank")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = bank_path();
    let bytes = std::fs::read(&path).map_err(|e| {
        format!(
            "could not read '{}': {e}\nrun `cargo xtask prepare-sampler-bank --source docs/samples/piano.wav --out assets/sampler/piano.bank` first",
            path.display()
        )
    })?;
    let bank = z_audio_synth::load_sampler_bank_bytes(&bytes)?;

    let mut sampler = GenericSampler::new(GenericSamplerConfig {
        sample_rate: SAMPLE_RATE,
        max_block_size: BLOCK_SIZE,
        max_polyphony: 8,
    });
    sampler.load_bank(std::sync::Arc::new(bank));

    let sr = SAMPLE_RATE as usize;
    let mut events: Vec<(usize, EventKind)> = Vec::new();

    // Segment 1 (0.0s - 1.0s): root note, default offset/tune.
    events.push((0, EventKind::NoteOn { note: 60, velocity: 0.9 }));
    events.push((sr * 8 / 10, EventKind::NoteOff { note: 60, velocity: 0.0 }));

    // Segment 2 (1.0s - 2.0s): an octave up, exercising pitch tracking.
    events.push((
        sr,
        EventKind::Param {
            id: ParamId::SamplerOffset,
            value: 0.0,
        },
    ));
    events.push((sr, EventKind::NoteOn { note: 72, velocity: 0.9 }));
    events.push((sr * 2 - sr / 5, EventKind::NoteOff { note: 72, velocity: 0.0 }));

    // Segment 3 (2.0s - 3.0s): root note again, with offset skipping ahead
    // into the sample and a +50 cent tune.
    events.push((
        sr * 2,
        EventKind::Param {
            id: ParamId::SamplerOffset,
            value: 0.1,
        },
    ));
    events.push((
        sr * 2,
        EventKind::Param {
            id: ParamId::SamplerTune,
            value: 50.0,
        },
    ));
    events.push((sr * 2, EventKind::NoteOn { note: 60, velocity: 0.9 }));
    events.push((sr * 3 - sr / 5, EventKind::NoteOff { note: 60, velocity: 0.0 }));

    // Segment 4 (3.0s - 5.0s): a short 1-second window of the sample looped
    // with `LoopMode::Sustain` while held for 4 seconds (well past the
    // window's natural length), then released.
    events.push((
        sr * 3,
        EventKind::Param {
            id: ParamId::SamplerTune,
            value: 0.0,
        },
    ));
    events.push((
        sr * 3,
        EventKind::Param {
            id: ParamId::SamplerLoopMode,
            value: z_audio_dsp::LoopMode::Sustain.to_param_value(),
        },
    ));
    events.push((
        sr * 3,
        EventKind::Param {
            id: ParamId::SamplerLoopStart,
            value: 0.2,
        },
    ));
    events.push((
        sr * 3,
        EventKind::Param {
            id: ParamId::SamplerLoopEnd,
            value: 0.22,
        },
    ));
    events.push((
        sr * 3,
        EventKind::Param {
            id: ParamId::SamplerLoopXfade,
            value: 0.02,
        },
    ));
    events.push((sr * 3, EventKind::NoteOn { note: 60, velocity: 0.9 }));
    events.push((sr * 7, EventKind::NoteOff { note: 60, velocity: 0.0 }));

    events.sort_by_key(|(sample, _)| *sample);

    // Total render length includes the sustain-loop segment and a release tail.
    let total_samples = sr * 8;
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
        sampler.process_with_context(
            &ctx,
            &mut left[block_start..block_start + block_len],
            &mut right[block_start..block_start + block_len],
        );
    }

    let out_path = output_path("render_generic_sampler.wav");
    write_stereo_wav(&out_path, SAMPLE_RATE as u32, &left, &right)?;
    println!("wrote {}", out_path.display());
    Ok(())
}
