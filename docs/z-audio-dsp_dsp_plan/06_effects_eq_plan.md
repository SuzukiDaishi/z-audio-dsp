# 06. Effects / 3-band Butterworth EQ Plan

## 第一弾 Effects

- Gain
- 3-band Butterworth EQ

## 注意: lowpass / bandpass / lowpass について

ユーザー指定では `butterworth lowpass, bandpass, lowpass` とありました。  
ただし3バンドEQとして自然なのは `lowpass, bandpass, highpass` です。

仕様としては、各Bandが `ButterworthKind` を持つようにして、デフォルトを以下にします。

```text
Low  : LowPass
Mid  : BandPass
High : HighPass
```

必要なら third band を LowPass にするプリセットも作れます。

## Biquadベースで実装

Butterworth 2nd-order biquad を基本にします。

```rust
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1_l: f32,
    z2_l: f32,
    z1_r: f32,
    z2_r: f32,
}
```

Direct Form I よりも、実装は Transposed Direct Form II を推奨します。

## Band params

```rust
pub struct ButterworthBand {
    pub enabled: bool,
    pub kind: ButterworthKind,
    pub frequency_hz: f32,
    pub q: f32,
}

pub enum ButterworthKind {
    LowPass,
    BandPass,
    HighPass,
}
```

Butterworth Qの初期値:

```text
Q = 1 / sqrt(2) = 0.70710678
```

## Frequency ranges

```text
Low frequency  : 20 Hz - 2000 Hz
Mid frequency  : 80 Hz - 8000 Hz
High frequency : 1000 Hz - 20000 Hz
```

ただし内部では sample_rate に応じて Nyquist 以下に clamp します。

```rust
freq = freq.clamp(20.0, sample_rate * 0.45);
```

## Parameter smoothing

周波数移動ができるようにするため、frequencyには smoothing をかけます。

```rust
pub struct SmoothedParam {
    current: f32,
    target: f32,
    coeff: f32,
}
```

第一弾では blockごとに係数更新でもよいですが、filter coefficientが変わるときのzipper noiseを避けるため、以下を推奨します。

- frequency targetは即時変更
- smoothed frequencyをsampleごと、または8sampleごとに更新
- biquad係数は8〜16sampleごとに再計算

## EQ signal chain

```text
input
  -> low band
  -> mid band
  -> high band
  -> output
```

第一弾では直列でよいです。

## Acceptance Criteria

- band on/offがクリック少なく切り替わる
- 周波数を動かしても大きなzipper noiseが出ない
- LP/BP/HPの基本特性がテストで確認できる
- 48kHz/96kHzで安定
- NaN/Infが出ない
