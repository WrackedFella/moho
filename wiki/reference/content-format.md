# Content format

**Source:** `moho_core/src/content.rs`, `searchlight_rules/src/weapon.rs`,
`searchlight/src/tuning.rs`, `searchlight/content/`.

Tunable game numbers live in definition files, not in code. `moho_core::content` loads
them; each game line owns the types they deserialize into.

## Layout

One TOML file per kind, named `<kind>.toml`, in a content root. Each top-level table is
one definition and its name is the id.

```toml
# searchlight/content/weapons.toml
[rifle]
magazine_capacity = 30
damage = 30
fire = "semi_auto"
cooldown_ticks = 6
reload_ticks = 120
range = 100.0
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
- `load_kind::<T>(&roots, "weapons")` reads `<root>/weapons.toml` from every root (a
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
| `weapons` | `rifle` (`RIFLE_ID`) | `searchlight_rules::RifleDef` | `magazine_capacity`, `damage`, `fire` (`semi_auto`), `cooldown_ticks`, `reload_ticks`, `range` (metres) |

`searchlight::tuning::tuning(&RifleDef)` builds the `SimConfig` from a definition; the
remaining sim numbers (movement, body profile, enemy tuning) are still set in code.
