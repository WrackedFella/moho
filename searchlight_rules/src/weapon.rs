//! The rifle's tunable numbers, as a definition a content file can supply.

use serde::{Deserialize, Deserializer};

/// How a trigger pull becomes shots, named by id in definition files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FireBehaviour {
    /// One shot per pull.
    SemiAuto,
}

impl<'de> Deserialize<'de> for FireBehaviour {
    fn deserialize<D: Deserializer<'de>>(_deserializer: D) -> Result<Self, D::Error> {
        todo!()
    }
}

/// The rifle's definition.
#[derive(Debug, Clone, PartialEq)]
pub struct RifleDef {
    /// Rounds a full magazine holds.
    pub magazine_capacity: u32,
    /// Damage of one hit.
    pub damage: u32,
    /// How a pull becomes shots.
    pub fire: FireBehaviour,
    /// Ticks between one shot and the next allowed pull.
    pub cooldown_ticks: u32,
    /// Ticks a reload takes.
    pub reload_ticks: u32,
    /// How far a shot reaches, in metres.
    pub range: f32,
}

impl<'de> Deserialize<'de> for RifleDef {
    fn deserialize<D: Deserializer<'de>>(_deserializer: D) -> Result<Self, D::Error> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid() -> serde_json::Value {
        json!({
            "magazine_capacity": 30,
            "damage": 30,
            "fire": "semi_auto",
            "cooldown_ticks": 6,
            "reload_ticks": 120,
            "range": 100.0
        })
    }

    #[test]
    fn a_valid_rifle_deserializes_with_semi_auto_fire() {
        let rifle: RifleDef = serde_json::from_value(valid()).unwrap();

        assert_eq!(
            rifle,
            RifleDef {
                magazine_capacity: 30,
                damage: 30,
                fire: FireBehaviour::SemiAuto,
                cooldown_ticks: 6,
                reload_ticks: 120,
                range: 100.0,
            }
        );
    }

    #[test]
    fn a_rifle_without_magazine_capacity_is_refused() {
        let mut value = valid();
        value.as_object_mut().unwrap().remove("magazine_capacity");

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
        value["magazine_capcity"] = json!(20);

        let error = serde_json::from_value::<RifleDef>(value).unwrap_err();

        assert!(error.to_string().contains("magazine_capcity"), "{error}");
    }
}
