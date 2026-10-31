//! Content identity of a level file: what a replay pins a level switch to (ADR-0032).

use std::fmt;

use sha2::{Digest, Sha256};

/// SHA-256 over every source a level load reads, so a copy of the file elsewhere hashes the same.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct LevelHash([u8; 32]);

impl LevelHash {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Hash `sources` in order, each preceded by its length as 8 bytes little-endian.
    pub(crate) fn of_sources<'a>(sources: impl IntoIterator<Item = &'a [u8]>) -> Self {
        let mut digest = Sha256::new();
        for source in sources {
            digest.update((source.len() as u64).to_le_bytes());
            digest.update(source);
        }
        Self(digest.finalize().into())
    }
}

impl fmt::Display for LevelHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash_of(sources: &[&[u8]]) -> LevelHash {
        LevelHash::of_sources(sources.iter().copied())
    }

    #[test]
    fn one_changed_byte_changes_the_hash() {
        let before = hash_of(&[b"main", b"buffer"]);

        let in_first = hash_of(&[b"maiN", b"buffer"]);
        let in_second = hash_of(&[b"main", b"buffes"]);

        assert_ne!(before, in_first);
        assert_ne!(before, in_second);
        assert_ne!(in_first, in_second);
    }

    #[test]
    fn moving_a_byte_between_sources_changes_the_hash() {
        let first = hash_of(&[b"ab", b"c"]);

        let second = hash_of(&[b"a", b"bc"]);

        assert_ne!(first, second);
    }

    #[test]
    fn known_two_source_vector_hashes_to_the_pinned_digest() {
        let hash = hash_of(&[b"abc", b"de"]);

        assert_eq!(
            hash.to_string(),
            "bb20916c560837a0d3d4bda7c626818e12e8be20944518c13f5cdf9dad73bed4"
        );
    }

    #[test]
    fn display_is_lowercase_hex() {
        let mut bytes = [0u8; 32];
        bytes[0] = 0xAB;
        bytes[1] = 0x01;
        bytes[31] = 0xFF;

        let shown = LevelHash::from_bytes(bytes).to_string();

        assert_eq!(
            shown,
            format!("ab01{}ff", "00".repeat(29)),
            "64 lowercase hex digits, two per byte"
        );
    }
}
