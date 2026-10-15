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
            seed: 0,
        }
    }
}

/// The enemy's state machine.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Brain {
    /// Walking the route towards waypoint `next`.
    Patrol { next: usize },
    /// Chasing a seen target; `next` is the route position to resume from.
    Engage {
        next: usize,
        /// Consecutive ticks the target has been seen, this one included.
        seen_ticks: u32,
        /// Ticks until the enemy may fire again.
        cooldown: u32,
    },
}

impl Brain {
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
        let _ = (route, percept, tuning, dt, PI);
        todo!()
    }
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
        };

        let intent = brain.think(&route, &behind, &tuning, DT);

        assert!(matches!(brain, Brain::Engage { next: 1, .. }), "{brain:?}");
        assert!(intent.yaw_delta > 0.0, "it should turn to face: {intent:?}");
        assert!(intent.yaw_delta <= tuning.turn_rate * DT + 1e-4);
    }

    #[test]
    fn losing_sight_returns_to_patrol_and_keeps_the_route_position() {
        let route = square();
        let tuning = tuning();
        let mut brain = Brain::Patrol { next: 2 };

        brain.think(
            &route,
            &at_origin(0.0, Some(bearing(0.0, 10.0))),
            &tuning,
            DT,
        );
        assert!(matches!(brain, Brain::Engage { .. }), "{brain:?}");
        brain.think(&route, &at_origin(0.0, None), &tuning, DT);

        assert_eq!(brain, Brain::Patrol { next: 2 });
    }
}
