//! The player's stat layer: sources declare contributions, one resolution turns them into a
//! value, and the same contributions explain it.

use crate::item::{ItemDef, ItemId};
use crate::load::LoadThresholds;
use crate::loadout::Loadout;
use serde::Deserialize;
use thiserror::Error;

/// A number the player's capability is built from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stat {
    /// How many kilograms the player carries before load bites.
    CarryCapacity,
    /// A factor on the gait speeds; 1.0 is unmodified.
    MovementSpeed,
    /// The sprint pool, in seconds.
    Stamina,
}

/// What a contribution does to a stat.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Verb {
    /// Adds to the stat.
    Add(f32),
    /// Multiplies the stat; scales multiply each other.
    Scale(f32),
    /// Caps the stat; the lowest cap wins.
    Cap(f32),
}

/// Who declared a contribution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceLabel {
    /// A worn item.
    Equipment(ItemId),
    /// What the player carries against what they can.
    Load,
    /// How the player is holding themselves, such as a lean.
    Stance,
}

/// One source's change to one stat.
#[derive(Debug, Clone, PartialEq)]
pub struct Contribution {
    /// Who declared it.
    pub source: SourceLabel,
    /// The stat it changes.
    pub stat: Stat,
    /// How it changes it.
    pub verb: Verb,
}

/// An item id that names no item definition.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("unknown item \"{id}\"")]
pub struct UnknownItem {
    /// The id with no definition.
    pub id: ItemId,
}

/// The player's attribute sheet and the baselines it gives every stat, from data.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(try_from = "RawPlayerDef")]
pub struct PlayerDef {
    /// The attribute carry capacity grows with.
    pub strength: f32,
    /// Kilograms of carry capacity each point of strength gives.
    pub carry_capacity_per_strength: f32,
    /// The unmodified movement speed factor.
    pub movement_speed: f32,
    /// The sprint pool, in seconds.
    pub stamina_pool: f32,
    /// The movement speed factor load applies once it passes the slowed threshold.
    pub slowed_scale: f32,
    /// Where load, as a fraction of carry capacity, changes the player's state.
    pub thresholds: LoadThresholds,
}

/// Why a player sheet was refused.
#[derive(Debug, Clone, PartialEq, Error)]
#[non_exhaustive]
pub enum PlayerDefError {
    /// A number that must be finite and at least 0 is not.
    #[error("{field} must be a finite number of 0 or more, got {value}")]
    Value {
        /// The key.
        field: &'static str,
        /// The value given.
        value: f32,
    },
    /// The slowed scale is above 1, which would speed the player up.
    #[error("slowed_scale must be at most 1, got {0}")]
    SlowedScale(f32),
}

/// Unvalidated wire form of a [`PlayerDef`].
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPlayerDef {
    strength: f32,
    carry_capacity_per_strength: f32,
    movement_speed: f32,
    stamina_pool: f32,
    slowed_scale: f32,
    thresholds: LoadThresholds,
}

impl TryFrom<RawPlayerDef> for PlayerDef {
    type Error = PlayerDefError;

    fn try_from(raw: RawPlayerDef) -> Result<Self, Self::Error> {
        let checks = [
            ("strength", raw.strength),
            (
                "carry_capacity_per_strength",
                raw.carry_capacity_per_strength,
            ),
            ("movement_speed", raw.movement_speed),
            ("stamina_pool", raw.stamina_pool),
            ("slowed_scale", raw.slowed_scale),
        ];
        for (field, value) in checks {
            if !(value.is_finite() && value >= 0.0) {
                return Err(PlayerDefError::Value { field, value });
            }
        }
        if raw.slowed_scale > 1.0 {
            return Err(PlayerDefError::SlowedScale(raw.slowed_scale));
        }
        Ok(Self {
            strength: raw.strength,
            carry_capacity_per_strength: raw.carry_capacity_per_strength,
            movement_speed: raw.movement_speed,
            stamina_pool: raw.stamina_pool,
            slowed_scale: raw.slowed_scale,
            thresholds: raw.thresholds,
        })
    }
}

impl PlayerDef {
    /// The value `stat` starts from before any source contributes.
    #[must_use]
    pub fn baseline(&self, stat: Stat) -> f32 {
        match stat {
            Stat::CarryCapacity => self.strength * self.carry_capacity_per_strength,
            Stat::MovementSpeed => self.movement_speed,
            Stat::Stamina => self.stamina_pool,
        }
    }
}

/// The values of `contributions` to `stat` that `pick` selects, smallest first, so that
/// summing or multiplying them gives the same float whatever order the sources came in.
fn sorted_values(
    stat: Stat,
    contributions: &[Contribution],
    pick: impl Fn(Verb) -> Option<f32>,
) -> Vec<f32> {
    let mut values: Vec<f32> = contributions
        .iter()
        .filter(|c| c.stat == stat)
        .filter_map(|c| pick(c.verb))
        .collect();
    values.sort_by(f32::total_cmp);
    values
}

/// `stat` from `baseline`: every add, then every scale, then the lowest cap, then `floor`.
/// Contributions to other stats are ignored; the order of `contributions` never matters.
#[must_use]
pub fn resolve(stat: Stat, baseline: f32, floor: f32, contributions: &[Contribution]) -> f32 {
    let adds = sorted_values(stat, contributions, |verb| match verb {
        Verb::Add(x) => Some(x),
        _ => None,
    });
    let scales = sorted_values(stat, contributions, |verb| match verb {
        Verb::Scale(x) => Some(x),
        _ => None,
    });
    let caps = sorted_values(stat, contributions, |verb| match verb {
        Verb::Cap(x) => Some(x),
        _ => None,
    });
    let added = adds.into_iter().fold(baseline, |sum, x| sum + x);
    let scaled = scales.into_iter().fold(added, |product, x| product * x);
    let capped = caps.into_iter().fold(scaled, f32::min);
    capped.max(floor)
}

/// The contributions to `stat` in the order [`resolve`] applies them: adds, scales, caps.
#[must_use]
pub fn explain(stat: Stat, contributions: &[Contribution]) -> Vec<Contribution> {
    let rank = |verb: Verb| match verb {
        Verb::Add(_) => 0,
        Verb::Scale(_) => 1,
        Verb::Cap(_) => 2,
    };
    let mut lines: Vec<Contribution> = contributions
        .iter()
        .filter(|c| c.stat == stat)
        .cloned()
        .collect();
    lines.sort_by_key(|c| rank(c.verb));
    lines
}

/// What the worn rig and backpack of `loadout` contribute through the `stats` of their
/// definitions, looked up through `lookup`.
///
/// # Errors
///
/// [`UnknownItem`] when a worn item has no definition.
pub fn equipment_contributions<'a>(
    loadout: &Loadout,
    lookup: impl Fn(&ItemId) -> Option<&'a ItemDef>,
) -> Result<Vec<Contribution>, UnknownItem> {
    let worn = std::iter::once(loadout.rig().item()).chain(loadout.backpack());
    let mut contributions = Vec::new();
    for id in worn {
        let def = lookup(id).ok_or_else(|| UnknownItem { id: id.clone() })?;
        contributions.extend(def.stats.iter().map(|entry| Contribution {
            source: SourceLabel::Equipment(id.clone()),
            stat: entry.stat,
            verb: entry.verb,
        }));
    }
    Ok(contributions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::ItemId;
    use crate::load::LoadThresholds;
    use proptest::prelude::*;

    fn contribution(source: SourceLabel, stat: Stat, verb: Verb) -> Contribution {
        Contribution { source, stat, verb }
    }

    fn speed(source: SourceLabel, verb: Verb) -> Contribution {
        contribution(source, Stat::MovementSpeed, verb)
    }

    fn gear(id: &str) -> SourceLabel {
        SourceLabel::Equipment(ItemId::new(id))
    }

    fn player() -> PlayerDef {
        PlayerDef {
            strength: 10.0,
            carry_capacity_per_strength: 3.0,
            movement_speed: 1.0,
            stamina_pool: 5.0,
            slowed_scale: 0.7,
            thresholds: LoadThresholds {
                slowed: 0.6,
                no_sprint: 0.8,
                cannot_move: 1.0,
            },
        }
    }

    #[test]
    fn baselines_come_from_the_attribute_sheet() {
        let player = PlayerDef {
            strength: 8.0,
            carry_capacity_per_strength: 2.5,
            movement_speed: 1.2,
            stamina_pool: 7.0,
            ..player()
        };

        assert_eq!(player.baseline(Stat::CarryCapacity), 20.0);
        assert_eq!(player.baseline(Stat::MovementSpeed), 1.2);
        assert_eq!(player.baseline(Stat::Stamina), 7.0);
    }

    #[test]
    fn a_stat_with_no_contributions_resolves_to_its_baseline() {
        let resolved = resolve(Stat::CarryCapacity, 30.0, 0.0, &[]);

        assert_eq!(resolved, 30.0);
    }

    #[test]
    fn adds_apply_before_scales_and_scales_before_caps() {
        let contributions = [
            speed(SourceLabel::Load, Verb::Cap(28.0)),
            speed(SourceLabel::Stance, Verb::Scale(2.0)),
            speed(gear("pack"), Verb::Add(5.0)),
        ];

        let resolved = resolve(Stat::MovementSpeed, 10.0, 0.0, &contributions);

        assert_eq!(
            resolved, 28.0,
            "(10 + 5) * 2 = 30, capped at 28; scaling first gives 25"
        );
    }

    #[test]
    fn a_scale_multiplies_the_sum_of_the_baseline_and_every_add() {
        let contributions = [
            speed(gear("a"), Verb::Add(5.0)),
            speed(gear("b"), Verb::Add(5.0)),
            speed(SourceLabel::Stance, Verb::Scale(1.5)),
        ];

        let resolved = resolve(Stat::MovementSpeed, 10.0, 0.0, &contributions);

        assert_eq!(resolved, 30.0);
    }

    #[test]
    fn two_half_scales_leave_a_quarter() {
        let contributions = [
            speed(SourceLabel::Load, Verb::Scale(0.5)),
            speed(SourceLabel::Stance, Verb::Scale(0.5)),
        ];

        let resolved = resolve(Stat::MovementSpeed, 1.0, 0.0, &contributions);

        assert_eq!(resolved, 0.25);
    }

    #[test]
    fn two_caps_resolve_to_the_lower() {
        let contributions = [
            speed(SourceLabel::Load, Verb::Cap(0.8)),
            speed(gear("boots"), Verb::Cap(0.3)),
            speed(SourceLabel::Stance, Verb::Cap(0.6)),
        ];

        let resolved = resolve(Stat::MovementSpeed, 1.0, 0.0, &contributions);

        assert_eq!(resolved, 0.3);
    }

    #[test]
    fn a_cap_above_the_value_changes_nothing() {
        let contributions = [speed(SourceLabel::Load, Verb::Cap(9.0))];

        let resolved = resolve(Stat::MovementSpeed, 1.0, 0.0, &contributions);

        assert_eq!(resolved, 1.0);
    }

    #[test]
    fn a_cap_of_zero_stops_the_stat() {
        let contributions = [
            speed(SourceLabel::Load, Verb::Cap(0.0)),
            speed(SourceLabel::Stance, Verb::Scale(0.5)),
        ];

        let resolved = resolve(Stat::MovementSpeed, 1.0, 0.0, &contributions);

        assert_eq!(resolved, 0.0);
    }

    #[test]
    fn the_floor_holds_against_a_subtraction_below_it() {
        let contributions = [speed(gear("anchor"), Verb::Add(-50.0))];

        let resolved = resolve(Stat::MovementSpeed, 1.0, 0.25, &contributions);

        assert_eq!(resolved, 0.25);
    }

    #[test]
    fn contributions_to_other_stats_are_ignored() {
        let contributions = [
            contribution(gear("pack"), Stat::CarryCapacity, Verb::Add(10.0)),
            contribution(SourceLabel::Load, Stat::Stamina, Verb::Cap(0.0)),
        ];

        let resolved = resolve(Stat::MovementSpeed, 1.0, 0.0, &contributions);

        assert_eq!(resolved, 1.0);
    }

    #[test]
    fn explain_lists_load_with_its_scale() {
        let contributions = [
            contribution(gear("pack"), Stat::CarryCapacity, Verb::Add(10.0)),
            speed(SourceLabel::Load, Verb::Scale(0.7)),
        ];

        let lines = explain(Stat::MovementSpeed, &contributions);

        assert_eq!(lines, [speed(SourceLabel::Load, Verb::Scale(0.7))]);
    }

    #[test]
    fn explain_lists_adds_then_scales_then_caps_whatever_the_input_order() {
        let cap = speed(SourceLabel::Load, Verb::Cap(0.5));
        let scale = speed(SourceLabel::Stance, Verb::Scale(0.5));
        let add = speed(gear("boots"), Verb::Add(0.1));

        let lines = explain(
            Stat::MovementSpeed,
            &[cap.clone(), scale.clone(), add.clone()],
        );

        assert_eq!(lines, [add, scale, cap]);
    }

    /// A contribution to movement speed with a value exactly representable in `f32`, so the
    /// order of arithmetic cannot show as rounding.
    fn exact_contribution() -> impl Strategy<Value = Contribution> {
        let source = prop_oneof![
            Just(SourceLabel::Load),
            Just(SourceLabel::Stance),
            Just(gear("item")),
        ];
        let verb = prop_oneof![
            (-40i16..40).prop_map(|n| Verb::Add(f32::from(n) * 0.5)),
            prop::sample::select(vec![0.0, 0.25, 0.5, 1.0, 1.5, 2.0]).prop_map(Verb::Scale),
            (0i16..40).prop_map(|n| Verb::Cap(f32::from(n) * 0.5)),
        ];
        (source, verb).prop_map(|(source, verb)| speed(source, verb))
    }

    proptest! {
        #[test]
        fn resolution_ignores_contribution_order(
            contributions in prop::collection::vec(exact_contribution(), 0..8),
            baseline in 0i16..40,
        ) {
            let baseline = f32::from(baseline) * 0.5;
            let mut reversed = contributions.clone();
            reversed.reverse();
            let mut rotated = contributions.clone();
            if !rotated.is_empty() {
                rotated.rotate_left(1);
            }

            let forward = resolve(Stat::MovementSpeed, baseline, 0.0, &contributions);

            prop_assert_eq!(forward, resolve(Stat::MovementSpeed, baseline, 0.0, &reversed));
            prop_assert_eq!(forward, resolve(Stat::MovementSpeed, baseline, 0.0, &rotated));
        }

        #[test]
        fn resolved_value_is_never_below_the_floor(
            contributions in prop::collection::vec(exact_contribution(), 0..8),
            baseline in -20i16..40,
            floor in 0i16..10,
        ) {
            let (baseline, floor) = (f32::from(baseline) * 0.5, f32::from(floor) * 0.5);

            let resolved = resolve(Stat::MovementSpeed, baseline, floor, &contributions);

            prop_assert!(resolved >= floor, "{resolved} is below the floor {floor}");
        }
    }
}
