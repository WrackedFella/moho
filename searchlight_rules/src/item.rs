#[cfg(test)]
mod tests {
    use super::*;
    use crate::weapon::{FireBehaviour, RifleDef};
    use serde_json::{Value, json};
    use std::num::NonZeroU32;

    fn parse(value: Value) -> Result<ItemDef, serde_json::Error> {
        serde_json::from_value(value)
    }

    fn weapon_section() -> Value {
        json!({
            "well": "rifle",
            "damage": 30,
            "fire": "semi_auto",
            "cooldown_ticks": 6,
            "reload_ticks": 120,
            "charge_ticks": 30,
            "range": 100.0
        })
    }

    fn error_text(value: Value) -> String {
        parse(value)
            .expect_err("the definition should be refused")
            .to_string()
            .to_lowercase()
    }

    #[test]
    fn a_tool_loads_with_the_tool_class_and_no_section() {
        let item = parse(json!({"class": "tool", "size": [1, 3], "condition": 100}))
            .expect("a tool needs no section");

        assert_eq!(item.class, ItemClass::Tool);
        assert_eq!(
            item.size,
            Size {
                width: 1,
                height: 3
            }
        );
        assert_eq!(item.lifecycle, Lifecycle::Condition(100));
        assert!(matches!(item.kind, ItemKind::Plain), "{:?}", item.kind);
    }

    #[test]
    fn an_item_with_condition_and_uses_is_refused() {
        let text = error_text(json!({
            "class": "tool", "size": [1, 1], "condition": 50, "uses": 3
        }));

        assert!(text.contains("condition"), "{text}");
        assert!(text.contains("uses"), "{text}");
    }

    #[test]
    fn condition_alone_and_uses_alone_are_each_accepted() {
        let worn = parse(json!({"class": "tool", "size": [1, 1], "condition": 0}))
            .expect("condition 0 is a valid, fully worn item");
        let limited = parse(json!({"class": "consumable", "size": [1, 1], "uses": 1}))
            .expect("one use is valid");
        let plain = parse(json!({"class": "tool", "size": [1, 1]}))
            .expect("neither condition nor uses is valid");

        assert_eq!(worn.lifecycle, Lifecycle::Condition(0));
        assert_eq!(limited.lifecycle, Lifecycle::Uses(NonZeroU32::MIN));
        assert_eq!(plain.lifecycle, Lifecycle::Unchanging);
    }

    #[test]
    fn a_condition_of_100_is_accepted_and_101_is_refused() {
        let top = parse(json!({"class": "tool", "size": [1, 1], "condition": 100}));
        let text = error_text(json!({"class": "tool", "size": [1, 1], "condition": 101}));

        assert_eq!(
            top.expect("100 is the top").lifecycle,
            Lifecycle::Condition(100)
        );
        assert!(text.contains("100"), "{text}");
    }

    #[test]
    fn zero_uses_are_refused() {
        let result = parse(json!({"class": "consumable", "size": [1, 1], "uses": 0}));

        assert!(result.is_err(), "{result:?}");
    }

    #[test]
    fn a_zero_width_or_height_is_refused() {
        for size in [json!([0, 1]), json!([1, 0]), json!([0, 0])] {
            let text = error_text(json!({"class": "tool", "size": size}));

            assert!(text.contains("size"), "{size}: {text}");
        }
    }

    #[test]
    fn a_missing_class_or_size_is_refused() {
        assert!(parse(json!({"size": [1, 1]})).is_err());
        assert!(parse(json!({"class": "tool"})).is_err());
    }

    #[test]
    fn an_unknown_class_or_field_is_refused() {
        let class = error_text(json!({"class": "ammo", "size": [1, 1]}));
        let field = error_text(json!({"class": "tool", "size": [1, 1], "wieght": 2}));

        assert!(class.contains("ammo"), "{class}");
        assert!(field.contains("wieght"), "{field}");
    }

    #[test]
    fn a_weapon_section_loads_the_rifle_definition_with_its_well() {
        let item = parse(json!({
            "class": "weapon", "size": [2, 5], "condition": 100, "weapon": weapon_section()
        }))
        .expect("a weapon with a weapon section loads");

        assert_eq!(item.class, ItemClass::Weapon);
        let ItemKind::Weapon(rifle) = item.kind else {
            panic!("expected a weapon kind, got {:?}", item.kind);
        };
        assert_eq!(
            rifle,
            RifleDef {
                well: MagazineWell::new("rifle"),
                damage: 30,
                fire: FireBehaviour::SemiAuto,
                cooldown_ticks: 6,
                reload_ticks: 120,
                charge_ticks: 30,
                range: 100.0,
            }
        );
    }

    #[test]
    fn a_magazine_section_loads_well_caliber_and_capacity() {
        let item = parse(json!({
            "class": "gear", "size": [1, 2],
            "magazine": {"well": "rifle", "caliber": "5.56x45", "capacity": 30}
        }))
        .expect("gear with a magazine section loads");

        let ItemKind::Magazine(spec) = item.kind else {
            panic!("expected a magazine kind, got {:?}", item.kind);
        };
        assert_eq!(
            spec,
            MagazineSpec {
                well: MagazineWell::new("rifle"),
                caliber: Caliber::new("5.56x45"),
                capacity: 30,
            }
        );
    }

    #[test]
    fn a_round_section_loads_its_caliber() {
        let item = parse(json!({
            "class": "consumable", "size": [1, 1], "round": {"caliber": "5.56x45"}
        }))
        .expect("a consumable with a round section loads");

        let ItemKind::Round(spec) = item.kind else {
            panic!("expected a round kind, got {:?}", item.kind);
        };
        assert_eq!(spec.caliber, Caliber::new("5.56x45"));
    }

    #[test]
    fn a_rig_section_loads_its_magazine_slots() {
        let item = parse(json!({
            "class": "gear", "size": [3, 2], "rig": {"magazine_slots": 3}
        }))
        .expect("gear with a rig section loads");

        let ItemKind::Rig(spec) = item.kind else {
            panic!("expected a rig kind, got {:?}", item.kind);
        };
        assert_eq!(spec, RigSpec { magazine_slots: 3 });
    }

    #[test]
    fn a_section_that_disagrees_with_the_class_is_refused() {
        let cases = [
            ("gear", "weapon", weapon_section()),
            (
                "weapon",
                "magazine",
                json!({"well": "w", "caliber": "c", "capacity": 1}),
            ),
            ("gear", "round", json!({"caliber": "c"})),
            ("consumable", "rig", json!({"magazine_slots": 1})),
            ("tool", "rig", json!({"magazine_slots": 1})),
        ];
        for (class, section, body) in cases {
            let mut value = json!({"class": class, "size": [1, 1]});
            value[section] = body;

            let text = error_text(value);

            assert!(text.contains(section), "{class}/{section}: {text}");
            assert!(text.contains(class), "{class}/{section}: {text}");
        }
    }

    #[test]
    fn two_sections_on_one_item_are_refused() {
        let result = parse(json!({
            "class": "gear", "size": [1, 1],
            "magazine": {"well": "rifle", "caliber": "5.56x45", "capacity": 30},
            "rig": {"magazine_slots": 3}
        }));

        assert!(result.is_err(), "{result:?}");
    }

    #[test]
    fn an_unknown_section_key_inside_a_section_is_refused() {
        let text = error_text(json!({
            "class": "gear", "size": [1, 1], "rig": {"magazine_slots": 3, "slotz": 1}
        }));

        assert!(text.contains("slotz"), "{text}");
    }

    #[test]
    fn a_class_without_a_section_is_plain() {
        for class in ["weapon", "consumable", "tool", "gear"] {
            let item = parse(json!({"class": class, "size": [1, 1]}))
                .unwrap_or_else(|e| panic!("{class} without a section should load: {e}"));

            assert!(matches!(item.kind, ItemKind::Plain), "{class}");
        }
    }

    #[test]
    fn ids_are_transparent_strings_that_display_as_themselves() {
        let id: ItemId = serde_json::from_value(json!("mag_rifle")).expect("a bare string");

        assert_eq!(id, ItemId::new("mag_rifle"));
        assert_eq!(id.as_str(), "mag_rifle");
        assert_eq!(id.to_string(), "mag_rifle");
        assert_eq!(serde_json::to_value(&id).unwrap(), json!("mag_rifle"));
        assert_eq!(Caliber::new("9mm").to_string(), "9mm");
        assert_eq!(MagazineWell::new("rifle").as_str(), "rifle");
    }
}
