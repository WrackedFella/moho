//! Wording the player reads for rifle state.

use searchlight_rules::{BodyPart, MagazineLevel};

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
    }
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
