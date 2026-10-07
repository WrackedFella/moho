# A malformed preference is reported and only that key falls back

**Feature:** [ENG-F1](_feature.md)
**Issue:** #91

## Summary

`Prefs::load` replaces any value it can't parse with the default and says
nothing. A typo in `config/prefs.ini` quietly resets that setting, and a file
the parser can't read resets every setting. Each fallback should log a warning
that names what was wrong, and only the bad key should fall back.

## Deliverables

- Each load fallback other than a missing key or section logs a warning naming
  the file, section, key and raw value (or the file and the reason, for
  whole-file failures).
- One bad value never stops the other keys from loading.
- `wiki/reference/prefs-format.md` documents the warn-and-fall-back rule in
  place of "unknown keys are ignored" and "unknown → `Windowed`".

## Acceptance criteria

```gherkin
Scenario Outline: A malformed value is reported and that key alone falls back
  Given prefs.ini sets every key to a valid non-default value
  And it sets <key> in [<section>] to "<value>"
  When preferences load
  Then a warning names prefs.ini, [<section>], <key> and "<value>"
  And <key> has its default value
  And every other key has the value from the file

  Examples:
    | section  | key                     | value   |
    | prefs    | mouse_sensitivity       | fast    |
    | prefs    | key_jump                | Spcae   |
    | prefs    | input_filtering_enabled | yes     |
    | audio    | music_volume            | 5,0     |
    | graphics | shadow_quality          | high    |
    | video    | window_mode             | Windowd |
    | video    | window_width            | -1920   |
    | world    | chunks_per_frame        | 0       |

Scenario: Each malformed key gets its own warning
  Given prefs.ini has malformed values for mouse_sensitivity and window_width
  When preferences load
  Then two warnings are logged, one naming each key

Scenario: A valid file loads without warnings
  Given prefs.ini as written by Prefs::save
  When preferences load
  Then no warning is logged
  And every key has the value from the file

Scenario: A missing key is not a warning
  Given prefs.ini has no [world] section
  When preferences load
  Then no warning is logged
  And load_radius, unload_radius and chunks_per_frame have their defaults

Scenario: An unknown key is reported
  Given prefs.ini contains mouse_sensitivty=2.0 in [prefs]
  When preferences load
  Then a warning names prefs.ini, [prefs] and mouse_sensitivty
  And mouse_sensitivity has its default value

Scenario: "unbound" still unbinds a key
  Given prefs.ini sets key_jump to "unbound"
  When preferences load
  Then no warning is logged
  And key_jump is unbound

Scenario: A file that can't be parsed falls back entirely, loudly
  Given prefs.ini is not valid for the prefs format
  When preferences load
  Then a warning names prefs.ini and the parser's reason, with the line if known
  And every key has its default value
  And the game reaches the main menu

Scenario: An unreadable file falls back entirely, loudly
  Given prefs.ini exists but can't be read as text
  When preferences load
  Then a warning names prefs.ini and the I/O reason
  And every key has its default value

Scenario: Failing to create the default file is reported
  Given prefs.ini is missing and its directory can't be written
  When preferences load
  Then a warning names prefs.ini and the reason
  And every key has its default value
```

## Tech spec

Pending (Tech Lead).

## Verification

- Edit `config/prefs.ini` to set `mouse_sensitivity=fast`, run `cargo run`,
  and see the warning in the console output. Other settings (window mode,
  bindings, volumes) are as set in the file.

## Notes

- **Parser:** criteria hold for INI and for TOML. The `ini` → `toml` call in
  [the ENG-F13 audit](../ENG-F13-dependency-audit/audit.md) is open. If TOML
  wins, a single `#[serde(default)]` struct deserialize fails the whole file on
  one bad type, which breaks the per-key outline. Read keys one at a time, or
  land this card with the migration.
- **Out of range is out of scope** (a negative volume, quality above 4); only
  unparseable or disallowed values are malformed. `chunks_per_frame=0` counts
  as disallowed because load currently clamps it to 1 without a word.
