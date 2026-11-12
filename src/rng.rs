//! Deterministic, platform-independent seeded randomness.
//!
//! Gameplay never touches thread-local randomness. Every random decision is
//! drawn from a [`GameRng`] built from the run's [`RunSeed`], so a seed always
//! reproduces the same floor layout and entity schedule (Phase 2+).
//!
//! We implement SplitMix64 ourselves instead of using `rand::StdRng` because
//! `StdRng`'s algorithm is explicitly allowed to change between `rand`
//! releases, which would silently break shared seeds.

use bevy::prelude::*;

/// The seed for the current (or next) run. Shown in the UI so runs can be shared.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunSeed(pub u64);

impl Default for RunSeed {
    fn default() -> Self {
        Self(0x4841_4C4C_5741_5953) // "HALLWAYS"
    }
}

impl RunSeed {
    /// Parses a user-entered seed. Pure numbers are used directly; any other
    /// text is hashed (FNV-1a) so players can share words like `"basement"`.
    pub fn from_text(text: &str) -> Self {
        let trimmed = text.trim();
        if let Ok(n) = trimmed.parse::<u64>() {
            return Self(n);
        }
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in trimmed.to_lowercase().bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
        }
        Self(hash)
    }

    /// Human-friendly representation for the HUD / summary screens.
    pub fn display(&self) -> String {
        format!("{:016X}", self.0)
    }
}

/// SplitMix64 generator: tiny, fast, and identical on every platform.
#[derive(Debug, Clone)]
pub struct GameRng {
    state: u64,
}

impl GameRng {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Derive an independent stream (e.g. one per room) from a parent seed.
    pub fn fork(seed: u64, stream: u64) -> Self {
        let mut rng = Self::new(seed ^ stream.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        rng.next_u64();
        rng
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform float in `[0, 1)`.
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Uniform float in `[lo, hi)`.
    pub fn range_f32(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next_f32()
    }

    /// Uniform integer in `[0, n)`. Returns 0 when `n == 0`.
    pub fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        ((self.next_u64() >> 32) * u64::from(n) >> 32) as u32
    }

    /// Picks an index according to non-negative weights. `None` if all weights are zero.
    pub fn weighted_index(&mut self, weights: &[f32]) -> Option<usize> {
        let total: f32 = weights.iter().copied().filter(|w| *w > 0.0).sum();
        if total <= 0.0 {
            return None;
        }
        let mut roll = self.next_f32() * total;
        for (i, w) in weights.iter().enumerate() {
            if *w <= 0.0 {
                continue;
            }
            if roll < *w {
                return Some(i);
            }
            roll -= *w;
        }
        weights.iter().rposition(|w| *w > 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = GameRng::new(42);
        let mut b = GameRng::new(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn known_first_values_are_stable() {
        // Guards against accidental algorithm changes that would break shared seeds.
        let mut rng = GameRng::new(0);
        assert_eq!(rng.next_u64(), 0xE220_A839_7B1D_CDAF);
        assert_eq!(rng.next_u64(), 0x6E78_9E6A_A1B9_65F4);
    }

    #[test]
    fn different_streams_diverge() {
        let mut a = GameRng::fork(7, 1);
        let mut b = GameRng::fork(7, 2);
        let same = (0..32).filter(|_| a.next_u64() == b.next_u64()).count();
        assert_eq!(same, 0);
    }

    #[test]
    fn floats_and_ranges_in_bounds() {
        let mut rng = GameRng::new(99);
        for _ in 0..10_000 {
            let f = rng.next_f32();
            assert!((0.0..1.0).contains(&f));
            let r = rng.range_f32(-2.0, 3.0);
            assert!((-2.0..3.0).contains(&r));
            assert!(rng.below(7) < 7);
        }
        assert_eq!(rng.below(0), 0);
    }

    #[test]
    fn weighted_index_respects_zero_weights() {
        let mut rng = GameRng::new(5);
        for _ in 0..1000 {
            let i = rng.weighted_index(&[0.0, 1.0, 0.0, 3.0]).unwrap();
            assert!(i == 1 || i == 3);
        }
        assert_eq!(rng.weighted_index(&[0.0, 0.0]), None);
        assert_eq!(rng.weighted_index(&[]), None);
    }

    #[test]
    fn text_seeds() {
        assert_eq!(RunSeed::from_text("1234"), RunSeed(1234));
        assert_eq!(RunSeed::from_text(" Basement "), RunSeed::from_text("basement"));
        assert_ne!(RunSeed::from_text("basement"), RunSeed::from_text("attic"));
    }
}
