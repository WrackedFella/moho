//! Wording the player reads for rifle state.

use glam::Vec3;
use searchlight_rules::{BodyPart, MagazineLevel};

use crate::control::ControlAction;
use crate::sim::Feedback;

/// The words that tell the player how full a magazine is, without a count.
pub fn magazine_level_text(level: MagazineLevel) -> &'static str {
    match level {
        MagazineLevel::Full => "full",
        MagazineLevel::MostlyFull => "mostly full",
        MagazineLevel::AboutHalf => "about half",
        MagazineLevel::NearlyEmpty => "nearly empty",
        MagazineLevel::Empty => "empty",
    }
}

/// The name of the body part a hit reports.
pub fn part_text(part: BodyPart) -> &'static str {
    match part {
        BodyPart::Head => "head",
        BodyPart::Torso => "torso",
        BodyPart::LeftArm => "left arm",
        BodyPart::RightArm => "right arm",
        BodyPart::LeftLeg => "left leg",
        BodyPart::RightLeg => "right leg",
    }
}

/// The status line a piece of feedback puts in the window title, if it changes it.
pub fn status_text(feedback: &Feedback) -> Option<String> {
    match feedback {
        Feedback::Fired => None,
        Feedback::Dry => Some("click, the rifle is empty".to_owned()),
        Feedback::Hit { part, report, .. } => Some(format!(
            "hit the {} for {}",
            part_text(*part),
            report.damage
        )),
        Feedback::MagazineLevel(level) => {
            Some(format!("magazine is {}", magazine_level_text(*level)))
        }
        Feedback::ReloadDone => Some("reloaded".to_owned()),
        Feedback::EnemyFired { .. }
        | Feedback::PlayerHit { .. }
        | Feedback::EnemyDied { .. }
        | Feedback::PlayerDied => None,
    }
}

/// The side of the screen a hit came from, as the player faces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Front,
    Right,
    Behind,
    Left,
}

/// The screen edge to flash for a shot that began at `from`, for a player standing at
/// `player` and facing `yaw` (0 faces +Z; the player's right at yaw 0 is -X).
pub fn hit_edge(player: Vec3, yaw: f32, from: Vec3) -> Edge {
    let offset = from - player;
    let forward = offset.x * yaw.sin() + offset.z * yaw.cos();
    let right = -offset.x * yaw.cos() + offset.z * yaw.sin();
    if forward.abs() >= right.abs() {
        if forward >= 0.0 {
            Edge::Front
        } else {
            Edge::Behind
        }
    } else if right > 0.0 {
        Edge::Right
    } else {
        Edge::Left
    }
}

pub fn health_text(health: u32, max_health: u32) -> String {
    format!("HP {health}/{max_health}")
}

/// The wave and score line, such as `Wave 3  Score 200`.
pub fn arena_text(wave: u32, score: u32) -> String {
    format!("Wave {wave}  Score {score}")
}

/// The title line: `vitals` followed by the wave and score, such as `HP 70/100 | Wave 3  Score 200`.
pub fn vitals_text(vitals: &str, wave: u32, score: u32) -> String {
    format!("{vitals} | {}", arena_text(wave, score))
}

/// `vitals` with `GOD MODE` appended while god mode is on, unchanged while off.
pub fn god_mode_text(vitals: &str, god_mode: bool) -> String {
    if god_mode {
        format!("{vitals} | GOD MODE")
    } else {
        vitals.to_owned()
    }
}

/// What the dead player reads in the title, or `None` while alive.
pub fn death_text(dead: bool) -> Option<&'static str> {
    dead.then_some("Dead - press F8 to restart")
}

/// The prompt for a control in reach: the `key` that uses it and what it does, such as
/// `[F] Next wave`. A `key` of `unbound` says the action has no key.
pub fn use_prompt(_key: &str, _action: ControlAction) -> String {
    String::new()
}

/// A translucent rectangle in screen pixels, colour as straight RGBA.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Overlay {
    pub min: [f32; 2],
    pub max: [f32; 2],
    pub rgba: [u8; 4],
}

/// The tint over a `screen`-sized frame while the player is dead, or `None` while alive.
pub fn death_overlay(dead: bool, screen: [u32; 2]) -> Option<Overlay> {
    dead.then(|| Overlay {
        min: [0.0, 0.0],
        max: [screen[0] as f32, screen[1] as f32],
        rgba: [120, 0, 0, 140],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magazine_levels_read_as_the_acceptance_wording() {
        let levels = [
            (MagazineLevel::Full, "full"),
            (MagazineLevel::MostlyFull, "mostly full"),
            (MagazineLevel::AboutHalf, "about half"),
            (MagazineLevel::NearlyEmpty, "nearly empty"),
            (MagazineLevel::Empty, "empty"),
        ];

        let words: Vec<&str> = levels
            .iter()
            .map(|&(l, _)| magazine_level_text(l))
            .collect();

        let expected: Vec<&str> = levels.iter().map(|&(_, w)| w).collect();
        assert_eq!(words, expected);
    }
}

#[cfg(test)]
mod feedback_tests {
    use std::f32::consts::FRAC_PI_2;

    use super::*;

    #[test]
    fn the_dead_title_keeps_wave_and_score() {
        let alive = health_text(70, 100);
        let dead = death_text(true).expect("a dead player has a title");

        let alive_title = vitals_text(&alive, 3, 200);
        let dead_title = vitals_text(dead, 3, 200);

        assert_eq!(alive_title, "HP 70/100 | Wave 3  Score 200");
        assert_eq!(dead_title, "Dead - press F8 to restart | Wave 3  Score 200");
    }

    #[test]
    fn the_title_shows_god_mode_only_when_on() {
        let alive = vitals_text(&health_text(70, 100), 3, 200);
        let dead = vitals_text(death_text(true).expect("dead"), 3, 200);

        let off = god_mode_text(&alive, false);
        let on = god_mode_text(&alive, true);
        let on_while_dead = god_mode_text(&dead, true);

        assert_eq!(off, alive);
        assert!(on.starts_with(&alive), "{on}");
        assert!(on.ends_with("GOD MODE"), "{on}");
        assert!(on_while_dead.starts_with(&dead), "{on_while_dead}");
        assert!(on_while_dead.ends_with("GOD MODE"), "{on_while_dead}");
        assert!(!off.contains("GOD MODE"));
    }

    #[test]
    fn the_hit_edge_points_toward_the_shooter() {
        let player = Vec3::new(2.0, 1.0, -3.0);
        let at = |offset: Vec3| player + offset * 8.0;

        let front = hit_edge(player, 0.0, at(Vec3::Z));
        let right = hit_edge(player, 0.0, at(Vec3::NEG_X));
        let behind = hit_edge(player, 0.0, at(Vec3::NEG_Z));
        let left = hit_edge(player, 0.0, at(Vec3::X));

        assert_eq!(
            [front, right, behind, left],
            [Edge::Front, Edge::Right, Edge::Behind, Edge::Left]
        );
    }

    #[test]
    fn the_hit_edge_turns_with_the_player() {
        let player = Vec3::ZERO;
        let shooter = Vec3::new(8.0, 3.0, 0.0);

        let facing_it = hit_edge(player, FRAC_PI_2, shooter);
        let facing_away = hit_edge(player, -FRAC_PI_2, shooter);
        let facing_z = hit_edge(player, 0.0, shooter);
        let facing_back = hit_edge(player, std::f32::consts::PI, shooter);

        assert_eq!(
            [facing_it, facing_away, facing_z, facing_back],
            [Edge::Front, Edge::Behind, Edge::Left, Edge::Right]
        );
    }

    #[test]
    fn the_hit_edge_ignores_height_and_splits_at_the_diagonals() {
        let from = |x: f32, z: f32| Vec3::new(x, 50.0, z);

        let nearly_front = hit_edge(Vec3::ZERO, 0.0, from(-4.0, 5.0));
        let nearly_right = hit_edge(Vec3::ZERO, 0.0, from(-5.0, 4.0));
        let nearly_behind = hit_edge(Vec3::ZERO, 0.0, from(4.0, -5.0));
        let nearly_left = hit_edge(Vec3::ZERO, 0.0, from(5.0, -4.0));

        assert_eq!(
            [nearly_front, nearly_right, nearly_behind, nearly_left],
            [Edge::Front, Edge::Right, Edge::Behind, Edge::Left]
        );
    }

    #[test]
    fn the_hit_edge_mixes_both_axes_when_the_player_faces_sideways() {
        let player = Vec3::ZERO;
        let ahead_in_world_z = Vec3::new(0.0, 0.0, 5.0);
        let behind_in_world_z = Vec3::new(0.0, 0.0, -5.0);

        let right = hit_edge(player, FRAC_PI_2, ahead_in_world_z);
        let left = hit_edge(player, FRAC_PI_2, behind_in_world_z);

        assert_eq!([right, left], [Edge::Right, Edge::Left]);
    }

    #[test]
    fn the_use_prompt_names_the_key_and_next_wave() {
        let bound = use_prompt("F", ControlAction::NextWave);
        let rebound = use_prompt("Mouse Right", ControlAction::NextWave);
        let unbound = use_prompt("unbound", ControlAction::NextWave);

        assert_eq!(bound, "[F] Next wave");
        assert_eq!(rebound, "[Mouse Right] Next wave");
        assert_eq!(unbound, "[unbound] Next wave");
    }

    #[test]
    fn the_status_line_shows_health() {
        assert_eq!(health_text(70, 100), "HP 70/100");
        assert_eq!(health_text(0, 100), "HP 0/100");
        assert_eq!(health_text(100, 100), "HP 100/100");
        assert_eq!(health_text(5, 12), "HP 5/12");
    }

    #[test]
    fn the_status_line_shows_wave_and_score() {
        assert_eq!(arena_text(3, 200), "Wave 3  Score 200");
        assert_eq!(arena_text(1, 0), "Wave 1  Score 0");
        assert_eq!(arena_text(12, 4_000), "Wave 12  Score 4000");
    }

    #[test]
    fn the_death_overlay_shows_only_when_dead() {
        let screen = [800, 600];

        let alive = death_overlay(false, screen);
        let dead = death_overlay(true, screen);

        assert_eq!(alive, None);
        let dead = dead.expect("a dead player sees the overlay");
        assert_eq!((dead.min, dead.max), ([0.0, 0.0], [800.0, 600.0]));
        assert!(dead.rgba[3] > 0 && dead.rgba[3] < 255, "{dead:?}");
        assert_eq!(death_text(false), None);
        assert_eq!(death_text(true), Some("Dead - press F8 to restart"));
    }
}

#[cfg(test)]
mod part_wording_tests {
    use super::*;

    #[test]
    fn each_part_reads_as_its_own_name() {
        let parts = [
            (BodyPart::Head, "head"),
            (BodyPart::Torso, "torso"),
            (BodyPart::LeftArm, "left arm"),
            (BodyPart::RightArm, "right arm"),
            (BodyPart::LeftLeg, "left leg"),
            (BodyPart::RightLeg, "right leg"),
        ];

        let words: Vec<&str> = parts.iter().map(|&(p, _)| part_text(p)).collect();

        let expected: Vec<&str> = parts.iter().map(|&(_, w)| w).collect();
        assert_eq!(words, expected);
    }
}
