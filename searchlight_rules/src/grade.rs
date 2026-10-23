//! Enemy grades: how soon an enemy fires, how far its shots stray and how often it fires,
//! as scales over a base tuning.

use std::str::FromStr;

/// How skilled an enemy is, lowest first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Grade {
    Rookie,
    Novice,
    #[default]
    Experienced,
    Veteran,
    Professional,
}

/// A grade name that is none of the five.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown grade {0:?}")]
pub struct UnknownGrade(pub String);

/// A grade's three scales, applied to a base tuning.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradeDials {
    reaction: f32,
    aim_error: f32,
    cooldown: f32,
}

impl Grade {
    /// Every grade, lowest first.
    pub const LADDER: [Grade; 5] = [
        Grade::Rookie,
        Grade::Novice,
        Grade::Experienced,
        Grade::Veteran,
        Grade::Professional,
    ];

    /// The lower-case name the grade parses from.
    pub fn name(self) -> &'static str {
        match self {
            Grade::Rookie => "rookie",
            Grade::Novice => "novice",
            Grade::Experienced => "experienced",
            Grade::Veteran => "veteran",
            Grade::Professional => "professional",
        }
    }

    /// The grade's scales; experienced is 1.0 on all three, so it is the base tuning.
    pub fn dials(self) -> GradeDials {
        let (reaction, aim_error, cooldown) = match self {
            Grade::Rookie => (2.0, 3.0, 2.0),
            Grade::Novice => (1.5, 2.0, 1.5),
            Grade::Experienced => (1.0, 1.0, 1.0),
            Grade::Veteran => (0.75, 0.6, 0.85),
            Grade::Professional => (0.5, 0.35, 0.7),
        };
        GradeDials {
            reaction,
            aim_error,
            cooldown,
        }
    }
}

impl FromStr for Grade {
    type Err = UnknownGrade;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Grade::LADDER
            .into_iter()
            .find(|grade| grade.name() == name)
            .ok_or_else(|| UnknownGrade(name.to_owned()))
    }
}

impl GradeDials {
    /// `base` ticks of reaction, scaled and rounded.
    pub fn reaction_ticks(self, base: u32) -> u32 {
        (base as f32 * self.reaction).round() as u32
    }

    /// `base` ticks between shots, scaled and rounded.
    pub fn cooldown_ticks(self, base: u32) -> u32 {
        (base as f32 * self.cooldown).round() as u32
    }

    /// `base` radians of aim error, scaled.
    pub fn aim_error(self, base: f32) -> f32 {
        base * self.aim_error
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REACTION: u32 = 30;
    const COOLDOWN: u32 = 12;
    const AIM_ERROR: f32 = 0.03;

    fn numbers(grade: Grade) -> (u32, u32, f32) {
        let dials = grade.dials();
        (
            dials.reaction_ticks(REACTION),
            dials.cooldown_ticks(COOLDOWN),
            dials.aim_error(AIM_ERROR),
        )
    }

    #[test]
    fn each_grade_is_at_least_as_quick_accurate_and_steady_as_the_one_below() {
        for pair in Grade::LADDER.windows(2) {
            let (below_reaction, below_cooldown, below_aim) = numbers(pair[0]);
            let (reaction, cooldown, aim) = numbers(pair[1]);

            assert!(reaction <= below_reaction, "{pair:?} reaction");
            assert!(cooldown <= below_cooldown, "{pair:?} cooldown");
            assert!(aim <= below_aim, "{pair:?} aim error");
        }
        let (rookie_reaction, rookie_cooldown, rookie_aim) = numbers(Grade::Rookie);
        let (pro_reaction, pro_cooldown, pro_aim) = numbers(Grade::Professional);
        assert!(pro_reaction < rookie_reaction);
        assert!(pro_cooldown < rookie_cooldown);
        assert!(pro_aim < rookie_aim);
    }

    #[test]
    fn experienced_leaves_every_base_number_unchanged() {
        let dials = Grade::Experienced.dials();

        for base in [0, 1, 7, 12, 30, 255, 100_000] {
            assert_eq!(dials.reaction_ticks(base), base);
            assert_eq!(dials.cooldown_ticks(base), base);
        }
        for base in [0.0, 0.0105, 0.03, 0.5, 2.0] {
            assert_eq!(dials.aim_error(base), base);
        }
    }

    #[test]
    fn the_ladder_scales_todays_numbers() {
        let expected = [
            (Grade::Rookie, 60, 24, 0.09),
            (Grade::Novice, 45, 18, 0.06),
            (Grade::Experienced, 30, 12, 0.03),
            (Grade::Veteran, 23, 10, 0.018),
            (Grade::Professional, 15, 8, 0.0105),
        ];

        for (grade, reaction, cooldown, aim) in expected {
            let (got_reaction, got_cooldown, got_aim) = numbers(grade);

            assert_eq!(got_reaction, reaction, "{grade:?} reaction");
            assert_eq!(got_cooldown, cooldown, "{grade:?} cooldown");
            assert!((got_aim - aim).abs() < 1e-6, "{grade:?} aim {got_aim}");
        }
    }

    #[test]
    fn every_grade_parses_from_its_name() {
        let names = ["rookie", "novice", "experienced", "veteran", "professional"];

        for (grade, name) in Grade::LADDER.into_iter().zip(names) {
            assert_eq!(grade.name(), name);
            assert_eq!(name.parse::<Grade>(), Ok(grade));
        }
    }

    #[test]
    fn an_unknown_name_is_refused() {
        for name in ["legend", "Rookie", ""] {
            assert_eq!(name.parse::<Grade>(), Err(UnknownGrade(name.to_owned())));
        }
    }
}
