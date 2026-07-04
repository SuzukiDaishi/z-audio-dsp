//! Golden audio regression tests (docs/09 "Golden audio tests").
//!
//! Snapshots live in `tests/golden/*.json` at the workspace root and store
//! aggregate metrics (RMS, peak, spectral centroid) rather than raw sample
//! data, since exact sample-for-sample reproduction is fragile across
//! platforms/SIMD. Each test regenerates its signal, checks it is free of
//! NaN/Inf, and compares its metrics against the stored snapshot within a
//! tolerance.
//!
//! Run with `-- --ignored regenerate_golden_snapshots` to (re)write the
//! snapshot files after an intentional DSP change.

use std::path::PathBuf;

use z_audio_dsp::{
    AnalogGenerator, AnalogWaveform, BUTTERWORTH_Q, ButterworthKind, Effect, Envelope,
    EnvelopeParams, Generator, Modulator, NoiseGenerator, ProcessContext, ThreeBandButterworthEq,
    midi_note_to_hz,
};

const SAMPLE_RATE: f32 = 48_000.0;
const BLOCK_SIZE: usize = 128;
const CENTROID_WINDOW: usize = 1024;

const RMS_TOLERANCE: f64 = 1e-3;
const PEAK_TOLERANCE: f64 = 1e-3;
const CENTROID_TOLERANCE_HZ: f64 = 5.0;

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden")
}

fn rms(signal: &[f32]) -> f64 {
    let sum_sq: f64 = signal.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum_sq / signal.len() as f64).sqrt()
}

fn peak(signal: &[f32]) -> f64 {
    signal
        .iter()
        .fold(0.0_f64, |acc, &s| acc.max(s.abs() as f64))
}

/// Computes the spectral centroid (Hz) of a `window`-sample slice of
/// `signal` starting at `window_start`, using a Hann-windowed, naive O(n^2)
/// DFT (no FFT crate dependency). The Hann window keeps leakage from a
/// non-bin-aligned tone from dragging the centroid toward Nyquist.
fn spectral_centroid_hz(
    signal: &[f32],
    sample_rate: f32,
    window_start: usize,
    window: usize,
) -> f64 {
    let slice = &signal[window_start..window_start + window];
    let n = slice.len();

    let windowed: Vec<f64> = slice
        .iter()
        .enumerate()
        .map(|(t, &x)| {
            let hann = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * t as f64 / (n - 1) as f64).cos();
            x as f64 * hann
        })
        .collect();

    let mut weighted_sum = 0.0_f64;
    let mut magnitude_sum = 0.0_f64;
    for k in 1..(n / 2) {
        let mut re = 0.0_f64;
        let mut im = 0.0_f64;
        for (t, &x) in windowed.iter().enumerate() {
            let angle = -2.0 * std::f64::consts::PI * k as f64 * t as f64 / n as f64;
            re += x * angle.cos();
            im += x * angle.sin();
        }
        let magnitude = (re * re + im * im).sqrt();
        let freq = k as f64 * sample_rate as f64 / n as f64;
        weighted_sum += freq * magnitude;
        magnitude_sum += magnitude;
    }

    if magnitude_sum > 0.0 {
        weighted_sum / magnitude_sum
    } else {
        0.0
    }
}

fn assert_finite(signal: &[f32]) {
    for &s in signal {
        assert!(s.is_finite(), "non-finite sample: {s}");
    }
}

fn measure(signal: &[f32], centroid_window_start: usize) -> golden_json::Snapshot {
    assert_finite(signal);
    golden_json::Snapshot {
        sample_count: signal.len(),
        rms: rms(signal),
        peak: peak(signal),
        spectral_centroid_hz: spectral_centroid_hz(
            signal,
            SAMPLE_RATE,
            centroid_window_start,
            CENTROID_WINDOW,
        ),
    }
}

fn assert_matches_golden(name: &str, signal: &[f32], centroid_window_start: usize) {
    let measured = measure(signal, centroid_window_start);
    let golden = golden_json::read(&golden_dir().join(format!("{name}.json")));

    assert_eq!(
        measured.sample_count, golden.sample_count,
        "{name}: sample count mismatch"
    );
    assert!(
        (measured.rms - golden.rms).abs() < RMS_TOLERANCE,
        "{name}: rms {} vs golden {}",
        measured.rms,
        golden.rms
    );
    assert!(
        (measured.peak - golden.peak).abs() < PEAK_TOLERANCE,
        "{name}: peak {} vs golden {}",
        measured.peak,
        golden.peak
    );
    assert!(
        (measured.spectral_centroid_hz - golden.spectral_centroid_hz).abs() < CENTROID_TOLERANCE_HZ,
        "{name}: spectral centroid {} vs golden {}",
        measured.spectral_centroid_hz,
        golden.spectral_centroid_hz
    );
}

/// AnalogGenerator Sine @ 440 Hz, 1 second.
fn render_sine_440() -> Vec<f32> {
    let mut osc = AnalogGenerator::new(AnalogWaveform::Sine, 0.0, 0.5);
    osc.prepare(SAMPLE_RATE, BLOCK_SIZE);
    osc.set_frequency_hz(440.0);

    let events = [];
    let total = SAMPLE_RATE as usize;
    let mut signal = vec![0.0_f32; total];
    for chunk in signal.chunks_mut(BLOCK_SIZE) {
        let ctx = ProcessContext::new(SAMPLE_RATE, chunk.len(), 120.0, &events);
        osc.process_mono(&ctx, chunk);
    }
    signal
}

/// AnalogGenerator Saw @ C4 (MIDI 60) through the default ADSR envelope, 0.5
/// seconds, no note-off (stays in sustain).
fn render_saw_c4_env() -> Vec<f32> {
    let mut osc = AnalogGenerator::new(AnalogWaveform::Saw, 0.0, 0.5);
    osc.prepare(SAMPLE_RATE, BLOCK_SIZE);
    osc.set_frequency_hz(midi_note_to_hz(60.0));

    let mut env = Envelope::new(EnvelopeParams::default());
    env.prepare(SAMPLE_RATE, BLOCK_SIZE);
    env.note_on();

    let total = (SAMPLE_RATE * 0.5) as usize;
    let mut signal = vec![0.0_f32; total];
    for sample in &mut signal {
        *sample = osc.next_sample() * env.next_sample();
    }
    signal
}

/// NoiseGenerator through the EQ mid bell (+12 dB, q=4.0), with the low/high
/// bands disabled and the mid frequency sweeping logarithmically from 100 Hz
/// to 6 kHz over 0.5 seconds.
fn render_noise_bp_sweep() -> Vec<f32> {
    let mut noise = NoiseGenerator::new(42);
    noise.prepare(SAMPLE_RATE, BLOCK_SIZE);

    let mut eq = ThreeBandButterworthEq::new();
    eq.low.enabled = false;
    eq.mid.enabled = true;
    eq.high.enabled = false;
    eq.mid.kind = ButterworthKind::BandPass;
    eq.mid.gain_db = 12.0;
    eq.mid.q = 4.0;
    eq.prepare(SAMPLE_RATE, BLOCK_SIZE);

    let total = (SAMPLE_RATE * 0.5) as usize;
    let mut left = vec![0.0_f32; total];
    let mut right = vec![0.0_f32; total];

    let events = [];
    let log_start = 100.0_f32.ln();
    let log_end = 6_000.0_f32.ln();
    for (block_index, (l_chunk, r_chunk)) in left
        .chunks_mut(BLOCK_SIZE)
        .zip(right.chunks_mut(BLOCK_SIZE))
        .enumerate()
    {
        for sample in l_chunk.iter_mut() {
            *sample = noise.next_sample();
        }
        r_chunk.copy_from_slice(l_chunk);

        let block_start = block_index * BLOCK_SIZE;
        let t = block_start as f32 / total as f32;
        eq.mid.frequency_hz = (log_start + (log_end - log_start) * t).exp();

        let ctx = ProcessContext::new(SAMPLE_RATE, l_chunk.len(), 120.0, &events);
        eq.process_stereo(&ctx, l_chunk, r_chunk);
    }
    left
}

/// 3-band EQ impulse response with only the low band enabled as a -12 dB low
/// shelf at 1 kHz.
fn render_eq_impulse_lp_1khz() -> Vec<f32> {
    let mut eq = ThreeBandButterworthEq::new();
    eq.low.enabled = true;
    eq.mid.enabled = false;
    eq.high.enabled = false;
    eq.low.kind = ButterworthKind::LowPass;
    eq.low.frequency_hz = 1_000.0;
    eq.low.gain_db = -12.0;
    eq.low.q = BUTTERWORTH_Q;
    eq.prepare(SAMPLE_RATE, CENTROID_WINDOW);

    let mut left = vec![0.0_f32; CENTROID_WINDOW];
    let mut right = vec![0.0_f32; CENTROID_WINDOW];
    left[0] = 1.0;
    right[0] = 1.0;

    let events = [];
    let ctx = ProcessContext::new(SAMPLE_RATE, left.len(), 120.0, &events);
    eq.process_stereo(&ctx, &mut left, &mut right);
    left
}

#[test]
fn sine_440_1sec() {
    let signal = render_sine_440();
    assert_matches_golden("sine_440_1sec", &signal, 4096);
}

#[test]
fn saw_c4_env() {
    let signal = render_saw_c4_env();
    assert_matches_golden("saw_c4_env", &signal, 12_000);
}

#[test]
fn noise_bp_sweep() {
    let signal = render_noise_bp_sweep();
    assert_matches_golden("noise_bp_sweep", &signal, 12_000);
}

#[test]
fn eq_impulse_lp_1khz() {
    let signal = render_eq_impulse_lp_1khz();
    assert_matches_golden("eq_impulse_lp_1khz", &signal, 0);
}

#[test]
#[ignore = "run manually to (re)write tests/golden/*.json after an intentional DSP change"]
fn regenerate_golden_snapshots() {
    let snapshots: [(&str, Vec<f32>, &str, usize); 4] = [
        (
            "sine_440_1sec",
            render_sine_440(),
            "AnalogGenerator Sine 440Hz @ 48kHz, 1 second",
            4096,
        ),
        (
            "saw_c4_env",
            render_saw_c4_env(),
            "AnalogGenerator Saw @ C4 (MIDI 60) through the default ADSR envelope, 0.5 seconds",
            12_000,
        ),
        (
            "noise_bp_sweep",
            render_noise_bp_sweep(),
            "NoiseGenerator through EQ mid bell (+12dB, q=4.0), 100Hz -> 6kHz log sweep, 0.5 seconds",
            12_000,
        ),
        (
            "eq_impulse_lp_1khz",
            render_eq_impulse_lp_1khz(),
            "3-band EQ impulse response, low shelf -12dB @ 1kHz",
            0,
        ),
    ];

    for (name, signal, description, centroid_window_start) in snapshots {
        let snapshot = measure(&signal, centroid_window_start);
        golden_json::write(
            &golden_dir().join(format!("{name}.json")),
            description,
            &snapshot,
        );
    }
}

/// Minimal hand-rolled JSON read/write for golden snapshot files (no
/// `serde_json` dependency). The format is a small, fixed set of top-level
/// scalar fields, so a tiny key-lookup parser is sufficient.
mod golden_json {
    use std::path::Path;

    pub struct Snapshot {
        pub sample_count: usize,
        pub rms: f64,
        pub peak: f64,
        pub spectral_centroid_hz: f64,
    }

    pub fn write(path: &Path, description: &str, snapshot: &Snapshot) {
        let json = format!(
            "{{\n  \"description\": \"{description}\",\n  \"sample_count\": {},\n  \"rms\": {:.8},\n  \"peak\": {:.8},\n  \"spectral_centroid_hz\": {:.4}\n}}\n",
            snapshot.sample_count, snapshot.rms, snapshot.peak, snapshot.spectral_centroid_hz
        );
        std::fs::write(path, json).expect("failed to write golden snapshot");
    }

    pub fn read(path: &Path) -> Snapshot {
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("failed to read golden snapshot {}: {e}", path.display()));
        Snapshot {
            sample_count: number(&text, "sample_count") as usize,
            rms: number(&text, "rms"),
            peak: number(&text, "peak"),
            spectral_centroid_hz: number(&text, "spectral_centroid_hz"),
        }
    }

    /// Extracts the numeric value following `"key":` in the document.
    fn number(json: &str, key: &str) -> f64 {
        let needle = format!("\"{key}\":");
        let start = json
            .find(&needle)
            .unwrap_or_else(|| panic!("missing key \"{key}\" in golden snapshot"))
            + needle.len();
        let rest = json[start..].trim_start();
        let end = rest
            .find(|c: char| !(c.is_ascii_digit() || matches!(c, '-' | '.' | 'e' | 'E' | '+')))
            .unwrap_or(rest.len());
        rest[..end]
            .parse()
            .unwrap_or_else(|_| panic!("invalid number for key \"{key}\""))
    }
}
