//! A fast, non-cryptographic hasher for the checker's internal tables.
//!
//! The standard library's default hasher (SipHash 1-3) resists hash
//! flooding, which the checker's memo tables don't need: their keys are
//! small ids and ranges produced by the checker itself, not attacker-chosen
//! strings. Profiling the semantic benchmarks (`docs/compiler/performance.md`)
//! showed SipHash at about 10% of a check; this is the multiply-rotate hash
//! of rustc and Firefox ("Fx"), eight bytes per step, the same function
//! `k_syntax`'s tree sink uses for its token cache.
//!
//! Iteration order of an [`FxHashMap`] is deterministic (no random seed),
//! but nothing may depend on it: results are sorted before they leave the
//! crate (sema.md §3.5).

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};

/// The Fx hasher state.
#[derive(Default, Clone, Copy)]
pub(crate) struct FxHasher(u64);

/// The multiplier of the Fx hash (the one rustc uses on 64-bit targets).
const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

impl FxHasher {
    /// Mixes one word into the state: rotate, xor, multiply.
    #[inline]
    fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(SEED);
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let mut chunks = bytes.chunks_exact(8);
        for chunk in &mut chunks {
            let mut word = [0; 8];
            word.copy_from_slice(chunk);
            self.add(u64::from_le_bytes(word));
        }
        let rest = chunks.remainder();
        if !rest.is_empty() {
            let mut word = [0; 8];
            word[..rest.len()].copy_from_slice(rest);
            self.add(u64::from_le_bytes(word));
        }
    }

    #[inline]
    fn write_u8(&mut self, n: u8) {
        self.add(u64::from(n));
    }

    #[inline]
    fn write_u16(&mut self, n: u16) {
        self.add(u64::from(n));
    }

    #[inline]
    fn write_u32(&mut self, n: u32) {
        self.add(u64::from(n));
    }

    #[inline]
    fn write_u64(&mut self, n: u64) {
        self.add(n);
    }

    #[inline]
    fn write_usize(&mut self, n: usize) {
        self.add(n as u64);
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
}

/// A `HashMap` with the [`FxHasher`].
pub(crate) type FxHashMap<K, V> = HashMap<K, V, BuildHasherDefault<FxHasher>>;

/// A `HashSet` with the [`FxHasher`].
pub(crate) type FxHashSet<K> = HashSet<K, BuildHasherDefault<FxHasher>>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::hash::{BuildHasher, Hash};

    fn hash<T: Hash>(value: &T) -> u64 {
        BuildHasherDefault::<FxHasher>::default().hash_one(value)
    }

    #[test]
    fn equal_values_hash_equally_and_distinct_values_differ() {
        assert_eq!(hash(&(1u32, 2u32)), hash(&(1u32, 2u32)));
        assert_ne!(hash(&(1u32, 2u32)), hash(&(2u32, 1u32)));
        assert_ne!(hash(&"abc"), hash(&"abd"));
        // Lengths that aren't a multiple of eight use the remainder.
        assert_ne!(hash(&"abcdefghi"), hash(&"abcdefghj"));
    }

    #[test]
    fn maps_and_sets_work() {
        let mut map: FxHashMap<u32, &str> = FxHashMap::default();
        map.insert(7, "seven");
        assert_eq!(map.get(&7), Some(&"seven"));
        let set: FxHashSet<String> = ["a".to_owned()].into_iter().collect();
        assert!(set.contains("a"));
    }
}
