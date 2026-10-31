//! What a ray can hit, and its round trip through the engine's opaque [`HitTag`].

use moho_physics::HitTag;
use searchlight_rules::BodyPart;

/// The game's meaning for a surface a ray reached. Enemies share the `Part` form.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SurfaceTag {
    /// Level geometry: stops a ray, damages nothing.
    World,
    /// One body part of the target numbered `target`.
    Part { target: u16, part: BodyPart },
    /// One body part of the player.
    Player(BodyPart),
    /// The placed control at this index of the arena's controls: stops a ray, damages nothing.
    Control(u16),
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

/// Engine tag of the player's first part, above every target part tag.
const PLAYER_BASE: u32 = PART_BASE + (1 << (16 + PART_BITS));

/// Engine tag of the first control, above every player tag.
const CONTROL_BASE: u32 = PLAYER_BASE + PARTS.len() as u32;

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
            SurfaceTag::Control(index) => HitTag(CONTROL_BASE + u32::from(index)),
            SurfaceTag::Player(part) => HitTag(PLAYER_BASE + part_index(part)),
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
        if let Some(index) = tag.0.checked_sub(CONTROL_BASE) {
            return u16::try_from(index)
                .map(SurfaceTag::Control)
                .map_err(|_| UnknownTag(tag.0));
        }
        if let Some(index) = tag.0.checked_sub(PLAYER_BASE) {
            return PARTS
                .get(index as usize)
                .map(|&part| SurfaceTag::Player(part))
                .ok_or(UnknownTag(tag.0));
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
            any::<u16>().prop_map(SurfaceTag::Control),
        ]
    }

    fn surface_tags_with_player() -> impl Strategy<Value = SurfaceTag> {
        prop_oneof![
            surface_tags(),
            proptest::sample::select(PARTS.to_vec()).prop_map(SurfaceTag::Player),
        ]
    }

    proptest! {
        #[test]
        fn surface_tags_round_trip(tag in surface_tags_with_player()) {
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

    proptest! {
        #[test]
        fn player_tags_never_collide_with_target_or_world_tags(
            part in proptest::sample::select(PARTS.to_vec()),
            other in surface_tags(),
        ) {
            let player = HitTag::from(SurfaceTag::Player(part));

            prop_assert_ne!(player, HitTag::from(other));
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

#[cfg(test)]
mod player_encoding_tests {
    use super::*;

    #[test]
    fn player_tags_start_one_past_the_world_tag_above_every_target_part() {
        let head = HitTag::from(SurfaceTag::Player(BodyPart::Head));
        let right_leg = HitTag::from(SurfaceTag::Player(BodyPart::RightLeg));
        let last_target_part = HitTag::from(SurfaceTag::Part {
            target: u16::MAX,
            part: BodyPart::RightLeg,
        });

        assert_eq!(head, HitTag(524_289));
        assert_eq!(right_leg, HitTag(524_294));
        assert!(last_target_part.0 < head.0);
    }

    #[test]
    fn control_tags_sit_above_every_player_tag_and_stay_in_order() {
        let last_player_part = HitTag::from(SurfaceTag::Player(BodyPart::RightLeg));

        let first = HitTag::from(SurfaceTag::Control(0));
        let second = HitTag::from(SurfaceTag::Control(1));
        let last = HitTag::from(SurfaceTag::Control(u16::MAX));

        assert!(first.0 > last_player_part.0, "{first:?}");
        assert!(second.0 > first.0, "{second:?}");
        assert!(last.0 > second.0, "{last:?}");
    }
}
