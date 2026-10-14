//! Body damage: one health pool plus per-part status pools.

use serde::{Deserialize, Serialize};

/// A hittable part of the body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BodyPart {
    /// Head; status at zero kills at once.
    Head,
    /// Torso; has no status pool.
    Torso,
    /// Left arm; status at zero disables it.
    LeftArm,
    /// Right arm; status at zero disables it.
    RightArm,
    /// Left leg; status at zero disables it.
    LeftLeg,
    /// Right leg; status at zero disables it.
    RightLeg,
}

/// Supplied pool sizes for a [`Body`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BodyProfile {
    /// Size of the main health pool.
    pub max_health: u32,
    /// Status pool of the head.
    pub head_status: u32,
    /// Status pool of each arm.
    pub arm_status: u32,
    /// Status pool of each leg.
    pub leg_status: u32,
}

/// What a single hit did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HitReport {
    /// Part that was struck.
    pub part: BodyPart,
    /// Damage dealt to the hit.
    pub damage: u32,
    /// Whether the struck part's status is now at zero.
    pub part_down: bool,
    /// Whether the body is dead after the hit.
    pub body_dead: bool,
}

/// A body tracking health and part status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Body {
    health: u32,
    profile: BodyProfile,
}

impl Body {
    /// A fresh body with every pool full.
    #[must_use]
    pub fn new(profile: BodyProfile) -> Self {
        Self {
            health: profile.max_health,
            profile,
        }
    }

    /// Applies `damage` to the main pool and the struck part's status pool.
    ///
    /// Pools saturate at zero and no other part's status changes.
    pub fn hit(&mut self, part: BodyPart, damage: u32) -> HitReport {
        HitReport {
            part,
            damage,
            part_down: false,
            body_dead: false,
        }
    }

    /// Remaining main health.
    #[must_use]
    pub fn health(&self) -> u32 {
        self.health
    }

    /// Remaining status of `part`, or `None` for the torso.
    #[must_use]
    pub fn status(&self, part: BodyPart) -> Option<u32> {
        match part {
            BodyPart::Head => Some(self.profile.head_status),
            BodyPart::Torso => None,
            BodyPart::LeftArm | BodyPart::RightArm => Some(self.profile.arm_status),
            BodyPart::LeftLeg | BodyPart::RightLeg => Some(self.profile.leg_status),
        }
    }

    /// Whether the body is dead.
    #[must_use]
    pub fn is_dead(&self) -> bool {
        false
    }

    /// Whether `part` is a limb whose status has reached zero.
    #[must_use]
    pub fn is_disabled(&self, part: BodyPart) -> bool {
        let _ = part;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const PROFILE: BodyProfile = BodyProfile {
        max_health: 100,
        head_status: 30,
        arm_status: 40,
        leg_status: 50,
    };
    const LIMBS: [BodyPart; 4] = [
        BodyPart::LeftArm,
        BodyPart::RightArm,
        BodyPart::LeftLeg,
        BodyPart::RightLeg,
    ];
    const ALL: [BodyPart; 6] = [
        BodyPart::Head,
        BodyPart::Torso,
        BodyPart::LeftArm,
        BodyPart::RightArm,
        BodyPart::LeftLeg,
        BodyPart::RightLeg,
    ];

    fn initial(profile: &BodyProfile, part: BodyPart) -> Option<u32> {
        match part {
            BodyPart::Head => Some(profile.head_status),
            BodyPart::Torso => None,
            BodyPart::LeftArm | BodyPart::RightArm => Some(profile.arm_status),
            BodyPart::LeftLeg | BodyPart::RightLeg => Some(profile.leg_status),
        }
    }

    #[test]
    fn a_hit_drains_health_and_only_the_struck_parts_status() {
        let mut body = Body::new(PROFILE);

        let head = body.hit(BodyPart::Head, 10);
        let torso = body.hit(BodyPart::Torso, 15);
        let leg = body.hit(BodyPart::LeftLeg, 20);

        assert_eq!(body.health(), 55);
        assert_eq!(body.status(BodyPart::Head), Some(20));
        assert_eq!(body.status(BodyPart::Torso), None);
        assert_eq!(body.status(BodyPart::LeftLeg), Some(30));
        assert_eq!(body.status(BodyPart::RightLeg), Some(50));
        assert_eq!(body.status(BodyPart::LeftArm), Some(40));
        assert_eq!(body.status(BodyPart::RightArm), Some(40));
        assert_eq!(
            (head.part, head.damage, head.part_down, head.body_dead),
            (BodyPart::Head, 10, false, false)
        );
        assert_eq!(
            (torso.part, torso.damage, torso.part_down, torso.body_dead),
            (BodyPart::Torso, 15, false, false)
        );
        assert_eq!(
            (leg.part, leg.damage, leg.part_down, leg.body_dead),
            (BodyPart::LeftLeg, 20, false, false)
        );
        assert!(!body.is_dead());
    }

    #[test]
    fn head_status_at_zero_kills_with_health_left() {
        let mut body = Body::new(PROFILE);

        let report = body.hit(BodyPart::Head, 30);

        assert_eq!(body.health(), 70);
        assert_eq!(body.status(BodyPart::Head), Some(0));
        assert!(body.is_dead());
        assert!(report.part_down);
        assert!(report.body_dead);
    }

    #[test]
    fn limb_status_at_zero_disables_and_the_body_lives() {
        for limb in LIMBS {
            let mut body = Body::new(PROFILE);
            let pool = body.status(limb).expect("limbs have a status pool");

            let report = body.hit(limb, pool);

            assert!(body.is_disabled(limb), "{limb:?}");
            assert!(!body.is_dead(), "{limb:?}");
            assert_eq!(body.status(limb), Some(0), "{limb:?}");
            assert_eq!(body.health(), 100 - pool, "{limb:?}");
            assert!(report.part_down, "{limb:?}");
            assert!(!report.body_dead, "{limb:?}");
            for other in LIMBS.into_iter().filter(|p| *p != limb) {
                assert!(!body.is_disabled(other), "{limb:?} disabled {other:?}");
            }
        }
    }

    #[test]
    fn health_at_zero_kills_whatever_part_was_hit() {
        for part in ALL {
            let mut body = Body::new(PROFILE);

            let report = body.hit(part, 100);

            assert_eq!(body.health(), 0, "{part:?}");
            assert!(body.is_dead(), "{part:?}");
            assert!(report.body_dead, "{part:?}");
        }
    }

    #[test]
    fn a_disabled_limb_still_drains_health() {
        let mut body = Body::new(PROFILE);
        body.hit(BodyPart::LeftArm, 40);

        let report = body.hit(BodyPart::LeftArm, 10);

        assert_eq!(body.health(), 50);
        assert_eq!(body.status(BodyPart::LeftArm), Some(0));
        assert!(body.is_disabled(BodyPart::LeftArm));
        assert!(!body.is_dead());
        assert!(!report.body_dead);
    }

    fn any_part() -> impl Strategy<Value = BodyPart> {
        proptest::sample::select(ALL.to_vec())
    }

    proptest! {
        #[test]
        fn pools_stop_at_zero_and_hits_never_touch_other_parts(
            max_health in 1u32..500,
            head_status in 1u32..500,
            arm_status in 1u32..500,
            leg_status in 1u32..500,
            hits in proptest::collection::vec((any_part(), 0u32..600), 0..20),
        ) {
            let profile = BodyProfile { max_health, head_status, arm_status, leg_status };
            let mut body = Body::new(profile);
            let mut health = max_health;
            let mut statuses: Vec<(BodyPart, Option<u32>)> =
                ALL.iter().map(|p| (*p, initial(&profile, *p))).collect();

            for (part, damage) in hits {
                body.hit(part, damage);
                health = health.saturating_sub(damage);
                for (p, pool) in &mut statuses {
                    if *p == part {
                        *pool = pool.map(|v| v.saturating_sub(damage));
                    }
                }
            }

            prop_assert_eq!(body.health(), health);
            for (part, pool) in statuses {
                prop_assert_eq!(body.status(part), pool);
            }
            let head_down = body.status(BodyPart::Head) == Some(0);
            prop_assert_eq!(body.is_dead(), health == 0 || head_down);
        }
    }
}
