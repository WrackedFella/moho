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
#[serde(deny_unknown_fields)]
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

impl PlayerDef {
    /// The value `stat` starts from before any source contributes.
    #[must_use]
    pub fn baseline(&self, _stat: Stat) -> f32 {
        todo!("the baseline of each stat from the attribute sheet")
    }
}

/// `stat` from `baseline`: every add, then every scale, then the lowest cap, then `floor`.
/// Contributions to other stats are ignored; the order of `contributions` never matters.
#[must_use]
pub fn resolve(_stat: Stat, _baseline: f32, _floor: f32, _contributions: &[Contribution]) -> f32 {
    todo!("resolve a stat from its contributions")
}

/// The contributions to `stat` in the order [`resolve`] applies them: adds, scales, caps.
#[must_use]
pub fn explain(_stat: Stat, _contributions: &[Contribution]) -> Vec<Contribution> {
    todo!("list the contributions to a stat in resolution order")
}

/// What the worn rig and backpack of `loadout` contribute through the `stats` of their
/// definitions, looked up through `lookup`.
///
/// # Errors
///
/// [`UnknownItem`] when a worn item has no definition.
pub fn equipment_contributions<'a>(
    _loadout: &Loadout,
    _lookup: impl Fn(&ItemId) -> Option<&'a ItemDef>,
) -> Result<Vec<Contribution>, UnknownItem> {
    todo!("one contribution per stat entry of each worn item")
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
