# 02. Core Architecture

## 基本単位

第一弾では完全Node Graphではなく、以下の抽象を用意します。

```rust
pub trait Generator {
    fn prepare(&mut self, sample_rate: f32, max_block_size: usize);
    fn reset(&mut self);
    fn process_mono(&mut self, ctx: &ProcessContext, out: &mut [f32]);
}

pub trait Modulator {
    fn prepare(&mut self, sample_rate: f32, max_block_size: usize);
    fn reset(&mut self);
    fn process_control(&mut self, ctx: &ProcessContext, out: &mut [f32]);
}

pub trait Effect {
    fn prepare(&mut self, sample_rate: f32, max_block_size: usize);
    fn reset(&mut self);
    fn process_stereo(&mut self, ctx: &ProcessContext, left: &mut [f32], right: &mut [f32]);
}
```

## ProcessContext

```rust
pub struct ProcessContext<'a> {
    pub sample_rate: f32,
    pub block_size: usize,
    pub tempo_bpm: f32,
    pub events: &'a [TimedEvent],
}
```

## TimedEvent

```rust
pub struct TimedEvent {
    pub sample_offset: usize,
    pub kind: EventKind,
}

pub enum EventKind {
    NoteOn { note: u8, velocity: f32 },
    NoteOff { note: u8, velocity: f32 },
    Param { id: ParamId, value: f32 },
}
```

## AudioBuffer

第一弾では複雑なバス管理は避け、次のユーティリティを持つだけでよいです。

```rust
pub struct AudioBuffer {
    channels: Vec<Vec<f32>>,
    block_size: usize,
}
```

ただし audio thread で `Vec` が増えないよう、`prepare()` 時に確保します。

## Signal Rate

将来のために型として定義しておきます。

```rust
pub enum SignalRate {
    Audio,
    Control,
    Event,
}
```

第一弾では以下で運用します。

- Generator output: audio-rate
- Envelope: control-rate or per-sample simple generation
- LFO: control-rate or per-sample simple generation
- Parameters: block先頭 + smoothing

## MVP Signal Chain

```text
Note Events
  -> VoiceManager
    -> Voice
      -> Generator
      -> Amp Envelope
      -> LFO target application
      -> pan/gain
  -> Sum Voices
  -> 3-band Butterworth EQ
  -> Output
```

第一弾では、Generatorは1 voiceにつき1つだけです。
