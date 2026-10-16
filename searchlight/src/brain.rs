//! The practice enemy's decision-making: a patrol route, noticing the player, and firing.
//!
//! Pure over [`glam`] types: the sim feeds in what the enemy perceives and applies the
//! [`Intent`] that comes back.

use std::f32::consts::PI;

use glam::Vec3;

/// What the enemy knows this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Percept {
    pub position: Vec3,
    /// Radians; the enemy faces `(sin yaw, 0, cos yaw)`, so yaw 0 faces +Z.
    pub yaw: f32,
    /// The player's aim point, set only when the sight ray reached a player part.
    pub target: Option<Vec3>,
    /// The player's position when it fired, set the tick after a non-lethal hit.
    pub hurt: Option<Vec3>,
    /// A cover point to run to; set by the sim only when `hurt`.
    pub offer: Option<CoverOffer>,
}

/// A cover point the sim offers to a hurt enemy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CoverOffer {
    pub index: usize,
    pub position: Vec3,
}

/// What the enemy wants to do this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Intent {
    /// Fraction of full walking speed, 0 to 1.
    pub forward: f32,
    /// Radians to turn this tick, positive towards +X at yaw 0.
    pub yaw_delta: f32,
    pub fire: bool,
}

/// Tunable numbers for one enemy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnemyTuning {
    /// Metres.
    pub sight_range: f32,
    /// Radians either side of the facing direction in which a target is noticed.
    pub view_half_angle: f32,
    pub reaction_ticks: u32,
    pub cooldown_ticks: u32,
    pub damage: u32,
    /// Radians of random aim error.
    pub aim_error: f32,
    /// Radians per second.
    pub turn_rate: f32,
    /// Fraction of full walking speed while patrolling.
    pub patrol_speed: f32,
    /// Metres from a waypoint at which it counts as reached.
    pub arrive_radius: f32,
    /// Radians either side of the target bearing within which the enemy may fire.
    pub fire_cone: f32,
    /// Metres within which a hurt enemy is offered a cover point.
    pub cover_range: f32,
    /// Ticks a running-to-cover enemy tries before it gives up and fights where it is.
    pub cover_ticks: u32,
    /// Ticks a chasing enemy tries before it returns to its patrol.
    pub chase_ticks: u32,
    /// Metres above a cover marker at which the player must be unable to see the enemy.
    pub cover_hide_height: f32,
    pub seed: u64,
}

impl Default for EnemyTuning {
    fn default() -> Self {
        Self {
            sight_range: 30.0,
            view_half_angle: 60.0_f32.to_radians(),
            reaction_ticks: 30,
            cooldown_ticks: 12,
            damage: 15,
            aim_error: 0.03,
            turn_rate: 3.0,
            patrol_speed: 0.5,
            arrive_radius: 0.5,
            fire_cone: 0.1,
            cover_range: 15.0,
            cover_ticks: 240,
            chase_ticks: 600,
            cover_hide_height: 0.9,
            seed: 0,
        }
    }
}

/// The enemy's state machine.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Brain {
    /// Walking the route towards waypoint `next`.
    Patrol { next: usize },
    /// Standing and facing a seen target (no chase); `next` is the route position to resume from.
    Engage {
        next: usize,
        /// Consecutive ticks the target has been seen, this one included.
        seen_ticks: u32,
        /// Ticks until the enemy may fire again.
        cooldown: u32,
        held: Option<usize>,
        last_seen: Vec3,
    },
    Cover {
        next: usize,
        offer: CoverOffer,
        last_seen: Vec3,
        /// Ticks spent running, this one included.
        ticks: u32,
    },
    Chase {
        next: usize,
        last_seen: Vec3,
        /// Ticks spent chasing, this one included.
        ticks: u32,
    },
}

struct Tick<'a> {
    route: &'a [Vec3],
    percept: &'a Percept,
    tuning: &'a EnemyTuning,
    max_turn: f32,
}

/// The state [`Tick::engage`] continues from.
struct Stance {
    next: usize,
    seen_ticks: u32,
    cooldown: u32,
    held: Option<usize>,
    last_seen: Vec3,
}

impl Stance {
    fn fresh(next: usize, held: Option<usize>, last_seen: Vec3) -> Self {
        Self {
            next,
            seen_ticks: 0,
            cooldown: 0,
            held,
            last_seen,
        }
    }
}

impl Tick<'_> {
    fn walk_to(&self, point: Vec3, forward: f32) -> Intent {
        Intent {
            forward,
            yaw_delta: turn_towards(self.percept, point, self.max_turn).0,
            fire: false,
        }
    }

    fn patrol(&self, next: usize) -> (Brain, Intent) {
        let arrived =
            flat_distance(self.percept.position, self.route[next]) <= self.tuning.arrive_radius;
        let next = if arrived {
            (next + 1) % self.route.len()
        } else {
            next
        };
        (
            Brain::Patrol { next },
            self.walk_to(self.route[next], self.tuning.patrol_speed),
        )
    }

    /// Faces `target` and fires when ready; without a target, holds still.
    fn engage(&self, stance: Stance, target: Option<Vec3>) -> (Brain, Intent) {
        let Stance {
            next,
            seen_ticks,
            cooldown,
            held,
            last_seen,
        } = stance;
        let Some(target) = target else {
            let brain = Brain::Engage {
                next,
                seen_ticks: 0,
                cooldown: 0,
                held,
                last_seen,
            };
            return (
                brain,
                Intent {
                    forward: 0.0,
                    yaw_delta: 0.0,
                    fire: false,
                },
            );
        };
        let (yaw_delta, gap) = turn_towards(self.percept, target, self.max_turn);
        let seen_ticks = seen_ticks.saturating_add(1);
        let cooldown = cooldown.saturating_sub(1);
        let fire = seen_ticks >= self.tuning.reaction_ticks
            && cooldown == 0
            && gap.abs() <= self.tuning.fire_cone;
        let brain = Brain::Engage {
            next,
            seen_ticks,
            cooldown: if fire {
                self.tuning.cooldown_ticks
            } else {
                cooldown
            },
            held,
            last_seen: target,
        };
        (
            brain,
            Intent {
                forward: 0.0,
                yaw_delta,
                fire,
            },
        )
    }

    fn chase(&self, next: usize, last_seen: Vec3, ticks: u32) -> (Brain, Intent) {
        let arrived = flat_distance(self.percept.position, last_seen) <= self.tuning.arrive_radius;
        if arrived || ticks >= self.tuning.chase_ticks {
            return self.patrol(next);
        }
        let brain = Brain::Chase {
            next,
            last_seen,
            ticks: ticks + 1,
        };
        (brain, self.walk_to(last_seen, self.tuning.patrol_speed))
    }

    fn cover(
        &self,
        next: usize,
        offer: CoverOffer,
        last_seen: Vec3,
        ticks: u32,
    ) -> (Brain, Intent) {
        let target = self.percept.target;
        if ticks >= self.tuning.cover_ticks {
            return self.engage(Stance::fresh(next, None, last_seen), target);
        }
        if flat_distance(self.percept.position, offer.position) <= self.tuning.arrive_radius {
            return self.engage(Stance::fresh(next, Some(offer.index), last_seen), target);
        }
        let brain = Brain::Cover {
            next,
            offer,
            last_seen,
            ticks: ticks + 1,
        };
        (brain, self.walk_to(offer.position, 1.0))
    }
}

impl Brain {
    pub fn claim(&self) -> Option<usize> {
        match *self {
            Self::Cover { offer, .. } => Some(offer.index),
            Self::Engage { held, .. } => held,
            Self::Patrol { .. } | Self::Chase { .. } => None,
        }
    }

    /// Being hit reveals the shooter; a brain not yet in cover takes an offered point.
    fn react_to_hurt(&mut self, shooter: Vec3, offer: Option<CoverOffer>) {
        match (*self, offer) {
            (
                Self::Patrol { next }
                | Self::Chase { next, .. }
                | Self::Engage {
                    next, held: None, ..
                },
                Some(offer),
            ) => {
                *self = Self::Cover {
                    next,
                    offer,
                    last_seen: shooter,
                    ticks: 0,
                };
            }
            (Self::Patrol { next } | Self::Chase { next, .. }, None) => {
                *self = Self::Engage {
                    next,
                    seen_ticks: 0,
                    cooldown: 0,
                    held: None,
                    last_seen: shooter,
                };
            }
            _ => {
                if let Self::Engage { last_seen, .. } | Self::Cover { last_seen, .. } = self {
                    *last_seen = shooter;
                }
            }
        }
    }

    /// Advance one tick of `dt` seconds and say what the enemy does.
    ///
    /// `route` must not be empty.
    pub fn think(
        &mut self,
        route: &[Vec3],
        percept: &Percept,
        tuning: &EnemyTuning,
        dt: f32,
    ) -> Intent {
        if let Some(shooter) = percept.hurt {
            self.react_to_hurt(shooter, percept.offer);
        }
        let engaged = matches!(self, Self::Engage { .. });
        let seen = percept
            .target
            .filter(|&target| engaged || notices(percept, target, tuning));
        let tick = Tick {
            route,
            percept,
            tuning,
            max_turn: tuning.turn_rate * dt,
        };
        let (brain, intent) = match *self {
            Self::Patrol { next } => match seen {
                Some(target) => tick.engage(Stance::fresh(next, None, target), seen),
                None => tick.patrol(next),
            },
            Self::Chase {
                next,
                last_seen,
                ticks,
            } => match seen {
                Some(target) => tick.engage(Stance::fresh(next, None, target), seen),
                None => tick.chase(next, last_seen, ticks),
            },
            Self::Cover {
                next,
                offer,
                last_seen,
                ticks,
            } => tick.cover(next, offer, last_seen, ticks),
            Self::Engage {
                next,
                seen_ticks,
                cooldown,
                held,
                last_seen,
            } => match seen {
                Some(_) => tick.engage(
                    Stance {
                        next,
                        seen_ticks,
                        cooldown,
                        held,
                        last_seen,
                    },
                    seen,
                ),
                None => tick.chase(next, last_seen, 0),
            },
        };
        *self = brain;
        intent
    }
}

pub(crate) fn flat_distance(a: Vec3, b: Vec3) -> f32 {
    (b.x - a.x).hypot(b.z - a.z)
}

/// Wraps an angle into `[-PI, PI)`.
fn wrap(angle: f32) -> f32 {
    (angle + PI).rem_euclid(2.0 * PI) - PI
}

/// The turn this tick towards `point`, clamped to `max_turn`, and the full signed gap.
fn turn_towards(percept: &Percept, point: Vec3, max_turn: f32) -> (f32, f32) {
    let offset = point - percept.position;
    let gap = wrap(offset.x.atan2(offset.z) - percept.yaw);
    (gap.clamp(-max_turn, max_turn), gap)
}

/// Whether `target` is within sight range and the view cone.
fn notices(percept: &Percept, target: Vec3, tuning: &EnemyTuning) -> bool {
    let (_, gap) = turn_towards(percept, target, 0.0);
    percept.position.distance(target) <= tuning.sight_range && gap.abs() <= tuning.view_half_angle
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    const DT: f32 = 0.1;

    fn tuning() -> EnemyTuning {
        EnemyTuning {
            sight_range: 30.0,
            view_half_angle: 60.0_f32.to_radians(),
            reaction_ticks: 3,
            cooldown_ticks: 4,
            damage: 10,
            aim_error: 0.0,
            turn_rate: 2.0,
            patrol_speed: 0.5,
            arrive_radius: 2.0,
            fire_cone: 0.1,
            cover_range: 10.0,
            cover_ticks: 5,
            chase_ticks: 7,
            cover_hide_height: 0.9,
            seed: 7,
        }
    }

    fn bearing(angle: f32, distance: f32) -> Vec3 {
        Vec3::new(angle.sin(), 0.0, angle.cos()) * distance
    }

    fn at_origin(yaw: f32, target: Option<Vec3>) -> Percept {
        Percept {
            position: Vec3::ZERO,
            yaw,
            target,
            hurt: None,
            offer: None,
        }
    }

    fn square() -> Vec<Vec3> {
        vec![
            Vec3::new(10.0, 0.0, 0.0),
            Vec3::new(10.0, 0.0, 10.0),
            Vec3::new(0.0, 0.0, 10.0),
            Vec3::new(0.0, 0.0, 0.0),
        ]
    }

    fn wrap(angle: f32) -> f32 {
        (angle + PI).rem_euclid(2.0 * PI) - PI
    }

    #[test]
    fn an_unseen_enemy_walks_toward_its_next_waypoint() {
        let route = [Vec3::new(0.0, 0.0, 10.0)];
        let mut brain = Brain::Patrol { next: 0 };

        let intent = brain.think(&route, &at_origin(0.0, None), &tuning(), DT);

        assert_eq!(intent.forward, 0.5);
        assert!(intent.yaw_delta.abs() < 1e-5, "{intent:?}");
        assert!(!intent.fire);
        assert_eq!(brain, Brain::Patrol { next: 0 });
    }

    #[test]
    fn arriving_advances_the_route_and_wraps() {
        let route = square();
        let tuning = tuning();
        let near = |waypoint: Vec3, gap: f32| Percept {
            position: waypoint + Vec3::new(0.0, 0.0, gap),
            yaw: 0.0,
            target: None,
            hurt: None,
            offer: None,
        };

        let mut inside = Brain::Patrol { next: 1 };
        inside.think(&route, &near(route[1], 1.9), &tuning, DT);
        let mut on_edge = Brain::Patrol { next: 1 };
        on_edge.think(&route, &near(route[1], 2.0), &tuning, DT);
        let mut outside = Brain::Patrol { next: 1 };
        outside.think(&route, &near(route[1], 2.1), &tuning, DT);
        let mut last = Brain::Patrol { next: 3 };
        last.think(&route, &near(route[3], 0.5), &tuning, DT);

        assert_eq!(inside, Brain::Patrol { next: 2 });
        assert_eq!(on_edge, Brain::Patrol { next: 2 });
        assert_eq!(outside, Brain::Patrol { next: 1 });
        assert_eq!(last, Brain::Patrol { next: 0 });
    }

    proptest! {
        #[test]
        fn turning_takes_the_short_way_round(
            yaw in -10.0_f32..10.0,
            heading in -PI..PI,
        ) {
            let tuning = tuning();
            let route = [bearing(heading, 10.0)];
            let gap = wrap(heading - yaw);
            prop_assume!(gap.abs() < PI - 0.01);
            let mut brain = Brain::Patrol { next: 0 };

            let intent = brain.think(&route, &at_origin(yaw, None), &tuning, DT);

            let step = tuning.turn_rate * DT;
            let expected = gap.clamp(-step, step);
            prop_assert!(intent.yaw_delta.abs() <= step + 1e-4, "{intent:?}");
            prop_assert!((intent.yaw_delta - expected).abs() < 1e-3, "{intent:?} vs {expected}");
        }
    }

    #[test]
    fn an_enemy_waits_out_its_reaction_time_then_fires_on_cooldown() {
        let percept = at_origin(0.0, Some(bearing(0.0, 10.0)));
        let route = square();
        let tuning = tuning();
        let mut brain = Brain::Patrol { next: 0 };

        let fired: Vec<bool> = (0..8)
            .map(|_| brain.think(&route, &percept, &tuning, DT).fire)
            .collect();

        assert_eq!(
            fired,
            [false, false, true, false, false, false, true, false]
        );
    }

    #[test]
    fn an_enemy_does_not_fire_until_it_faces_the_player() {
        let route = square();
        let tuning = tuning();
        let ready = Brain::Engage {
            next: 0,
            seen_ticks: 100,
            cooldown: 0,
            held: None,
            last_seen: Vec3::ZERO,
        };
        let fires_at = |off_by: f32| {
            let mut brain = ready;
            let percept = at_origin(0.0, Some(bearing(off_by, 10.0)));
            brain.think(&route, &percept, &tuning, DT)
        };

        let inside = fires_at(0.09);
        let outside = fires_at(0.11);
        let behind = fires_at(PI * 0.9);

        assert!(inside.fire);
        assert!(!outside.fire);
        assert!(!behind.fire);
        assert!(
            outside.yaw_delta > 0.0,
            "it should turn to face: {outside:?}"
        );
    }

    #[test]
    fn a_target_outside_the_view_cone_is_not_noticed() {
        let route = square();
        let tuning = tuning();
        let half = tuning.view_half_angle;
        let notices = |angle: f32, distance: f32| {
            let mut brain = Brain::Patrol { next: 2 };
            brain.think(
                &route,
                &at_origin(0.0, Some(bearing(angle, distance))),
                &tuning,
                DT,
            );
            matches!(brain, Brain::Engage { .. })
        };

        assert!(notices(half - 0.02, 10.0));
        assert!(notices(-(half - 0.02), 10.0));
        assert!(!notices(half + 0.02, 10.0));
        assert!(!notices(-(half + 0.02), 10.0));
        assert!(notices(0.0, 29.0));
        assert!(!notices(0.0, 31.0));
    }

    #[test]
    fn an_engaged_enemy_keeps_a_target_outside_the_cone() {
        let route = square();
        let tuning = tuning();
        let behind = at_origin(0.0, Some(bearing(PI * 0.8, 10.0)));
        let mut brain = Brain::Engage {
            next: 1,
            seen_ticks: 5,
            cooldown: 2,
            held: None,
            last_seen: Vec3::ZERO,
        };

        let intent = brain.think(&route, &behind, &tuning, DT);

        assert!(matches!(brain, Brain::Engage { next: 1, .. }), "{brain:?}");
        assert!(intent.yaw_delta > 0.0, "it should turn to face: {intent:?}");
        assert!(intent.yaw_delta <= tuning.turn_rate * DT + 1e-4);
    }

    fn offer_at(index: usize, position: Vec3) -> CoverOffer {
        CoverOffer { index, position }
    }

    /// A far-off offer, so the enemy never arrives while a test watches it run.
    fn far_offer() -> CoverOffer {
        offer_at(4, Vec3::new(0.0, 0.0, -30.0))
    }

    fn at(position: Vec3, target: Option<Vec3>) -> Percept {
        Percept {
            position,
            ..at_origin(0.0, target)
        }
    }

    fn hurt_by(shooter: Vec3, offer: Option<CoverOffer>, target: Option<Vec3>) -> Percept {
        Percept {
            hurt: Some(shooter),
            offer,
            ..at_origin(0.0, target)
        }
    }

    fn engaged(held: Option<usize>) -> Brain {
        Brain::Engage {
            next: 2,
            seen_ticks: 100,
            cooldown: 0,
            held,
            last_seen: Vec3::ZERO,
        }
    }

    fn chasing(last_seen: Vec3, ticks: u32) -> Brain {
        Brain::Chase {
            next: 2,
            last_seen,
            ticks,
        }
    }

    fn running_to(offer: CoverOffer) -> Brain {
        Brain::Cover {
            next: 2,
            offer,
            last_seen: Vec3::ZERO,
            ticks: 0,
        }
    }

    fn think_n(brain: &mut Brain, percept: &Percept, ticks: usize) -> Vec<Intent> {
        let route = square();
        let tuning = tuning();
        (0..ticks)
            .map(|_| brain.think(&route, percept, &tuning, DT))
            .collect()
    }

    #[test]
    fn a_hurt_enemy_with_an_offered_cover_point_heads_for_it() {
        let offer = offer_at(4, Vec3::new(8.0, 0.0, -6.0));
        let hurt = hurt_by(bearing(0.0, 15.0), Some(offer), None);
        let starts = [
            Brain::Patrol { next: 2 },
            engaged(None),
            chasing(bearing(0.3, 12.0), 3),
        ];

        for start in starts {
            let mut brain = start;
            let intent = think_n(&mut brain, &hurt, 1)[0];

            assert!(
                matches!(brain, Brain::Cover { next: 2, offer: o, .. } if o == offer),
                "from {start:?}: {brain:?}"
            );
            assert!(intent.forward > 0.0, "from {start:?}: {intent:?}");
            assert!(intent.yaw_delta > 0.0, "from {start:?}: {intent:?}");
            assert!(!intent.fire, "from {start:?}");
        }
    }

    #[test]
    fn an_enemy_runs_to_cover_without_firing() {
        let hurt = hurt_by(
            bearing(0.0, 10.0),
            Some(far_offer()),
            Some(bearing(0.0, 10.0)),
        );
        let visible = at_origin(0.0, Some(bearing(0.0, 10.0)));
        let mut brain = engaged(None);

        let first = think_n(&mut brain, &hurt, 1);
        let rest = think_n(&mut brain, &visible, 3);

        for intent in first.iter().chain(&rest) {
            assert!(!intent.fire, "{intent:?}");
            assert!(intent.forward > 0.0, "{intent:?}");
        }
        assert!(matches!(brain, Brain::Cover { .. }), "{brain:?}");
    }

    #[test]
    fn a_hurt_enemy_already_holding_cover_stays_put() {
        let other = offer_at(6, Vec3::new(-8.0, 0.0, 5.0));
        let hurt = hurt_by(bearing(0.0, 10.0), Some(other), Some(bearing(0.0, 10.0)));
        let mut holding = engaged(Some(2));
        let mut running = running_to(far_offer());

        think_n(&mut holding, &hurt, 1);
        think_n(&mut running, &hurt, 1);

        assert!(
            matches!(holding, Brain::Engage { held: Some(2), .. }),
            "{holding:?}"
        );
        assert_eq!(holding.claim(), Some(2));
        assert!(
            matches!(running, Brain::Cover { offer, .. } if offer == far_offer()),
            "{running:?}"
        );
        assert_eq!(running.claim(), Some(4));
    }

    #[test]
    fn an_enemy_at_its_cover_point_fires_on_cooldown_at_a_visible_target() {
        let spot = offer_at(1, Vec3::ZERO);
        let visible = at_origin(0.0, Some(bearing(0.0, 10.0)));
        let mut brain = running_to(spot);

        let fired: Vec<usize> = think_n(&mut brain, &visible, 12)
            .iter()
            .enumerate()
            .filter_map(|(tick, intent)| intent.fire.then_some(tick))
            .collect();

        assert!(
            matches!(brain, Brain::Engage { held: Some(1), .. }),
            "{brain:?}"
        );
        assert!(fired.len() >= 2, "{fired:?}");
        for pair in fired.windows(2) {
            assert_eq!(pair[1] - pair[0], 4, "{fired:?}");
        }
    }

    #[test]
    fn arriving_within_the_radius_switches_from_running_to_holding() {
        let spot = offer_at(4, Vec3::new(0.0, 0.0, 10.0));
        let after_arriving_from = |gap: f32| {
            let mut brain = running_to(spot);
            think_n(&mut brain, &at(Vec3::new(0.0, 0.0, 10.0 - gap), None), 1);
            brain
        };

        let inside = after_arriving_from(1.5);
        let on_edge = after_arriving_from(2.0);
        let outside = after_arriving_from(2.2);

        assert!(
            matches!(inside, Brain::Engage { held: Some(4), .. }),
            "{inside:?}"
        );
        assert!(
            matches!(on_edge, Brain::Engage { held: Some(4), .. }),
            "{on_edge:?}"
        );
        assert!(matches!(outside, Brain::Cover { .. }), "{outside:?}");
    }

    #[test]
    fn losing_sight_sends_an_engaged_enemy_to_the_last_seen_position() {
        let seen = bearing(0.5, 10.0);
        let starts = [Brain::Patrol { next: 2 }, engaged(Some(1))];

        for start in starts {
            let mut brain = start;
            think_n(&mut brain, &at_origin(0.0, Some(seen)), 1);
            let intent = think_n(&mut brain, &at_origin(0.0, None), 1)[0];

            assert!(
                matches!(brain, Brain::Chase { next: 2, last_seen, .. } if last_seen == seen),
                "from {start:?}: {brain:?}"
            );
            assert!(intent.forward > 0.0, "from {start:?}: {intent:?}");
            assert!(intent.yaw_delta > 0.0, "from {start:?}: {intent:?}");
            assert!(!intent.fire, "from {start:?}");
        }
    }

    #[test]
    fn being_hurt_reveals_the_shooters_position() {
        let shooter = Vec3::new(5.0, 0.0, 5.0);
        let quiet = at_origin(0.0, None);
        let without_offer = hurt_by(shooter, None, None);
        let with_offer = hurt_by(shooter, Some(far_offer()), None);

        let last_seen_after = |first: Percept| {
            let mut brain = Brain::Patrol { next: 2 };
            think_n(&mut brain, &first, 1);
            for _ in 0..20 {
                think_n(&mut brain, &quiet, 1);
                if let Brain::Chase { last_seen, .. } = brain {
                    return Some(last_seen);
                }
            }
            None
        };

        assert_eq!(last_seen_after(without_offer), Some(shooter));
        assert_eq!(last_seen_after(with_offer), Some(shooter));
    }

    #[test]
    fn a_chasing_enemy_that_sees_the_player_again_engages() {
        let half = tuning().view_half_angle;
        let engages = |angle: f32, distance: f32| {
            let mut brain = chasing(bearing(PI, 20.0), 0);
            think_n(
                &mut brain,
                &at_origin(0.0, Some(bearing(angle, distance))),
                1,
            );
            matches!(brain, Brain::Engage { next: 2, .. })
        };

        assert!(engages(half - 0.02, 10.0));
        assert!(engages(-(half - 0.02), 10.0));
        assert!(!engages(half + 0.02, 10.0));
        assert!(!engages(-(half + 0.02), 10.0));
        assert!(engages(0.0, 29.0));
        assert!(!engages(0.0, 31.0));
    }

    #[test]
    fn arriving_at_the_last_seen_position_with_nothing_in_sight_resumes_the_patrol() {
        let last_seen = Vec3::new(0.0, 0.0, 10.0);
        let after_arriving_from = |gap: f32| {
            let mut brain = Brain::Chase {
                next: 1,
                last_seen,
                ticks: 0,
            };
            think_n(&mut brain, &at(Vec3::new(0.0, 0.0, 10.0 - gap), None), 1);
            brain
        };

        let inside = after_arriving_from(1.5);
        let on_edge = after_arriving_from(2.0);
        let outside = after_arriving_from(2.2);

        assert_eq!(inside, Brain::Patrol { next: 1 });
        assert_eq!(on_edge, Brain::Patrol { next: 1 });
        assert!(matches!(outside, Brain::Chase { .. }), "{outside:?}");
    }

    #[test]
    fn a_hurt_enemy_with_no_offer_keeps_firing() {
        let target = bearing(0.0, 10.0);
        let hurt = hurt_by(target, None, Some(target));
        let visible = at_origin(0.0, Some(target));
        let starts = [
            Brain::Patrol { next: 2 },
            engaged(None),
            chasing(bearing(0.3, 12.0), 3),
        ];

        for start in starts {
            let mut brain = start;
            let mut intents = think_n(&mut brain, &hurt, 1);
            intents.extend(think_n(&mut brain, &visible, 8));

            assert!(
                matches!(brain, Brain::Engage { held: None, .. }),
                "from {start:?}: {brain:?}"
            );
            assert!(
                intents.iter().any(|intent| intent.fire),
                "from {start:?}: {intents:?}"
            );
        }
    }

    /// Thinks `ticks` times, applying each turn to the enemy's yaw so it can come to face
    /// the target; the target stays where it is.
    fn think_turning(
        brain: &mut Brain,
        first: Percept,
        target: Option<Vec3>,
        ticks: usize,
    ) -> Vec<Intent> {
        let route = square();
        let tuning = tuning();
        let mut percept = first;
        let mut intents = Vec::new();
        for _ in 0..ticks {
            let intent = brain.think(&route, &percept, &tuning, DT);
            percept = at_origin(percept.yaw + intent.yaw_delta, target);
            intents.push(intent);
        }
        intents
    }

    #[test]
    fn a_hurt_enemy_with_no_offer_engages_a_shooter_outside_its_view_cone() {
        let behind = bearing(PI * 0.8, 10.0);
        let hurt = hurt_by(behind, None, Some(behind));
        let starts = [Brain::Patrol { next: 2 }, chasing(bearing(0.3, 12.0), 3)];

        for start in starts {
            let mut brain = start;

            let intents = think_turning(&mut brain, hurt, Some(behind), 40);

            assert!(
                intents[0].yaw_delta > 0.0,
                "from {start:?}: it should turn to face the shooter: {:?}",
                intents[0]
            );
            assert!(
                intents.iter().all(|intent| intent.forward == 0.0),
                "from {start:?}: it walked instead of standing to fight: {intents:?}"
            );
            assert!(
                intents.iter().any(|intent| intent.fire),
                "from {start:?}: it never fired: {intents:?}"
            );
            assert!(
                matches!(brain, Brain::Engage { held: None, .. }),
                "from {start:?}: {brain:?}"
            );
        }
    }

    #[test]
    fn a_hurt_enemy_with_no_offer_becomes_engaged_on_the_hurt_tick_even_if_the_shooter_is_behind() {
        let behind = bearing(PI * 0.8, 10.0);
        let hurt = hurt_by(behind, None, Some(behind));
        let starts = [Brain::Patrol { next: 2 }, chasing(bearing(0.3, 12.0), 3)];

        for start in starts {
            let mut brain = start;

            think_n(&mut brain, &hurt, 1);

            assert!(
                matches!(
                    brain,
                    Brain::Engage {
                        next: 2,
                        held: None,
                        ..
                    }
                ),
                "from {start:?}: {brain:?}"
            );
        }
    }

    #[test]
    fn a_hurt_enemy_with_an_offer_runs_for_cover_even_if_the_shooter_is_behind() {
        let behind = bearing(PI * 0.8, 10.0);
        let hurt = hurt_by(behind, Some(far_offer()), Some(behind));
        let starts = [Brain::Patrol { next: 2 }, chasing(bearing(0.3, 12.0), 3)];

        for start in starts {
            let mut brain = start;

            let intents = think_turning(&mut brain, hurt, Some(behind), 4);

            assert!(
                matches!(brain, Brain::Cover { next: 2, offer, .. } if offer == far_offer()),
                "from {start:?}: {brain:?}"
            );
            for intent in &intents {
                assert!(intent.forward > 0.0, "from {start:?}: {intent:?}");
                assert!(!intent.fire, "from {start:?}: {intent:?}");
            }
        }
    }

    #[test]
    fn cover_and_chase_give_up_at_their_tick_limit() {
        let tuning = tuning();
        let target = bearing(0.0, 20.0);
        let visible = at_origin(0.0, Some(target));
        let blind = at_origin(0.0, None);
        let cover_limit = tuning.cover_ticks as usize;
        let chase_limit = tuning.chase_ticks as usize;

        let mut running = Brain::Patrol { next: 2 };
        think_n(
            &mut running,
            &hurt_by(target, Some(far_offer()), Some(target)),
            1,
        );
        think_n(&mut running, &visible, cover_limit - 1);
        let before_cover_limit = running;
        think_n(&mut running, &visible, 1);

        let mut chasing = Brain::Patrol { next: 2 };
        think_n(&mut chasing, &visible, 1);
        think_n(&mut chasing, &blind, 1);
        think_n(&mut chasing, &blind, chase_limit - 1);
        let before_chase_limit = chasing;
        think_n(&mut chasing, &blind, 1);

        assert!(
            matches!(before_cover_limit, Brain::Cover { .. }),
            "{before_cover_limit:?}"
        );
        assert!(
            matches!(running, Brain::Engage { held: None, .. }),
            "{running:?}"
        );
        assert!(
            matches!(before_chase_limit, Brain::Chase { .. }),
            "{before_chase_limit:?}"
        );
        assert_eq!(chasing, Brain::Patrol { next: 2 });
    }

    #[test]
    fn the_cover_claim_is_held_from_running_to_cover_until_it_is_left() {
        let spot = offer_at(3, Vec3::new(0.0, 0.0, 10.0));
        let target = bearing(0.0, 20.0);
        let mut brain = Brain::Patrol { next: 2 };
        let claim_after = |brain: &mut Brain, percept: Percept| {
            think_n(brain, &percept, 1);
            brain.claim()
        };

        let patrolling = brain.claim();
        let hurt = claim_after(&mut brain, hurt_by(target, Some(spot), Some(target)));
        let running = claim_after(&mut brain, at(Vec3::ZERO, Some(target)));
        let arrived = claim_after(&mut brain, at(Vec3::new(0.0, 0.0, 9.0), Some(target)));
        let holding = claim_after(&mut brain, at(Vec3::new(0.0, 0.0, 9.0), Some(target)));
        let lost_sight = claim_after(&mut brain, at(Vec3::new(0.0, 0.0, 9.0), None));

        assert_eq!(patrolling, None);
        assert_eq!(hurt, Some(3));
        assert_eq!(running, Some(3));
        assert_eq!(arrived, Some(3));
        assert_eq!(holding, Some(3));
        assert_eq!(lost_sight, None);
        assert!(matches!(brain, Brain::Chase { .. }), "{brain:?}");
        assert_eq!(engaged(None).claim(), None);
        assert_eq!(chasing(Vec3::ZERO, 0).claim(), None);
    }
}
