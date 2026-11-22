//! Fixed xorshift64* stream. Pure, seed-only, platform independent.

/// Deterministic pseudo-random stream used by generation.
pub struct Rng {
    state: u64,
}

impl Rng {
    /// A zero seed would lock xorshift at zero; substitute a fixed constant.
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0x9E37_79B9_7F4A_7C15 } else { seed },
        }
    }

    /// Advance the stream and return the next 64-bit value.
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform-ish value in `0..n`. Modulo bias is irrelevant for fixtures and
    /// documenting it is preferable to pretending the stream is cryptographic.
    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next_u64() % n }
    }

    /// Inclusive integer range.
    pub fn range_i64(&mut self, min: i64, max: i64) -> i64 {
        if min >= max {
            return min;
        }
        min.wrapping_add((self.below((max - min + 1) as u64)) as i64)
    }

    /// Pick one element; the caller guarantees the slice is non-empty.
    pub fn pick<'a>(&mut self, items: &'a [String]) -> &'a str {
        let index = self.below(items.len() as u64) as usize;
        items[index].as_str()
    }

    /// One unbiased coin toss from the low bit.
    pub fn boolean(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }
}
