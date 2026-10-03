//! The hash maps the compiler stages use for their own tables (`FxHashMap`,
//! `FxHashSet`): FxHash, the hash rustc (`rustc_data_structures::fx`) and Firefox use,
//! here as the `rustc-hash` crate's first algorithm.
//!
//! std's default, SipHash, resists hash flooding from a hostile input and seeds itself
//! randomly in each process. Neither matters for the names in a source file: SipHash
//! was a fifth of resolve's instructions (contracts plan step 2 review), and its random
//! seed moved the benchmark suite's resolve counts by up to 0.5% from run to run.

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};

/// A `HashMap` with FxHash. Made with `FxHashMap::default()`.
pub type FxHashMap<K, V> = HashMap<K, V, BuildHasherDefault<FxHasher>>;

/// A `HashSet` with FxHash. Made with `FxHashSet::default()`.
pub type FxHashSet<T> = HashSet<T, BuildHasherDefault<FxHasher>>;

/// FxHash: each word is rotated into the hash, which is then multiplied by a constant.
/// Fast, and the same in every run.
#[derive(Clone, Copy, Default)]
pub struct FxHasher {
    hash: u64,
}

/// The multiplier: odd, with well-mixed bits (from the `rustc-hash` crate).
const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

impl FxHasher {
    fn add(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(SEED);
    }
}

impl Hasher for FxHasher {
    /// Eight bytes at a time, then the rest as four, two and one.
    fn write(&mut self, bytes: &[u8]) {
        let (words, mut rest) = bytes.as_chunks::<8>();
        for word in words {
            self.add(u64::from_le_bytes(*word));
        }
        if let [a, b, c, d, tail @ ..] = rest {
            self.add(u64::from(u32::from_le_bytes([*a, *b, *c, *d])));
            rest = tail;
        }
        if let [a, b, tail @ ..] = rest {
            self.add(u64::from(u16::from_le_bytes([*a, *b])));
            rest = tail;
        }
        if let [a] = rest {
            self.add(u64::from(*a));
        }
    }

    fn write_u8(&mut self, i: u8) {
        self.add(u64::from(i));
    }

    fn write_u16(&mut self, i: u16) {
        self.add(u64::from(i));
    }

    fn write_u32(&mut self, i: u32) {
        self.add(u64::from(i));
    }

    fn write_u64(&mut self, i: u64) {
        self.add(i);
    }

    fn write_usize(&mut self, i: usize) {
        self.add(i as u64);
    }

    fn finish(&self) -> u64 {
        self.hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::hash::BuildHasher;

    fn hash(s: &str) -> u64 {
        BuildHasherDefault::<FxHasher>::default().hash_one(s)
    }

    #[test]
    fn the_same_key_hashes_the_same_in_every_run() {
        assert_eq!(hash("vcc"), hash("vcc"));
        assert_ne!(hash("vcc"), hash("vc"));
        // Every length of tail: 8-byte words, then 4, 2 and 1 bytes.
        let names: Vec<String> = (0..20).map(|n| "x".repeat(n)).collect();
        let hashes: FxHashSet<u64> = names.iter().map(|n| hash(n)).collect();
        assert_eq!(hashes.len(), names.len());
    }

    #[test]
    fn numbered_names_spread_out() {
        let hashes: FxHashSet<u64> = (0..100_000).map(|i| hash(&format!("p{i:05}"))).collect();
        assert_eq!(hashes.len(), 100_000);
        let mut map: FxHashMap<String, usize> = FxHashMap::default();
        map.insert("ambient".to_string(), 1);
        assert_eq!(map.get("ambient"), Some(&1));
    }
}
