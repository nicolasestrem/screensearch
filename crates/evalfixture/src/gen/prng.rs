//! Deterministic sampling for the fixture generator.
//!
//! The workspace has no `rand` dependency and the fixture must be byte-identical
//! across runs, so every sampling decision routes through this small SplitMix64
//! (Steele/Lea/Flood), the same construction the eval harness uses
//! (`crates/harness/src/eval/prng.rs`). Per-purpose streams come from
//! [`subseed`], so adding one stream can never reshuffle another's draws.

/// SplitMix64: 64-bit state, one multiply-xor-shift round per output.
#[derive(Debug, Clone)]
pub(crate) struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub(crate) fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform-enough index in `0..n`. `n == 0` returns 0 (callers never pass 0).
    pub(crate) fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        (self.next_u64() % n as u64) as usize
    }

    /// In-place Fisher-Yates shuffle.
    pub(crate) fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i + 1);
            items.swap(i, j);
        }
    }

    /// True with probability `num / den` (`num <= den`, both nonzero).
    pub(crate) fn chance(&mut self, num: u64, den: u64) -> bool {
        debug_assert!(num <= den && den > 0);
        self.next_u64() % den < num
    }
}

/// A per-purpose stream: the seed mixed with a stable tag, so two streams never
/// walk the same sequence (and adding a stream cannot reshuffle another's draws).
pub(crate) fn subseed(seed: u64, tag: &str) -> u64 {
    // FNV-1a over the tag, then mixed with the run seed.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in tag.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    seed ^ hash
}
