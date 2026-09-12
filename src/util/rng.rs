//! Deterministic pseudorandom bytes.
//!
//! Every byte the harness generates comes from here, so a run with the same
//! `--seed` produces the same corpus on any machine, CPU and operating
//! system. The stream is the BLAKE3 extendable output of a key derived from
//! the seed and a caller-supplied label: BLAKE3's XOF is specified
//! byte-for-byte, has no platform-dependent behaviour, and two different
//! labels give independent streams, so generators can be added or reordered
//! without disturbing the bytes of the existing ones.

/// An endless deterministic byte stream identified by `(seed, label)`.
pub struct Stream {
    reader: blake3::OutputReader,
}

impl Stream {
    /// Opens the stream for `label` under `seed`.
    pub fn new(seed: u64, label: &str) -> Stream {
        let mut hasher = blake3::Hasher::new_derive_key("amber-cas-bench 2026 corpus");
        hasher.update(&seed.to_le_bytes());
        hasher.update(label.as_bytes());
        Stream {
            reader: hasher.finalize_xof(),
        }
    }

    /// Fills `buf` with the next bytes of the stream.
    pub fn fill(&mut self, buf: &mut [u8]) {
        self.reader.fill(buf);
    }

    /// Returns the next `n` bytes.
    pub fn bytes(&mut self, n: usize) -> Vec<u8> {
        let mut v = vec![0u8; n];
        self.fill(&mut v);
        v
    }
}

/// A deterministic integer generator drawing from a [`Stream`].
pub struct Rng {
    stream: Stream,
}

impl Rng {
    /// Opens the generator for `label` under `seed`.
    pub fn new(seed: u64, label: &str) -> Rng {
        Rng {
            stream: Stream::new(seed, label),
        }
    }

    /// The next 64 raw bits.
    pub fn next_u64(&mut self) -> u64 {
        let mut b = [0u8; 8];
        self.stream.fill(&mut b);
        u64::from_le_bytes(b)
    }

    /// A value in `0..n`. Uses Lemire's multiply-shift reduction, which is
    /// very slightly biased (at most 2^-64 relative) but fully specified and
    /// branch-free, so the sequence never depends on rejection timing.
    pub fn below(&mut self, n: u64) -> u64 {
        assert!(n > 0, "below(0)");
        ((self.next_u64() as u128 * n as u128) >> 64) as u64
    }

    /// A value in `lo..=hi`.
    pub fn range(&mut self, lo: u64, hi: u64) -> u64 {
        assert!(lo <= hi, "range({lo}, {hi})");
        lo + self.below(hi - lo + 1)
    }

    /// True with probability `num/den`.
    pub fn chance(&mut self, num: u64, den: u64) -> bool {
        self.below(den) < num
    }

    /// Shuffles `items` in place (Fisher-Yates, descending).
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i as u64 + 1) as usize;
            items.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_is_a_function_of_seed_and_label() {
        let a = Stream::new(7, "alpha").bytes(64);
        let b = Stream::new(7, "alpha").bytes(64);
        assert_eq!(a, b, "same seed and label must reproduce the same bytes");

        assert_ne!(a, Stream::new(8, "alpha").bytes(64));
        assert_ne!(a, Stream::new(7, "beta").bytes(64));
    }

    #[test]
    fn stream_is_position_independent() {
        // Reading in two pieces must equal reading in one.
        let whole = Stream::new(3, "x").bytes(100);
        let mut s = Stream::new(3, "x");
        let mut head = s.bytes(37);
        head.extend(s.bytes(63));
        assert_eq!(whole, head);
    }

    #[test]
    fn below_stays_in_range_and_is_reproducible() {
        let mut a = Rng::new(1, "l");
        let mut b = Rng::new(1, "l");
        for _ in 0..1000 {
            let v = a.below(10);
            assert!(v < 10);
            assert_eq!(v, b.below(10));
        }
    }

    #[test]
    fn shuffle_is_a_permutation_and_reproducible() {
        let mut v: Vec<u32> = (0..50).collect();
        let mut w = v.clone();
        Rng::new(42, "s").shuffle(&mut v);
        Rng::new(42, "s").shuffle(&mut w);
        assert_eq!(v, w);
        v.sort_unstable();
        assert_eq!(v, (0..50).collect::<Vec<_>>());
    }
}
