# Content format

**Source:** `moho_core/src/content.rs`, `searchlight_rules/src/item.rs`, `searchlight_rules/src/kit.rs`,
`searchlight_rules/src/stats.rs`, `searchlight_rules/src/load.rs`, `searchlight/src/kit.rs`,
`searchlight/src/player.rs`, `searchlight/src/tuning.rs`, `searchlight/content/`.

Tunable game numbers live in definition files, not in code. `moho_core::content` loads
them; each game line owns the types they deserialize into.

## Layout

One TOML file per kind, named `<kind>.toml`, in a content root. Each top-level table is
one definition and its name is the id.

```toml
# searchlight/content/items.toml
[rifle]
class = "weapon"
size = [2, 5]
weight = 3.4
condition = 100

[rifle.weapon]
well = "rifle"
fire = "semi_auto"
cooldown_ticks = 6
reload_ticks = 120
charge_ticks = 30

[mag_rifle]
class = "gear"
size = [1, 2]
magazine = { well = "rifle", caliber = "5.56x45", capacity = 30 }

[round_rifle_ball]
class = "consumable"
size = [1, 1]
round = { caliber = "5.56x45", muzzle_velocity = 940.0, drag = 1.0, damage = 30 }
```

`searchlight` keeps its base root at `searchlight/content/` (`searchlight::content_root`).
Mod roots live under `mods/<name>/` in the working directory (`searchlight::mods_dir`), each
holding the same `<kind>.toml` files.

## Mods

`[content] mods = a, b` in the game's prefs file ([prefs format](prefs-format.md)) lists
mod names in load order. `ContentRoots::with_mods(mods_dir, names)` appends
`mods_dir/<name>` per name after the base, so later mods override earlier ones. A name must
be non-empty ASCII letters, digits, `_` or `-` (`ContentError::InvalidRootName`); a missing
folder is `ContentError::MissingRoot`. A mod defines only the ids it changes, and each
entry it defines must be complete.

## Loading

- `ContentRoots::base(path)` builds the root list; a missing folder is
  `ContentError::MissingRoot`. Roots are ordered base first, then mods; a later root's entry replaces
  an earlier one with the same id whole, not field by field.
- `load_kind::<T>(&roots, "items")` reads `<root>/items.toml` from every root (a
  missing file is skipped) and deserializes each table as `T`, giving `Definitions<T>`.
- `Definitions::get(id)` is optional; `require(id)` fails with `ContentError::MissingId`.

## Errors

Every failure is a `ContentError` naming the file, and the id where one applies: `Unreadable`,
`Syntax` (TOML, with line and column), `Invalid` (an entry does not fit its type, including
a mod entry missing a field), `MissingId`, and the root errors above. The reason is collapsed to one line. A game treats any of them as fatal at
start: the `searchlight` binary logs the error and exits with failure.

## Definition types

Types derive `Deserialize` in the rules crate. `#[serde(deny_unknown_fields)]` makes a
misspelt field an error rather than a silent default. Enum values are snake_case ids, which
are the file's contract.

| Kind | Id | Type | Fields |
|---|---|---|---|
| `items` | any item id | `searchlight_rules::ItemDef` | `class` (`weapon`, `consumable`, `tool`, `gear`), `size` (`[width, height]`, each at least 1), `weight` (kg, finite and 0 or more, required), `condition` (0 to 100) or `uses`, never both, and at most one section: `weapon`, `magazine`, `round`, `rig` or `backpack`, and optional `stats` (list of `{ stat, add }`, `{ stat, scale }` or `{ stat, cap }`, exactly one verb each; `stat` is `carry_capacity`, `movement_speed` or `stamina`), which a worn rig or backpack contributes to that stat; `searchlight_rules::ItemStat` |
| `player` | `player` | `searchlight_rules::PlayerDef` | `strength`, `carry_capacity_per_strength`, `movement_speed`, `stamina_pool` (all finite, 0 or more), `slowed_scale` (0 to 1) and `thresholds` (`slowed`, `no_sprint`, `cannot_move`: finite and strictly increasing from above 0, as fractions of carry capacity); see [load and stats](../searchlight/architecture.md#11-load-and-stats) |
| `kits` | `start` (`START_KIT_ID`), `enemy` (`ENEMY_KIT_ID`), optional `enemy_<grade>` (`rookie`, `novice`, `experienced`, `veteran`, `professional`) | `searchlight_rules::KitDef` | `rig` and `weapon` (item ids), `loaded`, `magazines` (magazine entries, below), optional `backpack` (item id) and `carried` (`KitItem` entries, below) |

Item sections (`ItemKind`), each valid only on its class:

| Section | Class | Fields |
|---|---|---|
| `weapon` | `weapon` | `well`, `fire` (`semi_auto`), `cooldown_ticks`, `reload_ticks`, `charge_ticks` (ticks added to a reload when the chamber is empty); `searchlight_rules::RifleDef`. A weapon that still names a `range` or a `damage` is refused: a round's reach comes from its flight and its damage from the round |
| `magazine` | `gear` | `well` (the weapon's `well` it fits), `caliber`, `capacity` |
| `round` | `consumable` | `caliber`, `muzzle_velocity` (m/s, above `MIN_SPEED` = 100), `drag` (1/s, 0 or more), `damage` (at least 1: the hit's damage at muzzle velocity, scaled down in proportion to the speed the round arrives at, never below 1); all required, a bad value fails the load naming the round and the field; `searchlight_rules::RoundSpec` |
| `rig` | `gear` | `magazine_slots` |
| `backpack` | `gear` | none; the section is `{}` and marks an item for the backpack slot |

A `carried` entry is `{ item }`, plus `round` and `rounds` when the item is a magazine, or `count` when it is a round. The `start` kit resolves into the player's `Loadout`; the `enemy` kit resolves into the `Remains` a dead enemy leaves (`searchlight::kit::load_enemy`), and a grade with an `enemy_<grade>` kit leaves that kit's remains instead (`load_enemy_for_grade`); a grade without one leaves the `enemy` kit's. A kit entry in `loaded` or `magazines` is `{ magazine, round, rounds }`; `rounds` defaults to a
full magazine. `Kit` resolution fails (`KitError`) on an unknown item, an item of the wrong kind,
a round of another caliber than its magazine, more rounds than the magazine holds, a `loaded`
magazine whose `well` differs from the weapon's, or more magazines than the rig has slots. Rig
magazines of another `well` are allowed; reload skips them.

`weapons.toml` is no longer read: a mod that still ships one is ignored without an error.

`searchlight::kit::load` resolves a kit and `searchlight::tuning::shipped` builds the
`SimConfig` from the content roots (kit, item definitions and `player.toml`); the remaining sim numbers (movement, body profile, enemy tuning) are still
set in code.
