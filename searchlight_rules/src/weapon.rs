//! The rifle's tunable numbers, as a definition a content file can supply.

use crate::item::MagazineWell;
use serde::Deserialize;
use thiserror::Error;

/// Why a muzzle velocity factor was refused.
#[derive(Debug, Clone, Copy, PartialEq, Error)]
#[non_exhaustive]
pub enum VelocityFactorError {
    /// Not a number, infinite, zero or negative.
    #[error("muzzle_velocity_factor must be finite and above 0, got {0}")]
    NotPositive(f32),
}

/// How much a barrel scales the speed its rounds leave at; finite and above 0.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(try_from = "f32")]
pub struct VelocityFactor(f32);

impl Default for VelocityFactor {
    /// A barrel that leaves the round's own velocity.
    fn default() -> Self {
        Self(1.0)
    }
}

impl VelocityFactor {
    /// The factor.
    #[must_use]
    pub fn get(self) -> f32 {
        self.0
    }
}

impl TryFrom<f32> for VelocityFactor {
    type Error = VelocityFactorError;

    fn try_from(_factor: f32) -> Result<Self, Self::Error> {
        todo!("FPS-F15-08: refuse non-finite and non-positive factors")
    }
}

/// How a trigger pull becomes shots, named by id in definition files.
///
/// The snake_case id is the file's contract, so code may resolve ids to behaviour
/// however it likes without changing the schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FireBehaviour {
    /// One shot per pull.
    SemiAuto,
}

/// The rifle's definition. A misspelt field is refused, not ignored.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RifleDef {
    /// The magazine well the rifle takes.
    pub well: MagazineWell,
    /// How a pull becomes shots.
    pub fire: FireBehaviour,
    /// Ticks between one shot and the next allowed pull.
    pub cooldown_ticks: u32,
    /// Ticks a tactical reload (round in the chamber) takes.
    pub reload_ticks: u32,
    /// Ticks added to `reload_ticks` when the chamber is empty, to charge the rifle.
    pub charge_ticks: u32,
    /// How much the barrel scales the speed rounds leave at; 1.0 when absent.
    #[serde(default)]
    pub muzzle_velocity_factor: VelocityFactor,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::MagazineWell;
    use serde_json::json;

    fn valid() -> serde_json::Value {
        json!({
            "well": "rifle",
            "fire": "semi_auto",
            "cooldown_ticks": 6,
            "reload_ticks": 120,
            "charge_ticks": 30
        })
    }

    #[test]
    fn a_valid_rifle_deserializes_with_semi_auto_fire() {
        let rifle: RifleDef = serde_json::from_value(valid()).unwrap();

        assert_eq!(
            rifle,
            RifleDef {
                well: MagazineWell::new("rifle"),
                fire: FireBehaviour::SemiAuto,
                cooldown_ticks: 6,
                reload_ticks: 120,
                charge_ticks: 30,
                muzzle_velocity_factor: VelocityFactor::default(),
            }
        );
    }

    fn with_factor(factor: serde_json::Value) -> serde_json::Value {
        let mut value = valid();
        value["muzzle_velocity_factor"] = factor;
        value
    }

    #[test]
    fn a_rifle_with_no_muzzle_velocity_factor_has_a_factor_of_1() {
        let rifle: RifleDef = serde_json::from_value(valid()).unwrap();

        assert_eq!(rifle.muzzle_velocity_factor.get(), 1.0);
    }

    #[test]
    fn a_rifle_with_a_muzzle_velocity_factor_keeps_it() {
        let short: RifleDef = serde_json::from_value(with_factor(json!(0.8))).unwrap();
        let long: RifleDef = serde_json::from_value(with_factor(json!(1.1))).unwrap();

        assert_eq!(short.muzzle_velocity_factor.get(), 0.8);
        assert_eq!(long.muzzle_velocity_factor.get(), 1.1);
    }

    #[test]
    fn a_rifle_with_a_zero_or_negative_muzzle_velocity_factor_is_refused_naming_the_field() {
        for factor in [json!(0), json!(0.0), json!(-0.8), json!(-1)] {
            let error = serde_json::from_value::<RifleDef>(with_factor(factor.clone()))
                .expect_err("a factor of zero or less is refused");

            assert!(
                error.to_string().contains("muzzle_velocity_factor"),
                "{factor}: {error}"
            );
        }
    }

    #[test]
    fn a_velocity_factor_that_is_not_finite_and_above_zero_is_refused_naming_the_field() {
        let refused = [
            0.0,
            -0.0,
            -f32::MIN_POSITIVE,
            -1.0,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ];
        for factor in refused {
            let error = VelocityFactor::try_from(factor).expect_err("not a valid factor");

            assert!(
                error.to_string().contains("muzzle_velocity_factor"),
                "{factor}: {error}"
            );
        }
    }

    #[test]
    fn a_velocity_factor_accepts_any_finite_positive_value() {
        for factor in [f32::MIN_POSITIVE, 0.8, 1.0, 1.1, 250.0] {
            let accepted = VelocityFactor::try_from(factor).expect("a positive finite factor");

            assert_eq!(accepted.get(), factor);
        }
    }

    #[test]
    fn a_rifle_that_still_names_a_range_is_refused() {
        let mut value = valid();
        value["range"] = json!(100.0);

        let error = serde_json::from_value::<RifleDef>(value).unwrap_err();

        assert!(error.to_string().contains("range"), "{error}");
    }

    #[test]
    fn a_rifle_that_still_names_damage_is_refused() {
        let value = json!({
            "well": "rifle",
            "damage": 30,
            "fire": "semi_auto",
            "cooldown_ticks": 6,
            "reload_ticks": 120,
            "charge_ticks": 30
        });

        let error = serde_json::from_value::<RifleDef>(value)
            .expect_err("damage belongs to the round, not the weapon");

        assert!(error.to_string().contains("damage"), "{error}");
    }

    #[test]
    fn a_rifle_without_charge_ticks_is_refused() {
        let mut value = valid();
        value.as_object_mut().unwrap().remove("charge_ticks");

        let error = serde_json::from_value::<RifleDef>(value).unwrap_err();

        assert!(error.to_string().contains("charge_ticks"), "{error}");
    }

    #[test]
    fn a_rifle_without_a_magazine_well_is_refused() {
        let mut value = valid();
        value.as_object_mut().unwrap().remove("well");

        let error = serde_json::from_value::<RifleDef>(value).unwrap_err();

        assert!(error.to_string().contains("well"), "{error}");
    }

    #[test]
    fn a_rifle_that_still_names_a_magazine_capacity_is_refused() {
        let mut value = valid();
        value["magazine_capacity"] = json!(30);

        let error = serde_json::from_value::<RifleDef>(value).unwrap_err();

        assert!(error.to_string().contains("magazine_capacity"), "{error}");
    }

    #[test]
    fn an_unknown_fire_behaviour_is_refused() {
        let mut value = valid();
        value["fire"] = json!("burst");

        let error = serde_json::from_value::<RifleDef>(value).unwrap_err();

        assert!(error.to_string().contains("burst"), "{error}");
    }

    #[test]
    fn an_unknown_rifle_field_is_refused() {
        let mut value = valid();
        value["dammage"] = json!(20);

        let error = serde_json::from_value::<RifleDef>(value).unwrap_err();

        assert!(error.to_string().contains("dammage"), "{error}");
    }
}
