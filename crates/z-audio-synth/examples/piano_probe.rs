use std::{
    fs::{self, File},
    io::{self, BufWriter, Write},
    path::Path,
};

use z_audio_dsp::midi_note_to_hz;
use z_audio_synth::{FormulaPiano, FormulaPianoConfig, ParamId};

const SAMPLE_RATE: f32 = 48_000.0;
const RENDER_SECONDS: f32 = 2.2;
const ANALYSIS_START_SECONDS: f32 = 0.08;
const ANALYSIS_SECONDS: f32 = 1.10;
const NOTES: [u8; 10] = [33, 36, 45, 48, 57, 60, 69, 72, 81, 84];
const REFERENCE_C4_NOTE: u8 = 60;

#[derive(Debug, Clone, Copy)]
struct ProbePreset {
    name: &'static str,
    body_amount: f32,
    pedal_resonance: f32,
    sympathetic_amount: f32,
}

fn main() -> io::Result<()> {
    let out_dir = Path::new("target").join("piano-debug");
    fs::create_dir_all(&out_dir)?;

    let presets = [
        ProbePreset {
            name: "default",
            body_amount: 0.08,
            pedal_resonance: 0.0,
            sympathetic_amount: 0.0,
        },
        ProbePreset {
            name: "body_min",
            body_amount: 0.0,
            pedal_resonance: 0.0,
            sympathetic_amount: 0.0,
        },
    ];

    let mut summary = String::new();
    summary.push_str("# Piano Probe\n\n");
    summary.push_str(
        "| preset | note | f0 Hz | peak Hz | peak/f0 | RMS | low/f0 | f0 | p2/f0 | p3/f0 | p4/f0 | p8/f0 |\n",
    );
    summary.push_str(
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n",
    );

    for preset in presets {
        for note in NOTES {
            let rendered = render_note(note, preset);
            let stem = format!("{}_note_{note}", preset.name);
            write_wav16(&out_dir.join(format!("{stem}.wav")), &rendered)?;
            write_spectrogram_bmp(&out_dir.join(format!("{stem}_spectrogram.bmp")), &rendered)?;
            let analysis = analyze_note(note, &rendered);
            write_harmonics_csv(&out_dir.join(format!("{stem}_harmonics.csv")), &analysis)?;
            summary.push_str(&format!(
                "| {} | {} | {:.2} | {:.2} | {:.3} | {:.6} | {:.3} | {:.6} | {:.3} | {:.3} | {:.3} | {:.3} |\n",
                preset.name,
                note,
                analysis.fundamental_hz,
                analysis.peak_hz,
                analysis.peak_hz / analysis.fundamental_hz,
                analysis.rms,
                analysis.low_to_f0,
                analysis.partials[0],
                ratio(analysis.partials[1], analysis.partials[0]),
                ratio(analysis.partials[2], analysis.partials[0]),
                ratio(analysis.partials[3], analysis.partials[0]),
                ratio(analysis.partials[7], analysis.partials[0]),
            ));
        }
    }

    fs::write(out_dir.join("summary.md"), summary)?;
    if let Some(reference_path) = reference_sample_path() {
        let reference = read_wav_mono(&reference_path)?;
        let events = analyze_reference_events(&reference.samples, reference.sample_rate);
        write_reference_outputs(&out_dir, &reference_path, &reference, &events)?;
        if let Some(c4) = select_reference_c4(&events) {
            let synth = render_note(REFERENCE_C4_NOTE, presets[0]);
            let synth_analysis = analyze_note(REFERENCE_C4_NOTE, &synth);
            write_c4_comparison(&out_dir, c4, &synth_analysis)?;
        }
    }
    println!("wrote {}", out_dir.display());
    Ok(())
}

fn render_note(note: u8, preset: ProbePreset) -> Vec<f32> {
    let samples = (SAMPLE_RATE * RENDER_SECONDS) as usize;
    let mut piano = FormulaPiano::new(FormulaPianoConfig {
        sample_rate: SAMPLE_RATE,
        max_block_size: 128,
        max_polyphony: 8,
    });
    piano.set_param(ParamId::PianoBodyAmount, preset.body_amount);
    piano.set_param(ParamId::PianoPedalResonance, preset.pedal_resonance);
    piano.set_param(ParamId::PianoSympatheticAmount, preset.sympathetic_amount);
    piano.set_param(ParamId::PianoMasterGain, -6.0);
    piano.note_on(note, 0.95);

    let mut out = Vec::with_capacity(samples);
    let mut left = [0.0_f32; 128];
    let mut right = [0.0_f32; 128];
    while out.len() < samples {
        piano.process(&mut left, &mut right);
        for (l, r) in left.iter().zip(right.iter()) {
            if out.len() == samples {
                break;
            }
            out.push((l + r) * 0.5);
        }
    }
    out
}

#[derive(Debug)]
struct NoteAnalysis {
    fundamental_hz: f32,
    peak_hz: f32,
    rms: f32,
    low_to_f0: f32,
    partials: [f32; 12],
    decay_50_sec: Option<f32>,
    decay_25_sec: Option<f32>,
    decay_10_sec: Option<f32>,
    centroid_hz: f32,
    rolloff_85_hz: f32,
}

fn analyze_note(note: u8, samples: &[f32]) -> NoteAnalysis {
    let fundamental_hz = midi_note_to_hz(note as f32);
    let start = (ANALYSIS_START_SECONDS * SAMPLE_RATE) as usize;
    let len = (ANALYSIS_SECONDS * SAMPLE_RATE) as usize;
    let window = &samples[start..(start + len).min(samples.len())];
    let rms = (window.iter().map(|s| s * s).sum::<f32>() / window.len() as f32).sqrt();

    let mut partials = [0.0_f32; 12];
    for (i, partial) in partials.iter_mut().enumerate() {
        *partial = partial_mag(window, fundamental_hz, i + 1);
    }
    let peak_hz = strongest_peak_hz(window, fundamental_hz);
    let (centroid_hz, rolloff_85_hz) = spectral_stats(window);

    let low_energy = log_band_energy(window, 24.0, fundamental_hz * 0.72, 40);
    NoteAnalysis {
        fundamental_hz,
        peak_hz,
        rms,
        low_to_f0: ratio(low_energy.sqrt(), partials[0]),
        partials,
        decay_50_sec: decay_time(samples, 0, 0.50),
        decay_25_sec: decay_time(samples, 0, 0.25),
        decay_10_sec: decay_time(samples, 0, 0.10),
        centroid_hz,
        rolloff_85_hz,
    }
}

fn write_harmonics_csv(path: &Path, analysis: &NoteAnalysis) -> io::Result<()> {
    let mut file = BufWriter::new(File::create(path)?);
    writeln!(file, "partial,frequency_hz,magnitude,ratio_to_f0")?;
    for (index, mag) in analysis.partials.iter().enumerate() {
        writeln!(
            file,
            "{},{:.5},{:.9},{:.6}",
            index + 1,
            analysis.fundamental_hz * (index as f32 + 1.0),
            mag,
            ratio(*mag, analysis.partials[0])
        )?;
    }
    Ok(())
}

fn write_wav16(path: &Path, samples: &[f32]) -> io::Result<()> {
    let mut file = BufWriter::new(File::create(path)?);
    let data_len = samples.len() as u32 * 2;
    file.write_all(b"RIFF")?;
    file.write_all(&(36 + data_len).to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16_u32.to_le_bytes())?;
    file.write_all(&1_u16.to_le_bytes())?;
    file.write_all(&1_u16.to_le_bytes())?;
    file.write_all(&(SAMPLE_RATE as u32).to_le_bytes())?;
    file.write_all(&((SAMPLE_RATE as u32) * 2).to_le_bytes())?;
    file.write_all(&2_u16.to_le_bytes())?;
    file.write_all(&16_u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&data_len.to_le_bytes())?;
    for sample in samples {
        let v = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        file.write_all(&v.to_le_bytes())?;
    }
    Ok(())
}

struct WavMono {
    sample_rate: f32,
    samples: Vec<f32>,
}

#[derive(Debug, Clone)]
struct ReferenceEvent {
    onset_sec: f32,
    f0_hz: f32,
    midi_note: f32,
    rms: f32,
    partials: [f32; 12],
    decay_50_sec: Option<f32>,
    decay_25_sec: Option<f32>,
    decay_10_sec: Option<f32>,
    centroid_hz: f32,
    rolloff_85_hz: f32,
}

fn reference_sample_path() -> Option<std::path::PathBuf> {
    [
        Path::new("docs/samples/piano.wav"),
        Path::new("../../docs/samples/piano.wav"),
    ]
    .into_iter()
    .find(|path| path.exists())
    .map(Path::to_path_buf)
}

fn read_wav_mono(path: &Path) -> io::Result<WavMono> {
    let bytes = fs::read(path)?;
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "expected RIFF/WAVE file",
        ));
    }

    let mut offset = 12usize;
    let mut channels = 0u16;
    let mut sample_rate = 0u32;
    let mut bits_per_sample = 0u16;
    let mut audio_format = 0u16;
    let mut data: &[u8] = &[];
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let len = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
        let start = offset + 8;
        let end = start.saturating_add(len).min(bytes.len());
        match id {
            b"fmt " if len >= 16 => {
                audio_format = u16::from_le_bytes(bytes[start..start + 2].try_into().unwrap());
                channels = u16::from_le_bytes(bytes[start + 2..start + 4].try_into().unwrap());
                sample_rate = u32::from_le_bytes(bytes[start + 4..start + 8].try_into().unwrap());
                bits_per_sample =
                    u16::from_le_bytes(bytes[start + 14..start + 16].try_into().unwrap());
            }
            b"data" => data = &bytes[start..end],
            _ => {}
        }
        offset = end + (len & 1);
    }

    if audio_format != 1 || channels == 0 || sample_rate == 0 || data.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "expected PCM WAV with fmt and data chunks",
        ));
    }
    if sample_rate != SAMPLE_RATE as u32 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "piano_probe expects a 48 kHz reference WAV",
        ));
    }

    let bytes_per_sample = (bits_per_sample / 8) as usize;
    let frame_bytes = bytes_per_sample * channels as usize;
    let frames = data.len() / frame_bytes;
    let mut samples = Vec::with_capacity(frames);
    for frame in 0..frames {
        let mut sum = 0.0;
        for channel in 0..channels as usize {
            let index = frame * frame_bytes + channel * bytes_per_sample;
            sum += match bits_per_sample {
                16 => {
                    let value = i16::from_le_bytes(data[index..index + 2].try_into().unwrap());
                    value as f32 / 32768.0
                }
                24 => {
                    let b0 = data[index] as i32;
                    let b1 = data[index + 1] as i32;
                    let b2 = data[index + 2] as i32;
                    let mut value = b0 | (b1 << 8) | (b2 << 16);
                    if value & 0x80_0000 != 0 {
                        value -= 0x100_0000;
                    }
                    value as f32 / 8_388_608.0
                }
                32 => {
                    let value = i32::from_le_bytes(data[index..index + 4].try_into().unwrap());
                    value as f32 / 2_147_483_648.0
                }
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "unsupported PCM bit depth",
                    ));
                }
            };
        }
        samples.push(sum / channels as f32);
    }

    Ok(WavMono {
        sample_rate: sample_rate as f32,
        samples,
    })
}

fn analyze_reference_events(samples: &[f32], sample_rate: f32) -> Vec<ReferenceEvent> {
    let win = (sample_rate * 0.010) as usize;
    let frames = samples.len() / win;
    if frames == 0 {
        return Vec::new();
    }

    let mut env = Vec::with_capacity(frames);
    for frame in 0..frames {
        let start = frame * win;
        let sum = samples[start..start + win]
            .iter()
            .map(|s| s * s)
            .sum::<f32>();
        env.push((sum / win as f32).sqrt());
    }
    let mut smoothed = env.clone();
    for i in 1..env.len().saturating_sub(1) {
        smoothed[i] = (env[i - 1] + env[i] + env[i + 1]) / 3.0;
    }
    let max_env = smoothed.iter().copied().fold(0.0_f32, f32::max);
    let threshold = max_env * 0.12;

    let mut peaks = Vec::new();
    for i in 2..smoothed.len().saturating_sub(2) {
        if smoothed[i] > threshold
            && smoothed[i] >= smoothed[i - 1]
            && smoothed[i] >= smoothed[i + 1]
            && smoothed[i] >= smoothed[i - 2]
            && smoothed[i] >= smoothed[i + 2]
        {
            if peaks.last().map_or(true, |last| i - *last > 25) {
                peaks.push(i);
            } else if let Some(last) = peaks.last_mut() {
                if smoothed[i] > smoothed[*last] {
                    *last = i;
                }
            }
        }
    }

    peaks
        .into_iter()
        .filter_map(|peak| {
            let onset_sample = peak * win;
            let analysis_start = onset_sample + (sample_rate * 0.060) as usize;
            let analysis_len = (sample_rate * 0.70) as usize;
            if analysis_start + 2048 >= samples.len() {
                return None;
            }
            let end = (analysis_start + analysis_len).min(samples.len());
            let window = &samples[analysis_start..end];
            let (f0_hz, midi_note) = estimate_midi_f0(window);
            let mut partials = [0.0; 12];
            for (i, partial) in partials.iter_mut().enumerate() {
                *partial = partial_mag(window, f0_hz, i + 1);
            }
            let rms = (window.iter().map(|s| s * s).sum::<f32>() / window.len() as f32).sqrt();
            let (centroid_hz, rolloff_85_hz) = spectral_stats(window);
            Some(ReferenceEvent {
                onset_sec: onset_sample as f32 / sample_rate,
                f0_hz,
                midi_note,
                rms,
                partials,
                decay_50_sec: decay_time(samples, onset_sample, 0.50),
                decay_25_sec: decay_time(samples, onset_sample, 0.25),
                decay_10_sec: decay_time(samples, onset_sample, 0.10),
                centroid_hz,
                rolloff_85_hz,
            })
        })
        .collect()
}

fn estimate_midi_f0(samples: &[f32]) -> (f32, f32) {
    let mut best_score = -1.0;
    let mut best_note = 60.0;
    for midi in 24..=96 {
        let f0 = midi_note_to_hz(midi as f32);
        let mut score = 0.0;
        for harmonic in 1..=10 {
            let frequency = f0 * harmonic as f32;
            if frequency >= SAMPLE_RATE * 0.45 {
                break;
            }
            score += goertzel_mag(samples, frequency) / (harmonic as f32).powf(0.45);
        }
        if score > best_score {
            best_score = score;
            best_note = midi as f32;
        }
    }
    (midi_note_to_hz(best_note), best_note)
}

fn select_reference_c4(events: &[ReferenceEvent]) -> Option<&ReferenceEvent> {
    events
        .iter()
        .filter(|event| (event.midi_note - REFERENCE_C4_NOTE as f32).abs() <= 0.5)
        .max_by(|a, b| {
            a.rms
                .partial_cmp(&b.rms)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .or_else(|| {
            events.iter().min_by(|a, b| {
                (a.midi_note - REFERENCE_C4_NOTE as f32)
                    .abs()
                    .partial_cmp(&(b.midi_note - REFERENCE_C4_NOTE as f32).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        })
}

fn write_reference_outputs(
    out_dir: &Path,
    source_path: &Path,
    reference: &WavMono,
    events: &[ReferenceEvent],
) -> io::Result<()> {
    let mut summary = String::new();
    summary.push_str("# Piano Reference Probe\n\n");
    summary.push_str(&format!(
        "- source: `{}`\n- sample_rate: {:.0} Hz\n- duration: {:.3} s\n- detected_events: {}\n\n",
        source_path.display(),
        reference.sample_rate,
        reference.samples.len() as f32 / reference.sample_rate,
        events.len()
    ));
    summary.push_str("| idx | onset s | f0 Hz | midi | RMS | p2/f0 | p3/f0 | p4/f0 | decay50 | decay25 | decay10 | centroid | rolloff85 |\n");
    summary.push_str("| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for (index, event) in events.iter().enumerate() {
        summary.push_str(&format!(
            "| {} | {:.3} | {:.2} | {:.2} | {:.6} | {:.3} | {:.3} | {:.3} | {} | {} | {} | {:.1} | {:.1} |\n",
            index,
            event.onset_sec,
            event.f0_hz,
            event.midi_note,
            event.rms,
            ratio(event.partials[1], event.partials[0]),
            ratio(event.partials[2], event.partials[0]),
            ratio(event.partials[3], event.partials[0]),
            fmt_opt(event.decay_50_sec),
            fmt_opt(event.decay_25_sec),
            fmt_opt(event.decay_10_sec),
            event.centroid_hz,
            event.rolloff_85_hz,
        ));
    }
    fs::write(out_dir.join("reference_summary.md"), summary)?;

    if let Some(c4) = select_reference_c4(events) {
        let mut csv = String::from("partial,frequency_hz,magnitude,ratio_to_f0\n");
        for (index, mag) in c4.partials.iter().enumerate() {
            csv.push_str(&format!(
                "{},{:.5},{:.9},{:.6}\n",
                index + 1,
                c4.f0_hz * (index as f32 + 1.0),
                mag,
                ratio(*mag, c4.partials[0])
            ));
        }
        fs::write(out_dir.join("reference_partials.csv"), csv)?;
    }
    Ok(())
}

fn write_c4_comparison(
    out_dir: &Path,
    reference: &ReferenceEvent,
    synth: &NoteAnalysis,
) -> io::Result<()> {
    let mut summary = String::new();
    summary.push_str("# C4 Reference Comparison\n\n");
    summary.push_str("| source | f0 Hz | peak/f0 | p2/f0 | p3/f0 | p4/f0 | decay50 | decay25 | decay10 | centroid | rolloff85 |\n");
    summary.push_str(
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n",
    );
    summary.push_str(&format!(
        "| reference | {:.2} | {:.3} | {:.3} | {:.3} | {:.3} | {} | {} | {} | {:.1} | {:.1} |\n",
        reference.f0_hz,
        1.0,
        ratio(reference.partials[1], reference.partials[0]),
        ratio(reference.partials[2], reference.partials[0]),
        ratio(reference.partials[3], reference.partials[0]),
        fmt_opt(reference.decay_50_sec),
        fmt_opt(reference.decay_25_sec),
        fmt_opt(reference.decay_10_sec),
        reference.centroid_hz,
        reference.rolloff_85_hz,
    ));
    summary.push_str(&format!(
        "| synth | {:.2} | {:.3} | {:.3} | {:.3} | {:.3} | {} | {} | {} | {:.1} | {:.1} |\n",
        synth.fundamental_hz,
        synth.peak_hz / synth.fundamental_hz,
        ratio(synth.partials[1], synth.partials[0]),
        ratio(synth.partials[2], synth.partials[0]),
        ratio(synth.partials[3], synth.partials[0]),
        fmt_opt(synth.decay_50_sec),
        fmt_opt(synth.decay_25_sec),
        fmt_opt(synth.decay_10_sec),
        synth.centroid_hz,
        synth.rolloff_85_hz,
    ));
    fs::write(out_dir.join("c4_reference_comparison.md"), summary)
}

fn write_spectrogram_bmp(path: &Path, samples: &[f32]) -> io::Result<()> {
    let window_size = 2048usize;
    let hop = 512usize;
    let bins = 128usize;
    let frames = samples.len().saturating_sub(window_size) / hop;
    let mut powers = vec![0.0_f32; frames * bins];
    let mut max_db = -120.0_f32;

    for frame in 0..frames {
        let start = frame * hop;
        let window = &samples[start..start + window_size];
        for bin in 0..bins {
            let frequency = log_lerp(25.0, 12_000.0, bin as f32 / (bins - 1) as f32);
            let mag = goertzel_mag(window, frequency);
            let db = 20.0 * mag.max(1.0e-9).log10();
            powers[frame * bins + bin] = db;
            max_db = max_db.max(db);
        }
    }

    let min_db = max_db - 72.0;
    write_bmp(path, frames.max(1), bins, |x, y| {
        let bin = y;
        let db = powers[x * bins + bin];
        let t = ((db - min_db) / (max_db - min_db)).clamp(0.0, 1.0);
        heat(t)
    })?;
    Ok(())
}

fn write_bmp(
    path: &Path,
    width: usize,
    height: usize,
    mut pixel: impl FnMut(usize, usize) -> (u8, u8, u8),
) -> io::Result<()> {
    let row_stride = (width * 3).div_ceil(4) * 4;
    let pixel_bytes = row_stride * height;
    let file_bytes = 54 + pixel_bytes;
    let mut file = BufWriter::new(File::create(path)?);

    file.write_all(b"BM")?;
    file.write_all(&(file_bytes as u32).to_le_bytes())?;
    file.write_all(&[0; 4])?;
    file.write_all(&54_u32.to_le_bytes())?;
    file.write_all(&40_u32.to_le_bytes())?;
    file.write_all(&(width as i32).to_le_bytes())?;
    file.write_all(&(height as i32).to_le_bytes())?;
    file.write_all(&1_u16.to_le_bytes())?;
    file.write_all(&24_u16.to_le_bytes())?;
    file.write_all(&0_u32.to_le_bytes())?;
    file.write_all(&(pixel_bytes as u32).to_le_bytes())?;
    file.write_all(&2835_i32.to_le_bytes())?;
    file.write_all(&2835_i32.to_le_bytes())?;
    file.write_all(&0_u32.to_le_bytes())?;
    file.write_all(&0_u32.to_le_bytes())?;

    let pad = vec![0_u8; row_stride - width * 3];
    for file_y in 0..height {
        let y = file_y;
        for x in 0..width {
            let (r, g, b) = pixel(x, y);
            file.write_all(&[b, g, r])?;
        }
        file.write_all(&pad)?;
    }
    Ok(())
}

fn goertzel_mag(samples: &[f32], frequency_hz: f32) -> f32 {
    let step = core::f32::consts::TAU * frequency_hz / SAMPLE_RATE;
    let mut re = 0.0;
    let mut im = 0.0;
    for (i, sample) in samples.iter().enumerate() {
        let w = hann(i, samples.len());
        let phase = step * i as f32;
        re += sample * w * phase.cos();
        im -= sample * w * phase.sin();
    }
    (re.mul_add(re, im * im).sqrt() * 4.0) / samples.len() as f32
}

fn partial_mag(samples: &[f32], fundamental_hz: f32, harmonic: usize) -> f32 {
    let nominal = fundamental_hz * harmonic as f32;
    if nominal >= SAMPLE_RATE * 0.45 {
        return 0.0;
    }
    let width = if harmonic == 1 { 0.006 } else { 0.022 };
    let mut max_mag = 0.0;
    for i in 0..25 {
        let t = i as f32 / 24.0;
        let frequency = nominal * (1.0 - width + 2.0 * width * t);
        max_mag = f32::max(max_mag, goertzel_mag(samples, frequency));
    }
    max_mag
}

fn log_band_energy(samples: &[f32], low_hz: f32, high_hz: f32, bins: usize) -> f32 {
    if high_hz <= low_hz {
        return 0.0;
    }
    let mut sum = 0.0;
    for i in 0..bins {
        let t = i as f32 / (bins - 1).max(1) as f32;
        let frequency = log_lerp(low_hz, high_hz, t);
        let mag = goertzel_mag(samples, frequency);
        sum += mag * mag;
    }
    sum / bins as f32
}

fn decay_time(samples: &[f32], start_sample: usize, percent: f32) -> Option<f32> {
    let win = (SAMPLE_RATE * 0.010) as usize;
    if start_sample + win >= samples.len() {
        return None;
    }
    let frames = (samples.len() - start_sample) / win;
    let mut env = Vec::with_capacity(frames);
    for frame in 0..frames {
        let start = start_sample + frame * win;
        let sum = samples[start..start + win]
            .iter()
            .map(|s| s * s)
            .sum::<f32>();
        env.push((sum / win as f32).sqrt());
    }
    let (peak_index, peak) =
        env.iter()
            .copied()
            .enumerate()
            .fold(
                (0usize, 0.0_f32),
                |best, item| {
                    if item.1 > best.1 { item } else { best }
                },
            );
    let threshold = peak * percent;
    env.iter()
        .enumerate()
        .skip(peak_index)
        .find(|(_, value)| **value <= threshold)
        .map(|(index, _)| (index - peak_index) as f32 * win as f32 / SAMPLE_RATE)
}

fn spectral_stats(samples: &[f32]) -> (f32, f32) {
    if samples.is_empty() {
        return (0.0, 0.0);
    }
    let mut total = 0.0;
    let mut weighted = 0.0;
    let mut bins = Vec::with_capacity(192);
    for bin in 0..192 {
        let frequency = log_lerp(25.0, 12_000.0, bin as f32 / 191.0);
        let mag = goertzel_mag(samples, frequency);
        bins.push((frequency, mag));
        total += mag;
        weighted += frequency * mag;
    }
    if total <= 1.0e-12 {
        return (0.0, 0.0);
    }
    let centroid = weighted / total;
    let mut cumulative = 0.0;
    let mut rolloff = bins.last().map(|(frequency, _)| *frequency).unwrap_or(0.0);
    for (frequency, mag) in bins {
        cumulative += mag;
        if cumulative >= total * 0.85 {
            rolloff = frequency;
            break;
        }
    }
    (centroid, rolloff)
}

fn strongest_peak_hz(samples: &[f32], fundamental_hz: f32) -> f32 {
    let low = (fundamental_hz * 0.45).max(24.0);
    let high = (fundamental_hz * 5.2).min(8_000.0);
    let mut peak_hz = fundamental_hz;
    let mut peak_mag = 0.0;
    for i in 0..240 {
        let t = i as f32 / 239.0;
        let frequency = log_lerp(low, high, t);
        let mag = goertzel_mag(samples, frequency);
        if mag > peak_mag {
            peak_mag = mag;
            peak_hz = frequency;
        }
    }
    peak_hz
}

fn hann(index: usize, len: usize) -> f32 {
    if len <= 1 {
        return 1.0;
    }
    0.5 - 0.5 * (core::f32::consts::TAU * index as f32 / (len - 1) as f32).cos()
}

fn ratio(value: f32, base: f32) -> f32 {
    value / base.max(1.0e-9)
}

fn fmt_opt(value: Option<f32>) -> String {
    value
        .map(|value| format!("{value:.3}"))
        .unwrap_or_else(|| "-".to_string())
}

fn log_lerp(a: f32, b: f32, t: f32) -> f32 {
    (a.ln() + (b.ln() - a.ln()) * t.clamp(0.0, 1.0)).exp()
}

fn heat(t: f32) -> (u8, u8, u8) {
    let r = (255.0 * smoothstep(0.45, 1.0, t)) as u8;
    let g = (255.0 * smoothstep(0.16, 0.78, t) * (1.0 - 0.30 * smoothstep(0.82, 1.0, t))) as u8;
    let b = (255.0 * (0.18 + 0.82 * smoothstep(0.02, 0.42, t)) * (1.0 - smoothstep(0.55, 1.0, t)))
        as u8;
    (r, g, b)
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
