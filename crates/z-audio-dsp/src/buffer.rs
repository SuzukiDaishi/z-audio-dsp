//! Pre-allocated multi-channel audio buffer.

/// A fixed-size, multi-channel audio buffer. All channel storage is
/// allocated up front (e.g. in `prepare()`); [`AudioBuffer::clear`] and the
/// channel accessors never allocate, so it is safe to use from the audio
/// thread.
pub struct AudioBuffer {
    channels: Vec<Vec<f32>>,
    block_size: usize,
}

impl AudioBuffer {
    /// Creates a new buffer with `num_channels` channels, each holding
    /// `block_size` samples initialized to `0.0`.
    pub fn new(num_channels: usize, block_size: usize) -> Self {
        Self {
            channels: (0..num_channels).map(|_| vec![0.0; block_size]).collect(),
            block_size,
        }
    }

    /// Returns the number of channels.
    pub fn num_channels(&self) -> usize {
        self.channels.len()
    }

    /// Returns the number of samples per channel.
    pub fn block_size(&self) -> usize {
        self.block_size
    }

    /// Returns an immutable view of the given channel.
    pub fn channel(&self, index: usize) -> &[f32] {
        &self.channels[index]
    }

    /// Returns a mutable view of the given channel.
    pub fn channel_mut(&mut self, index: usize) -> &mut [f32] {
        &mut self.channels[index]
    }

    /// Sets every sample in every channel to `0.0`.
    pub fn clear(&mut self) {
        for channel in &mut self.channels {
            channel.fill(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_buffer_is_zeroed() {
        let buf = AudioBuffer::new(2, 128);
        assert_eq!(buf.num_channels(), 2);
        assert_eq!(buf.block_size(), 128);
        assert!(buf.channel(0).iter().all(|&s| s == 0.0));
        assert!(buf.channel(1).iter().all(|&s| s == 0.0));
    }

    #[test]
    fn clear_resets_to_zero() {
        let mut buf = AudioBuffer::new(1, 4);
        buf.channel_mut(0).copy_from_slice(&[1.0, 2.0, 3.0, 4.0]);
        buf.clear();
        assert!(buf.channel(0).iter().all(|&s| s == 0.0));
    }
}
