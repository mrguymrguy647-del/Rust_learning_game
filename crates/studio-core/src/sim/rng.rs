//! A tiny serializable RNG (SplitMix64). Its state is saved with the game so that
//! save/load never changes what happens next, and simulations are reproducible in tests.

use std::convert::Infallible;

use rand::{RngExt, TryRng};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameRng {
    state: u64,
}

impl GameRng {
    pub fn from_seed(seed: u64) -> GameRng {
        GameRng { state: seed ^ 0x9E37_79B9_7F4A_7C15 }
    }

    /// Seeded from OS entropy (new games).
    pub fn from_entropy() -> GameRng {
        GameRng::from_seed(rand::random::<u64>())
    }

    fn step(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform float in `[0, 1)`.
    pub fn unit(&mut self) -> f32 {
        (self.step() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Uniform float in `[lo, hi)`.
    pub fn range_f32(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }

    /// Uniform integer in `[lo, hi]` (inclusive). Returns `lo` if the range is empty.
    pub fn range_i64(&mut self, lo: i64, hi: i64) -> i64 {
        if hi <= lo {
            return lo;
        }
        self.random_range(lo..=hi)
    }

    pub fn range_usize(&mut self, lo: usize, hi: usize) -> usize {
        if hi <= lo {
            return lo;
        }
        self.random_range(lo..=hi)
    }

    /// True with probability `p` (clamped to `[0, 1]`).
    pub fn chance(&mut self, p: f32) -> bool {
        self.unit() < p.clamp(0.0, 1.0)
    }

    /// Approximately normal noise with mean 0 and standard deviation ~`sd`.
    pub fn noise(&mut self, sd: f32) -> f32 {
        // Sum of four uniforms: mean 2, variance 1/3.
        let s: f32 = (0..4).map(|_| self.unit()).sum();
        (s - 2.0) * sd * 1.732
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            None
        } else {
            items.get(self.range_usize(0, items.len() - 1))
        }
    }

    /// Index chosen proportionally to `weights` (negative weights count as zero).
    pub fn weighted_index(&mut self, weights: &[f32]) -> Option<usize> {
        let total: f32 = weights.iter().map(|w| w.max(0.0)).sum();
        if total <= 0.0 {
            return None;
        }
        let mut roll = self.unit() * total;
        for (i, w) in weights.iter().enumerate() {
            roll -= w.max(0.0);
            if roll < 0.0 {
                return Some(i);
            }
        }
        Some(weights.len() - 1)
    }
}

impl TryRng for GameRng {
    type Error = Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Infallible> {
        Ok((self.step() >> 32) as u32)
    }

    fn try_next_u64(&mut self) -> Result<u64, Infallible> {
        Ok(self.step())
    }

    fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), Infallible> {
        for chunk in dst.chunks_mut(8) {
            let bytes = self.step().to_le_bytes();
            chunk.copy_from_slice(&bytes[..chunk.len()]);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = GameRng::from_seed(42);
        let mut b = GameRng::from_seed(42);
        for _ in 0..100 {
            assert_eq!(a.range_i64(0, 1000), b.range_i64(0, 1000));
        }
    }

    #[test]
    fn serde_round_trip_preserves_stream() {
        let mut a = GameRng::from_seed(7);
        a.unit();
        let json = serde_json::to_string(&a).unwrap();
        let mut b: GameRng = serde_json::from_str(&json).unwrap();
        for _ in 0..20 {
            assert_eq!(a.unit(), b.unit());
        }
    }

    #[test]
    fn unit_is_in_range_and_weighted_index_respects_zero_weights() {
        let mut r = GameRng::from_seed(1);
        for _ in 0..1000 {
            let u = r.unit();
            assert!((0.0..1.0).contains(&u));
        }
        for _ in 0..200 {
            assert_eq!(r.weighted_index(&[0.0, 0.0, 3.0]), Some(2));
        }
        assert_eq!(r.weighted_index(&[0.0]), None);
        assert_eq!(r.pick::<u8>(&[]), None);
    }
}
