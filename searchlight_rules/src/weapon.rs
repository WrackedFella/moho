//! The rifle's tunable numbers, as a definition a content file can supply.

use crate::item::MagazineWell;
use serde::Deserialize;

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
    /// Damage of one hit.
    pub damage: u32,
    /// How a pull becomes shots.
    pub fire: FireBehaviour,
    /// Ticks between one shot and the next allowed pull.
    pub cooldown_ticks: u32,
    /// Ticks a tactical reload (round in the chamber) takes.
    pub reload_ticks: u32,
    /// Ticks added to `reload_ticks` when the chamber is empty, to charge the rifle.
    pub charge_ticks: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::MagazineWell;
    use serde_json::json;

    fn valid() -> serde_json::Value {
        json!({
            "well": "rifle",
            "damage": 30,
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
                damage: 30,
                fire: FireBehaviour::SemiAuto,
                cooldown_ticks: 6,
                reload_ticks: 120,
                charge_ticks: 30,
            }
        );
    }

    #[test]
    fn a_rifle_that_still_names_a_range_is_refused() {
        let mut value = valid();
        value["range"] = json!(100.0);

        let error = serde_json::from_value::<RifleDef>(value).unwrap_err();

        assert!(error.to_string().contains("range"), "{error}");
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
