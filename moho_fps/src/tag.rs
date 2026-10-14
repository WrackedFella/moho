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

impl From<SurfaceTag> for HitTag {
    fn from(_tag: SurfaceTag) -> Self {
        unimplemented!("encode the surface tag as an engine tag")
    }
}

impl TryFrom<HitTag> for SurfaceTag {
    type Error = UnknownTag;

    fn try_from(_tag: HitTag) -> Result<Self, Self::Error> {
        unimplemented!("decode an engine tag into a surface tag")
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
