//! A tiny deterministic PRNG (SplitMix64) used only for tie-breaking.
//!
//! The simulator is otherwise fully deterministic; the seed makes genuine ties
//! (equal priorities, equal placement objectives) resolve reproducibly. Using a
//! hand-rolled, version-stable generator keeps `--seed` reproducible across
//! platforms and toolchains with no external dependency.

/// SplitMix64 state.
#[derive(Debug, Clone)]
pub struct SplitMix64(u64);

impl SplitMix64 {
    /// Create a generator from a seed.
    pub fn new(seed: u64) -> Self {
        SplitMix64(seed)
    }

    /// Next 64-bit output.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A stable tie-break key for an item identified by `id`, independent of
    /// iteration order. Same `(seed, id)` always yields the same key.
    pub fn key_for(seed: u64, id: u64) -> u64 {
        // Mix the seed and id, then run one SplitMix64 step for avalanche.
        let mut g = SplitMix64(seed ^ id.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        g.next_u64()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_for_same_seed() {
        let mut a = SplitMix64::new(42);
        let mut b = SplitMix64::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn key_is_stable_and_seed_sensitive() {
        assert_eq!(SplitMix64::key_for(1, 7), SplitMix64::key_for(1, 7));
        assert_ne!(SplitMix64::key_for(1, 7), SplitMix64::key_for(2, 7));
    }
}
