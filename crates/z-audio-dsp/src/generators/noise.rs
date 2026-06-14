//! White noise generator.

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

use crate::Generator;
use crate::context::ProcessContext;

/// A white-noise generator, uniformly distributed in `[-1.0, 1.0]`.
///
/// Phase 1 supports white noise only; pink/brown/blue/violet/velvet and
/// sample-and-hold variants are planned for later phases.
pub struct NoiseGenerator {
    rng: SmallRng,
}

impl NoiseGenerator {
    /// Creates a new white noise generator seeded with `seed`.
    pub fn new(seed: u64) -> Self {
        Self {
            rng: SmallRng::seed_from_u64(seed),
        }
    }

    /// Produces the next sample, uniformly distributed in `[-1.0, 1.0]`.
    pub fn next_sample(&mut self) -> f32 {
        self.rng.gen_range(-1.0..=1.0)
    }
}

impl Generator for NoiseGenerator {
    fn prepare(&mut self, _sample_rate: f32, _max_block_size: usize) {}

    /// No-op: noise has no phase to reset, and re-seeding would make
    /// `reset()` calls audibly discontinuous in a way Phase 1 doesn't need.
    fn reset(&mut self) {}

    fn process_mono(&mut self, _ctx: &ProcessContext, out: &mut [f32]) {
        for sample in out.iter_mut() {
            *sample = self.next_sample();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_is_finite_and_bounded() {
        let mut noise = NoiseGenerator::new(42);
        for _ in 0..48_000 {
            let s = noise.next_sample();
            assert!(s.is_finite());
            assert!((-1.0..=1.0).contains(&s));
        }
    }
}
