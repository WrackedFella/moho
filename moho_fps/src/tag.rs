//! What a ray can hit, and its round trip through the engine's opaque [`HitTag`].

use moho_fps_rules::BodyPart;
use moho_physics::HitTag;

/// The game's meaning for a surface a ray reached. Enemies share the `Part` form.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SurfaceTag {
    /// Level geometry: stops a ray, damages nothing.
    World,
    /// One body part of the target numbered `target`.
    Part { target: u16, part: BodyPart },
}

/// The engine tag does not encode any [`SurfaceTag`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("tag {0} is not a surface tag")]
pub struct UnknownTag(pub u32);

/// Engine tag of [`SurfaceTag::World`].
const WORLD: u32 = 0;
/// Engine tag of the first part; every part tag is at or above it.
const PART_BASE: u32 = WORLD + 1;
/// Bits of a part tag that hold the part's index; the target number sits above them.
const PART_BITS: u32 = 3;

/// Every part, at the index its tag stores.
const PARTS: [BodyPart; 6] = [
    BodyPart::Head,
    BodyPart::Torso,
    BodyPart::LeftArm,
    BodyPart::RightArm,
    BodyPart::LeftLeg,
    BodyPart::RightLeg,
];

fn part_index(part: BodyPart) -> u32 {
    PARTS
        .iter()
        .position(|&p| p == part)
        .map(|index| index as u32)
        .expect("PARTS lists every BodyPart")
}

impl From<SurfaceTag> for HitTag {
    fn from(tag: SurfaceTag) -> Self {
        match tag {
            SurfaceTag::World => HitTag(WORLD),
            SurfaceTag::Part { target, part } => {
                HitTag(PART_BASE + ((u32::from(target) << PART_BITS) | part_index(part)))
            }
        }
    }
}

impl TryFrom<HitTag> for SurfaceTag {
    type Error = UnknownTag;

    fn try_from(tag: HitTag) -> Result<Self, Self::Error> {
        if tag.0 == WORLD {
            return Ok(SurfaceTag::World);
        }
        let packed = tag.0 - PART_BASE;
        let part = PARTS.get((packed & ((1 << PART_BITS) - 1)) as usize);
        let target = u16::try_from(packed >> PART_BITS);
        match (part, target) {
            (Some(&part), Ok(target)) => Ok(SurfaceTag::Part { target, part }),
            _ => Err(UnknownTag(tag.0)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const PARTS: [BodyPart; 6] = [
        BodyPart::Head,
        BodyPart::Torso,
        BodyPart::LeftArm,
        BodyPart::RightArm,
        BodyPart::LeftLeg,
        BodyPart::RightLeg,
    ];

    fn surface_tags() -> impl Strategy<Value = SurfaceTag> {
        prop_oneof![
            Just(SurfaceTag::World),
            (any::<u16>(), proptest::sample::select(PARTS.to_vec()))
                .prop_map(|(target, part)| SurfaceTag::Part { target, part }),
        ]
    }

    proptest! {
        #[test]
        fn surface_tags_round_trip(tag in surface_tags()) {
            let engine = HitTag::from(tag);

            let back = SurfaceTag::try_from(engine);

            prop_assert_eq!(back, Ok(tag));
        }

        #[test]
        fn distinct_surface_tags_get_distinct_engine_tags(a in surface_tags(), b in surface_tags()) {
            prop_assume!(a != b);

            prop_assert_ne!(HitTag::from(a), HitTag::from(b));
        }
    }

    #[test]
    fn unknown_tag_is_rejected() {
        let unknown = HitTag(u32::MAX);

        let decoded = SurfaceTag::try_from(unknown);

        assert_eq!(decoded, Err(UnknownTag(u32::MAX)));
    }
}

#[cfg(test)]
mod encoding_tests {
    use super::*;

    #[test]
    fn part_tags_pack_the_target_above_three_index_bits_after_the_world_tag() {
        let tag = |target, part| HitTag::from(SurfaceTag::Part { target, part });

        assert_eq!(HitTag::from(SurfaceTag::World), HitTag(0));
        assert_eq!(tag(0, BodyPart::Head), HitTag(1));
        assert_eq!(tag(1, BodyPart::Torso), HitTag(10));
        assert_eq!(tag(3, BodyPart::LeftArm), HitTag(27));
        assert_eq!(tag(2, BodyPart::RightLeg), HitTag(22));
    }

    #[test]
    fn part_indices_past_the_last_part_are_rejected_not_clamped_or_wrapped() {
        // Tag = 1 + (target << 3 | index); indices 6 and 7 name no part.
        let past_the_parts = [7, 8, 15, 16, 1 + (500 << 3 | 6), 1 + (500 << 3 | 7)];

        for raw in past_the_parts {
            let decoded = SurfaceTag::try_from(HitTag(raw));

            assert_eq!(decoded, Err(UnknownTag(raw)), "tag {raw}");
        }
    }
}
