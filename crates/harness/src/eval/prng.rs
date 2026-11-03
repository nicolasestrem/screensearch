//! Deterministic seeded sampling for the P4 eval runner (`docs/kickoffs/P4.md`).
//!
//! The workspace has no `rand` dependency and the runner must be reproducible from
//! `(db, seed)` alone, so this is a small SplitMix64 (Steele/Lea/Flood) plus the two sampling
//! primitives the generators use. Nothing here touches the database or the clock.

/// SplitMix64: 64-bit state, one multiply-xor-shift round per output.
#[derive(Debug, Clone)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform-enough index in `0..n`. `n == 0` returns 0 (callers never pass 0).
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        (self.next_u64() % n as u64) as usize
    }

    /// In-place Fisher-Yates shuffle.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i + 1);
            items.swap(i, j);
        }
    }
}

/// A generator-specific stream: `seed` mixed with a stable tag, so two generators never walk the
/// same random sequence (and adding one generator cannot reshuffle another's sample).
pub fn subseed(seed: u64, tag: &str) -> u64 {
    // FNV-1a over the tag, then mixed with the run seed.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in tag.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    seed ^ hash
}

/// Deterministically choose `k` items out of `items`, preserving the source order of the chosen
/// items (the generators order their candidates newest-first / by `started_at`, and re-sorting the
/// sample keeps that meaning). When `items.len() <= k` the whole slice is returned unchanged.
pub fn sample<T: Clone>(rng: &mut SplitMix64, items: &[T], k: usize) -> Vec<T> {
    if k == 0 || items.is_empty() {
        return Vec::new();
    }
    if items.len() <= k {
        return items.to_vec();
    }
    let mut idx: Vec<usize> = (0..items.len()).collect();
    rng.shuffle(&mut idx);
    idx.truncate(k);
    idx.sort_unstable();
    idx.into_iter().map(|i| items[i].clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence_and_different_seeds_differ() {
        let a: Vec<u64> = (0..8).map(|_| SplitMix64::new(7).next_u64()).collect();
        assert!(
            a.windows(2).all(|w| w[0] == w[1]),
            "uniform seed -> uniform first draw"
        );
        let seq = |seed| {
            let mut r = SplitMix64::new(seed);
            (0..16).map(|_| r.next_u64()).collect::<Vec<_>>()
        };
        assert_eq!(seq(1), seq(1));
        assert_ne!(seq(1), seq(2));
    }

    #[test]
    fn sample_is_a_subset_in_source_order() {
        let items: Vec<u32> = (0..100).collect();
        let mut r1 = SplitMix64::new(42);
        let picked = sample(&mut r1, &items, 10);
        assert_eq!(picked.len(), 10);
        assert!(picked.windows(2).all(|w| w[0] < w[1]), "source order kept");
        assert!(picked.iter().all(|p| items.contains(p)));

        let mut r2 = SplitMix64::new(42);
        assert_eq!(sample(&mut r2, &items, 10), picked);
        let mut r3 = SplitMix64::new(43);
        assert_ne!(sample(&mut r3, &items, 10), picked);
    }

    #[test]
    fn sample_past_the_end_returns_everything() {
        let items = vec![1, 2, 3];
        let mut r = SplitMix64::new(1);
        assert_eq!(sample(&mut r, &items, 10), items);
    }

    #[test]
    fn subseeds_are_distinct_per_generator() {
        let seed = 1234;
        let a = subseed(seed, "last_seen");
        let b = subseed(seed, "involvement");
        assert_ne!(a, b);
        assert_eq!(a, subseed(seed, "last_seen"));
    }
}
