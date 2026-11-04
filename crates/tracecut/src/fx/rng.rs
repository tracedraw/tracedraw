//! A small pseudo-random generator for the random features (puzzle tabs,
//! circle fill, random copies): the same seed gives the same result on
//! every machine and every build.
//!
//! The generator is SplitMix64: a 64-bit counter stepped by a fixed odd
//! constant and passed through a mixing function. It is not meant for
//! anything but repeatable layouts.

/// A seeded pseudo-random generator.
#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng { state: seed }
    }

    /// The next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number from 0 (included) to 1 (excluded).
    pub fn next_f64(&mut self) -> f64 {
        // The top 53 bits make an evenly spread double.
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// A number between `a` and `b` (either order); `a` when the range is
    /// empty or not finite.
    pub fn range(&mut self, a: f64, b: f64) -> f64 {
        let u = self.next_f64();
        if !(a.is_finite() && b.is_finite()) {
            return if a.is_finite() { a } else { 0.0 };
        }
        a + (b - a) * u
    }

    /// A number from -1 to 1.
    pub fn signed(&mut self) -> f64 {
        self.next_f64() * 2.0 - 1.0
    }

    /// True half of the time.
    pub fn coin(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_repeats_the_same_numbers() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        let mut c = Rng::new(43);
        let mut a = Rng::new(42);
        assert_ne!(a.next_u64(), c.next_u64());
    }

    #[test]
    fn numbers_stay_in_their_range_and_spread() {
        let mut r = Rng::new(7);
        let mut sum = 0.0;
        for _ in 0..10_000 {
            let v = r.next_f64();
            assert!((0.0..1.0).contains(&v));
            sum += v;
            let w = r.range(5.0, 2.0);
            assert!((2.0..=5.0).contains(&w));
        }
        let mean = sum / 10_000.0;
        assert!((mean - 0.5).abs() < 0.02, "{mean}");
        assert_eq!(r.range(f64::NAN, 3.0), 0.0);
        assert_eq!(r.range(1.0, f64::INFINITY), 1.0);
    }
}
