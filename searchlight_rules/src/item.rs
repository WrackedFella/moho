//! Items defined in data: one model for weapons, magazines, rounds, rigs and tools.

use crate::ballistics::{Ballistics, MIN_SPEED, RoundStats};
use crate::weapon::RifleDef;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::num::NonZeroU32;
use thiserror::Error;

macro_rules! string_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Wraps `id` as given; whether it names anything is checked where it is looked up.
            #[must_use]
            pub fn new(id: impl Into<String>) -> Self {
                Self(id.into())
            }

            /// The id as text.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

string_id!(
    /// The id an item is defined under.
    ItemId
);
string_id!(
    /// A round's caliber; a round loads into a magazine of the same caliber.
    Caliber
);
string_id!(
    /// The kind of magazine well a weapon has; a magazine fits a weapon with the same well.
    MagazineWell
);

/// What an item is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemClass {
    /// Fired or swung.
    Weapon,
    /// Used up.
    Consumable,
    /// Held and used, not used up.
    Tool,
    /// Worn or fitted, neither used up nor held: rigs and magazines.
    Gear,
}

impl fmt::Display for ItemClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Weapon => "weapon",
            Self::Consumable => "consumable",
            Self::Tool => "tool",
            Self::Gear => "gear",
        })
    }
}

/// Footprint in inventory cells, at least 1 by 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    /// Cells across.
    pub width: u8,
    /// Cells down.
    pub height: u8,
}

/// How an item wears, if it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifecycle {
    /// Neither wears nor is used up.
    Unchanging,
    /// Condition from 0 (worn out) to 100 (new).
    Condition(u8),
    /// A number of uses left.
    Uses(NonZeroU32),
}

/// The round section of an item definition.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(try_from = "RawRoundSpec")]
pub struct RoundSpec {
    /// Caliber of the round.
    pub caliber: Caliber,
    /// Speed at the muzzle, in m/s; above [`crate::MIN_SPEED`].
    pub muzzle_velocity: f32,
    /// Fraction of its velocity the round sheds per second, in 1/s; not negative.
    pub drag: f32,
    /// Damage of a hit arriving at the muzzle velocity; at least 1.
    pub damage: u32,
}

/// Why a round section was refused.
#[derive(Debug, Clone, PartialEq, Error)]
#[non_exhaustive]
pub enum RoundSpecError {
    /// The muzzle velocity is not a finite speed above the minimum.
    #[error("muzzle_velocity must be above {MIN_SPEED} m/s, got {0}")]
    MuzzleVelocity(f32),
    /// The drag is negative or not finite.
    #[error("drag must be 0 or more, got {0}")]
    Drag(f32),
    /// The damage is 0.
    #[error("damage must be at least 1, got {0}")]
    Damage(u32),
}

/// Unvalidated wire form of a [`RoundSpec`].
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRoundSpec {
    caliber: Caliber,
    muzzle_velocity: f32,
    drag: f32,
    damage: u32,
}

impl TryFrom<RawRoundSpec> for RoundSpec {
    type Error = RoundSpecError;

    fn try_from(raw: RawRoundSpec) -> Result<Self, Self::Error> {
        if !(raw.muzzle_velocity.is_finite() && raw.muzzle_velocity > MIN_SPEED) {
            return Err(RoundSpecError::MuzzleVelocity(raw.muzzle_velocity));
        }
        if !(raw.drag.is_finite() && raw.drag >= 0.0) {
            return Err(RoundSpecError::Drag(raw.drag));
        }
        if raw.damage == 0 {
            return Err(RoundSpecError::Damage(raw.damage));
        }
        Ok(Self {
            caliber: raw.caliber,
            muzzle_velocity: raw.muzzle_velocity,
            drag: raw.drag,
            damage: raw.damage,
        })
    }
}

impl RoundSpec {
    /// How the round flies and the damage it does at the muzzle.
    #[must_use]
    pub fn stats(&self) -> RoundStats {
        RoundStats {
            ballistics: Ballistics {
                muzzle_velocity: self.muzzle_velocity,
                drag: self.drag,
            },
            damage: self.damage,
        }
    }
}

/// The magazine section of an item definition.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MagazineSpec {
    /// The well it fits.
    pub well: MagazineWell,
    /// The caliber it takes.
    pub caliber: Caliber,
    /// Rounds it holds.
    pub capacity: u32,
}

/// The rig section of an item definition.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RigSpec {
    /// Magazines the rig carries.
    pub magazine_slots: u32,
}

/// The backpack section of an item definition: marks an item that goes in the backpack slot.
/// It holds nothing.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackpackSpec {}

/// What an item does beyond being carried.
#[derive(Debug, Clone, PartialEq)]
pub enum ItemKind {
    /// A weapon.
    Weapon(RifleDef),
    /// A magazine.
    Magazine(MagazineSpec),
    /// A round.
    Round(RoundSpec),
    /// A magazine rig.
    Rig(RigSpec),
    /// A backpack.
    Backpack(BackpackSpec),
    /// Nothing beyond its class, like a tool.
    Plain,
}

impl ItemKind {
    /// The class an item with this kind's section must have; `None` for a kind with no section.
    pub(crate) fn required_class(&self) -> Option<ItemClass> {
        match self {
            Self::Weapon(_) => Some(ItemClass::Weapon),
            Self::Magazine(_) | Self::Rig(_) | Self::Backpack(_) => Some(ItemClass::Gear),
            Self::Round(_) => Some(ItemClass::Consumable),
            Self::Plain => None,
        }
    }

    /// The data key of this kind's section; `None` for a kind with no section.
    fn section_name(&self) -> Option<&'static str> {
        match self {
            Self::Weapon(_) => Some("weapon"),
            Self::Magazine(_) => Some("magazine"),
            Self::Round(_) => Some("round"),
            Self::Rig(_) => Some("rig"),
            Self::Backpack(_) => Some("backpack"),
            Self::Plain => None,
        }
    }
}

/// An item as defined in data.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(try_from = "RawItemDef")]
pub struct ItemDef {
    /// What the item is for.
    pub class: ItemClass,
    /// Inventory footprint.
    pub size: Size,
    /// Wear or uses.
    pub lifecycle: Lifecycle,
    /// Mass in kilograms; finite and at least 0.
    pub weight: f32,
    /// What the item does.
    pub kind: ItemKind,
}

/// Why an item definition was refused.
#[derive(Debug, Clone, PartialEq, Error)]
#[non_exhaustive]
pub enum ItemDefError {
    /// The weight is negative or not finite.
    #[error("weight must be 0 kg or more, got {0}")]
    Weight(f32),
    /// Both lifecycle keys were given.
    #[error("an item has either a condition or uses, not both")]
    ConditionAndUses,
    /// Condition is a percentage.
    #[error("condition {condition} is over 100")]
    ConditionOver100 {
        /// The value given.
        condition: u8,
    },
    /// A side of the size was 0.
    #[error("size must be at least 1 by 1")]
    ZeroSize,
    /// A section that the item's class does not take.
    #[error("a {section} section does not belong on an item of class {class}")]
    SectionForClass {
        /// The section given.
        section: &'static str,
        /// The item's class.
        class: ItemClass,
    },
    /// More than one section was given.
    #[error("an item takes at most one section")]
    TwoSections,
}

/// Unvalidated wire form of an [`ItemDef`].
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawItemDef {
    class: ItemClass,
    size: [u8; 2],
    condition: Option<u8>,
    uses: Option<NonZeroU32>,
    weight: f32,
    weapon: Option<RifleDef>,
    magazine: Option<MagazineSpec>,
    round: Option<RoundSpec>,
    rig: Option<RigSpec>,
    backpack: Option<BackpackSpec>,
}

impl RawItemDef {
    fn lifecycle(&self) -> Result<Lifecycle, ItemDefError> {
        match (self.condition, self.uses) {
            (Some(_), Some(_)) => Err(ItemDefError::ConditionAndUses),
            (Some(condition), None) if condition > 100 => {
                Err(ItemDefError::ConditionOver100 { condition })
            }
            (Some(condition), None) => Ok(Lifecycle::Condition(condition)),
            (None, Some(uses)) => Ok(Lifecycle::Uses(uses)),
            (None, None) => Ok(Lifecycle::Unchanging),
        }
    }
}

impl TryFrom<RawItemDef> for ItemDef {
    type Error = ItemDefError;

    fn try_from(raw: RawItemDef) -> Result<Self, Self::Error> {
        let lifecycle = raw.lifecycle()?;
        if !(raw.weight.is_finite() && raw.weight >= 0.0) {
            return Err(ItemDefError::Weight(raw.weight));
        }
        let [width, height] = raw.size;
        if width == 0 || height == 0 {
            return Err(ItemDefError::ZeroSize);
        }
        let class = raw.class;
        let mut sections = [
            raw.weapon.map(ItemKind::Weapon),
            raw.magazine.map(ItemKind::Magazine),
            raw.round.map(ItemKind::Round),
            raw.rig.map(ItemKind::Rig),
            raw.backpack.map(ItemKind::Backpack),
        ]
        .into_iter()
        .flatten();
        let kind = sections.next().unwrap_or(ItemKind::Plain);
        if sections.next().is_some() {
            return Err(ItemDefError::TwoSections);
        }
        if let (Some(needs), Some(section)) = (kind.required_class(), kind.section_name())
            && class != needs
        {
            return Err(ItemDefError::SectionForClass { section, class });
        }
        Ok(Self {
            class,
            size: Size { width, height },
            lifecycle,
            weight: raw.weight,
            kind,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weapon::{FireBehaviour, RifleDef};
    use serde_json::{Value, json};
    use std::num::NonZeroU32;

    fn parse(mut value: Value) -> Result<ItemDef, serde_json::Error> {
        if let Some(fields) = value.as_object_mut() {
            fields.entry("weight").or_insert(json!(1.0));
        }
        serde_json::from_value(value)
    }

    /// One value of a hand-built item definition, so a weight JSON cannot write (NaN,
    /// infinity) still reaches the deserializer.
    enum Field {
        Text(&'static str),
        Size([u8; 2]),
        Float(f32),
    }

    impl<'de> serde::Deserializer<'de> for Field {
        type Error = serde::de::value::Error;

        fn deserialize_any<V: serde::de::Visitor<'de>>(
            self,
            visitor: V,
        ) -> Result<V::Value, Self::Error> {
            match self {
                Field::Text(text) => visitor.visit_str(text),
                Field::Size(size) => visitor
                    .visit_seq(serde::de::value::SeqDeserializer::<_, Self::Error>::new(
                        size.into_iter(),
                    )),
                Field::Float(float) => visitor.visit_f32(float),
            }
        }

        fn deserialize_enum<V: serde::de::Visitor<'de>>(
            self,
            _name: &'static str,
            _variants: &'static [&'static str],
            visitor: V,
        ) -> Result<V::Value, Self::Error> {
            match self {
                Field::Text(text) => {
                    visitor.visit_enum(serde::de::value::StrDeserializer::<Self::Error>::new(text))
                }
                _ => Err(serde::de::Error::custom("only text names an enum variant")),
            }
        }

        serde::forward_to_deserialize_any! {
            bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes
            byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct map
            struct identifier ignored_any
        }
    }

    impl serde::de::IntoDeserializer<'_, serde::de::value::Error> for Field {
        type Deserializer = Self;

        fn into_deserializer(self) -> Self {
            self
        }
    }

    /// The error text of a tool definition weighing `weight` kg, which JSON cannot always say.
    fn weight_error_text(weight: f32) -> String {
        let fields = [
            ("class", Field::Text("tool")),
            ("size", Field::Size([1, 1])),
            ("weight", Field::Float(weight)),
        ];
        let definition = serde::de::value::MapDeserializer::<_, serde::de::value::Error>::new(
            fields.into_iter(),
        );

        ItemDef::deserialize(definition)
            .expect_err("the weight should be refused")
            .to_string()
            .to_lowercase()
    }

    fn weapon_section() -> Value {
        json!({
            "well": "rifle",
            "fire": "semi_auto",
            "cooldown_ticks": 6,
            "reload_ticks": 120,
            "charge_ticks": 30
        })
    }

    fn round_section(muzzle_velocity: Value, drag: Value) -> Value {
        json!({
            "caliber": "5.56x45", "muzzle_velocity": muzzle_velocity, "drag": drag, "damage": 30
        })
    }

    fn round_error(round: Value) -> String {
        error_text(json!({"class": "consumable", "size": [1, 1], "round": round}))
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
                fire: FireBehaviour::SemiAuto,
                cooldown_ticks: 6,
                reload_ticks: 120,
                charge_ticks: 30,
                muzzle_velocity_factor: crate::weapon::VelocityFactor::default(),
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
            "class": "consumable", "size": [1, 1],
            "round": round_section(json!(940.0), json!(1.0))
        }))
        .expect("a consumable with a round section loads");

        let ItemKind::Round(spec) = item.kind else {
            panic!("expected a round kind, got {:?}", item.kind);
        };
        assert_eq!(spec.caliber, Caliber::new("5.56x45"));
        assert_eq!(spec.muzzle_velocity, 940.0);
        assert_eq!(spec.drag, 1.0);
    }

    #[test]
    fn a_round_without_muzzle_velocity_is_refused_naming_the_field() {
        let text = round_error(json!({"caliber": "5.56x45", "drag": 1.0}));

        assert!(text.contains("muzzle_velocity"), "{text}");
    }

    #[test]
    fn a_round_without_drag_is_refused_naming_the_field() {
        let text = round_error(json!({"caliber": "5.56x45", "muzzle_velocity": 940.0}));

        assert!(text.contains("drag"), "{text}");
    }

    #[test]
    fn a_round_section_loads_its_damage() {
        for damage in [45, 1] {
            let item = parse(json!({
                "class": "consumable", "size": [1, 1],
                "round": {
                    "caliber": "5.56x45", "muzzle_velocity": 940.0, "drag": 1.0, "damage": damage
                }
            }))
            .unwrap_or_else(|e| panic!("a round with damage {damage} loads: {e}"));

            let ItemKind::Round(spec) = item.kind else {
                panic!("expected a round kind, got {:?}", item.kind);
            };
            assert_eq!(spec.damage, damage);
        }
    }

    #[test]
    fn a_round_without_damage_is_refused_naming_the_field() {
        let text = round_error(json!({
            "caliber": "5.56x45", "muzzle_velocity": 940.0, "drag": 1.0
        }));

        assert!(text.contains("damage"), "{text}");
    }

    #[test]
    fn a_round_with_damage_0_is_refused_naming_the_field() {
        let text = round_error(json!({
            "caliber": "5.56x45", "muzzle_velocity": 940.0, "drag": 1.0, "damage": 0
        }));

        assert!(text.contains("damage must be at least 1, got 0"), "{text}");
    }

    #[test]
    fn a_round_with_a_muzzle_velocity_of_0_is_refused_naming_the_field() {
        let text = round_error(round_section(json!(0.0), json!(1.0)));

        assert!(text.contains("muzzle_velocity"), "{text}");
        assert!(text.contains("above 100"), "{text}");
    }

    #[test]
    fn a_round_with_negative_drag_is_refused_naming_the_field() {
        let text = round_error(round_section(json!(940.0), json!(-0.5)));

        assert!(text.contains("drag"), "{text}");
        assert!(text.contains("-0.5"), "{text}");
    }

    #[test]
    fn a_round_at_the_minimum_speed_is_refused_naming_the_field() {
        let text = round_error(round_section(json!(100.0), json!(1.0)));

        assert!(text.contains("muzzle_velocity"), "{text}");
        let above = parse(json!({
            "class": "consumable", "size": [1, 1],
            "round": round_section(json!(100.5), json!(1.0))
        }));
        assert!(above.is_ok(), "{above:?}");
    }

    #[test]
    fn a_round_with_drag_0_is_accepted() {
        let item = parse(json!({
            "class": "consumable", "size": [1, 1],
            "round": round_section(json!(940.0), json!(0.0))
        }))
        .expect("a round that never slows is valid");

        let ItemKind::Round(spec) = item.kind else {
            panic!("expected a round kind, got {:?}", item.kind);
        };
        assert_eq!(spec.drag, 0.0);
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
            ("gear", "round", round_section(json!(940.0), json!(1.0))),
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
    fn an_item_without_weight_is_refused() {
        let result = serde_json::from_value::<ItemDef>(json!({"class": "tool", "size": [1, 1]}));

        let text = result
            .expect_err("an item with no weight is refused")
            .to_string();
        assert!(text.contains("weight"), "{text}");
    }

    #[test]
    fn a_negative_or_non_finite_weight_is_refused() {
        let negative = parse(json!({"class": "tool", "size": [1, 1], "weight": -0.5}))
            .expect_err("a negative weight is refused")
            .to_string();
        let texts = [
            weight_error_text(f32::NAN),
            weight_error_text(f32::INFINITY),
            weight_error_text(f32::NEG_INFINITY),
        ];

        assert!(negative.contains("weight"), "{negative}");
        assert!(negative.contains("-0.5"), "{negative}");
        for text in texts {
            assert!(text.contains("weight"), "{text}");
        }
    }

    #[test]
    fn a_weight_of_0_or_more_is_kept() {
        let none = parse(json!({"class": "tool", "size": [1, 1], "weight": 0.0}))
            .expect("a weightless item is valid");
        let some = parse(json!({"class": "tool", "size": [1, 1], "weight": 2.5}))
            .expect("a positive weight is valid");

        assert_eq!(none.weight, 0.0);
        assert_eq!(some.weight, 2.5);
    }

    #[test]
    fn a_backpack_section_needs_class_gear() {
        for class in ["weapon", "consumable", "tool"] {
            let text = error_text(json!({"class": class, "size": [1, 1], "backpack": {}}));

            assert!(text.contains("backpack"), "{class}: {text}");
            assert!(text.contains(class), "{class}: {text}");
        }
        let item = parse(json!({"class": "gear", "size": [3, 3], "backpack": {}}))
            .expect("gear with a backpack section loads");

        assert!(
            matches!(item.kind, ItemKind::Backpack(_)),
            "{:?}",
            item.kind
        );
    }

    #[test]
    fn a_backpack_section_with_a_key_is_refused() {
        let text = error_text(json!({
            "class": "gear", "size": [3, 3], "backpack": {"slots": 4}
        }));

        assert!(text.contains("slots"), "{text}");
    }
}
