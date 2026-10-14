//! Wording the player reads for rifle state.

use moho_fps_rules::MagazineLevel;

/// The words that tell the player how full a magazine is, without a count.
pub fn magazine_level_text(_level: MagazineLevel) -> &'static str {
    unimplemented!("map each magazine level to its wording")
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
