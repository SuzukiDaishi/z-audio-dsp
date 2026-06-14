# 05. Modulators Plan

## 第一弾 Modulators

- Envelope
- LFO

第一弾では arbitrary modulation graph は作りません。  
単一Generatorに対して、Amp Envelope と LFO を固定ルーティングで適用します。

## Envelope

### Type

ADSR を基本にします。

```rust
pub enum EnvelopeState {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}
```

```rust
pub struct Envelope {
    params: EnvelopeParams,
    state: EnvelopeState,
    value: f32,
    release_start_value: f32,
}
```

### Params

```rust
pub struct EnvelopeParams {
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    pub curve: EnvelopeCurve,
}
```

### Behavior

- NoteOn: Attackへ
- Attack完了: Decayへ
- Decay完了: Sustainへ
- NoteOff: Releaseへ
- Release完了: Idleへ

### Curve

第一弾では `Linear` と `Exponential` を用意します。  
音楽的には Exponential をdefaultにします。

## LFO

```rust
pub struct Lfo {
    sample_rate: f32,
    phase: f32,
    params: LfoParams,
    random_value: f32,
}
```

### Waveforms

- Sine
- Triangle
- SawUp
- SawDown
- Square
- RandomHold

### Targets

第一弾では固定ターゲットのみ。

```rust
pub enum LfoTarget {
    None,
    Gain,
    PitchSemitone,
    EqLowFreq,
    EqMidFreq,
    EqHighFreq,
}
```

### LFO value range

LFOの出力は基本 `[-1.0, 1.0]`。

ターゲットごとに解釈します。

```text
Gain          : gain *= 1.0 + lfo * amount
PitchSemitone : pitch += lfo * amount
EqFreq        : frequency *= 2^(lfo * amount_oct / 1.0)
```

## 第一弾の制約

やらないこと:

- Env -> Env
- LFO -> Env
- LFO -> LFO
- Modulatorの任意パラメータ制御
- audio-rate LFO
- sample-accurate automation

将来的には ModMatrix を導入します。

## Acceptance Criteria

- ADSRがクリックなく動作する
- Release中に再NoteOnしても大きな段差が出ない
- LFOが指定ターゲットに反映される
- LFO retrigger true/false が動作する
- EnvelopeとLFOの組み合わせで簡単な音色変化が作れる
