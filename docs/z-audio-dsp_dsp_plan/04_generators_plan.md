# 04. Generators Plan

## 命名

公開APIは `Generator` に統一します。  
`Oscillator` は内部実装名としては使ってもよいですが、ドキュメント・ユーザー向けには `Generator` を優先します。

## 第一弾 Generator

```rust
pub enum GeneratorKind {
    Sine,
    Triangle,
    Saw,
    Pulse,
    Noise,
}
```

## Analog Generator

```rust
pub struct AnalogGenerator {
    sample_rate: f32,
    phase: f32,
    frequency_hz: f32,
    waveform: AnalogWaveform,
    pulse_width: f32,
}
```

```rust
pub enum AnalogWaveform {
    Sine,
    Triangle,
    Saw,
    Pulse,
}
```

## Noise Generator

第一弾は White Noise のみでよいです。

```rust
pub struct NoiseGenerator {
    rng: SmallRng,
}
```

将来的には以下を追加します。

- Pink
- Brown
- Blue
- Violet
- Velvet
- Sample & Hold

## Band-limiting方針

第一弾では音楽的な使い勝手優先なので、アンチエイリアスは簡易で開始します。

### v1

- Sine: 直接 `sin`
- Triangle: naive
- Saw: naive
- Pulse: naive
- Noise: white

### v1.5

- PolyBLEP saw
- PolyBLEP pulse

### v2

- mipmapped wavetable
- oversampling
- hard sync / FM / PM対応

## 周波数

```rust
pub fn midi_note_to_hz(note: f32) -> f32 {
    440.0 * 2.0_f32.powf((note - 69.0) / 12.0)
}
```

## Phase

phaseは `[0.0, 1.0)` に正規化します。

```rust
phase += frequency_hz / sample_rate;
phase -= phase.floor();
```

## Waveform formulas

### Sine

```rust
(phase * TAU).sin()
```

### Saw

```rust
2.0 * phase - 1.0
```

### Pulse

```rust
if phase < pulse_width { 1.0 } else { -1.0 }
```

### Triangle

```rust
4.0 * (phase - 0.5).abs() - 1.0
```

## Acceptance Criteria

- 48kHz / 128 samples blockで安定動作
- note onで正しいpitchが鳴る
- phase resetあり/なしを選べる
- pulse_width 0.05〜0.95で破綻しない
- noise が -1.0〜1.0 に収まる
- render exampleでwav出力できる
