//! Minimal deterministic PRNG (xoshiro256** variant seeded from a u64).
//!
//! No external rand crate dependency — keeps the crate footprint small and
//! avoids version-pinning issues across workspace members.

/// xoshiro256** PRNG state.
pub struct Rng {
    s: [u64; 4],
}

impl Rng {
    /// Seed from a single u64 using splitmix64.
    pub fn new(seed: u64) -> Self {
        let mut x = seed;
        let mut s = [0u64; 4];
        for slot in &mut s {
            x = x.wrapping_add(0x9e3779b97f4a7c15);
            let mut z = x;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
            *slot = z ^ (z >> 31);
        }
        Self { s }
    }

    /// Derive a child RNG for a named sub-stream (domain separation).
    pub fn child(&self, tag: u64) -> Self {
        let mut child = Self { s: self.s };
        child.s[0] ^= tag;
        child.s[1] ^= tag.rotate_left(17);
        // Warm up so the initial state isn't correlated with the parent.
        for _ in 0..8 {
            child.next_u64();
        }
        child
    }

    /// xoshiro256** step.
    pub fn next_u64(&mut self) -> u64 {
        let result = self.s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    /// Uniform float in [0, 1).
    pub fn next_f64(&mut self) -> f64 {
        let bits = self.next_u64() >> 11;
        bits as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Uniform integer in [0, n).
    pub fn next_usize(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        (self.next_u64() % n as u64) as usize
    }

    /// Bernoulli trial: true with probability p.
    pub fn bernoulli(&mut self, p: f64) -> bool {
        self.next_f64() < p
    }

    /// Sample from a lognormal distribution with given mean (ms) via
    /// Box-Muller transform. Returns a positive f64.
    pub fn lognormal(&mut self, mean_ms: f64, sigma: f64) -> f64 {
        // Box-Muller: two uniforms → one normal.
        let u1 = self.next_f64().max(1e-15);
        let u2 = self.next_f64();
        let normal = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
        // For lognormal(mu, sigma): mu = ln(mean) - sigma^2/2
        let mu = mean_ms.ln() - sigma * sigma / 2.0;
        (mu + sigma * normal).exp()
    }

    /// Weighted pick from a slice: weights are unnormalized non-negative f64.
    pub fn weighted_pick<'a, T>(&mut self, choices: &'a [(T, f64)]) -> &'a T {
        let total: f64 = choices.iter().map(|(_, w)| w).sum();
        let mut r = self.next_f64() * total;
        for (item, w) in choices {
            r -= w;
            if r <= 0.0 {
                return item;
            }
        }
        &choices.last().unwrap().0
    }
}
