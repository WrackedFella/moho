//! Seeded, platform-independent random numbers for simulation state.
//!
//! A [`SimRng`] is one named stream's SplitMix64 generator, held by value in the
//! owning system's state. The sequence is integer-only and pinned by tests, so the same
//! seed and stream replay the same numbers on every platform and dependency version.
//! Changing the sequence breaks saved games and replays, so treat it as a compatibility break.

use serde::{Deserialize, Serialize};

/// A system's stream key: the 64-bit FNV-1a hash of its name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StreamId(u64);

impl StreamId {
    /// Derives the stream key for `name` with 64-bit FNV-1a.
    pub const fn named(name: &str) -> StreamId {
        const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
        const PRIME: u64 = 0x0000_0100_0000_01b3;

        let bytes = name.as_bytes();
        let mut hash = OFFSET_BASIS;
        let mut i = 0;
        while i < bytes.len() {
            hash ^= bytes[i] as u64;
            hash = hash.wrapping_mul(PRIME);
            i += 1;
        }
        StreamId(hash)
    }
}

/// One stream's SplitMix64 generator.
///
/// Not `Copy`, so a stream cannot be duplicated by accident and replay values twice.
///
/// ```compile_fail
/// use moho_core::rng::{SimRng, StreamId};
///
/// let rng = SimRng::new(1, StreamId::named("x"));
/// let a = rng;
/// let b = rng;
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimRng {
    state: u64,
}

impl SimRng {
    /// Starts `stream` from `seed`; calling it again restarts the sequence.
    pub fn new(seed: u64, stream: StreamId) -> SimRng {
        SimRng {
            state: mix64(seed ^ mix64(stream.0)),
        }
    }

    /// Next 64 uniformly distributed bits.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(GAMMA);
        mix64(self.state)
    }

    /// Next value in `[0, 1)`, the top 24 bits times `2^-24`.
    pub fn next_f32(&mut self) -> f32 {
        unit_f32(self.next_u64())
    }
}

/// Maps the top 24 bits of `bits` to `[0, 1)`.
fn unit_f32(bits: u64) -> f32 {
    (bits >> 40) as f32 * (1.0 / (1u32 << 24) as f32)
}

/// SplitMix64's golden-ratio increment, `2^64 / phi` rounded to odd.
const GAMMA: u64 = 0x9e37_79b9_7f4a_7c15;

/// SplitMix64's output finalizer; a bijection on `u64`.
const fn mix64(x: u64) -> u64 {
    let z = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    let z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn draw_u64(rng: &mut SimRng, n: usize) -> Vec<u64> {
        (0..n).map(|_| rng.next_u64()).collect()
    }

    fn name_strategy() -> impl Strategy<Value = String> {
        "[a-z._]{0,16}"
    }

    proptest! {
        #[test]
        fn same_seed_and_stream_give_equal_sequences(
            seed in any::<u64>(),
            name in name_strategy(),
            count in 0usize..64,
        ) {
            let stream = StreamId::named(&name);
            let mut a = SimRng::new(seed, stream);
            let mut b = SimRng::new(seed, stream);

            let from_a = draw_u64(&mut a, count);
            let from_b = draw_u64(&mut b, count);

            prop_assert_eq!(from_a, from_b);
            prop_assert_eq!(a, b);
        }

        #[test]
        fn extra_draw_on_one_stream_leaves_other_unchanged(
            seed in any::<u64>(),
            name_a in name_strategy(),
            name_b in name_strategy(),
            extra in 1usize..32,
            count in 1usize..32,
        ) {
            let stream_a = StreamId::named(&name_a);
            let stream_b = StreamId::named(&name_b);
            prop_assume!(stream_a != stream_b);
            let mut a = SimRng::new(seed, stream_a);
            let mut b = SimRng::new(seed, stream_b);
            let mut fresh_b = SimRng::new(seed, stream_b);

            draw_u64(&mut a, extra);
            let from_b = draw_u64(&mut b, count);

            prop_assert_eq!(from_b, draw_u64(&mut fresh_b, count));
        }

        #[test]
        fn postcard_round_trip_continues_sequence(
            seed in any::<u64>(),
            name in name_strategy(),
            drawn in 0usize..64,
            next in 1usize..64,
        ) {
            let mut original = SimRng::new(seed, StreamId::named(&name));
            draw_u64(&mut original, drawn);

            let bytes = postcard::to_allocvec(&original).unwrap();
            let mut restored: SimRng = postcard::from_bytes(&bytes).unwrap();

            prop_assert_eq!(&restored, &original);
            prop_assert_eq!(draw_u64(&mut restored, next), draw_u64(&mut original, next));
        }

        #[test]
        fn distinct_seeds_give_distinct_first_value(
            a in any::<u64>(),
            bit in 0u32..64,
            name in name_strategy(),
        ) {
            let b = a ^ (1u64 << bit);
            let stream = StreamId::named(&name);

            let first_a = SimRng::new(a, stream).next_u64();
            let first_b = SimRng::new(b, stream).next_u64();

            prop_assert_ne!(first_a, first_b);
        }

        #[test]
        fn next_f32_is_in_unit_interval(
            seed in any::<u64>(),
            name in name_strategy(),
        ) {
            let mut rng = SimRng::new(seed, StreamId::named(&name));

            for _ in 0..64 {
                let x = rng.next_f32();
                prop_assert!((0.0..1.0).contains(&x), "{x} outside [0, 1)");
            }
        }
    }

    #[test]
    fn fixed_seed_draws_recorded_known_values() {
        let mut ints = SimRng::new(42, StreamId::named("test.known"));
        let mut floats = SimRng::new(42, StreamId::named("test.known"));

        let u = draw_u64(&mut ints, 4);
        let f = [floats.next_f32(), floats.next_f32()];

        assert_eq!(
            u,
            [
                0x9322_df1e_3a7c_f004,
                0x3e22_38b5_e4d5_af72,
                0xb533_81f7_13cf_31ee,
                0x78c6_21a3_0eec_1510,
            ]
        );
        assert_eq!(f[0].to_bits(), (9_642_719.0_f32 / 16_777_216.0).to_bits());
        assert_eq!(f[1].to_bits(), (4_071_992.0_f32 / 16_777_216.0).to_bits());
    }

    #[test]
    fn distinct_streams_give_distinct_first_value() {
        let a = StreamId::named("test.a");
        let b = StreamId::named("test.b");

        let first_a = SimRng::new(7, a).next_u64();
        let first_b = SimRng::new(7, b).next_u64();

        assert_ne!(a, b);
        assert_ne!(first_a, first_b);
    }

    #[test]
    fn next_f32_of_max_bits_is_below_one() {
        let largest = unit_f32(u64::MAX);
        let smallest_step = unit_f32(1 << 40);

        assert!(largest < 1.0);
        assert_eq!(
            largest.to_bits(),
            (16_777_215.0_f32 / 16_777_216.0).to_bits()
        );
        assert_eq!(smallest_step.to_bits(), (1.0_f32 / 16_777_216.0).to_bits());
        assert_eq!(unit_f32((1 << 40) - 1), 0.0);
    }

    #[test]
    fn stream_id_named_matches_fnv1a_known_value() {
        const KNOWN: StreamId = StreamId::named("test.known");
        let empty = StreamId::named("");
        let a = StreamId::named("a");
        let test_known = StreamId::named("test.known");

        assert_eq!(empty, StreamId(0xcbf2_9ce4_8422_2325));
        assert_eq!(a, StreamId(0xaf63_dc4c_8601_ec8c));
        assert_eq!(test_known, StreamId(0x3106_ca11_9262_65d0));
        assert_eq!(KNOWN, test_known);
    }

    #[test]
    fn serialized_bytes_are_pinned() {
        let expected = [0xef, 0x9b, 0xaf, 0xcd, 0xf8, 0xac, 0xd1, 0x91, 0x01];

        let mut restored: SimRng = postcard::from_bytes(&expected).unwrap();
        let reserialized = postcard::to_allocvec(&restored).unwrap();
        let next = restored.next_u64();

        assert_eq!(reserialized, expected);
        assert_eq!(next, 0x157a_3807_a48f_aa9d);
    }
}
