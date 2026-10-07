# A malformed preference is reported and only that key falls back

**Feature:** [ENG-F1](_feature.md)
**Issue:** #91

## Summary

`Prefs::load` replaces any value it can't parse with the default and says
nothing. A typo in `config/prefs.ini` quietly resets that setting, a misspelt
section header resets the whole section, and a file the parser can't read
resets every setting. Each fallback should log a warning that names what was
wrong, and only the bad key should fall back.

## Deliverables

- Each load fallback other than a missing key or section logs a warning naming
  the file, section, key and raw value (or the file and the reason, for
  whole-file failures).
- One bad value never stops the other keys from loading.
- `wiki/reference/prefs-format.md` documents the warn-and-fall-back rule and
  matches what load accepts: no "unknown keys are ignored", no
  "unknown → `Windowed`", no `Ctrl+W` modifier syntax (the parser has none),
  no "clamped" for graphics quality (load does not clamp), and booleans are
  `true`/`false`/`1`/`0`.

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
    | prefs    | mouse_sensitivity       | NaN     |
    | prefs    | key_jump                | Spcae   |
    | prefs    | key_w                   |         |
    | prefs    | input_filtering_enabled | yes     |
    | audio    | music_volume            | 5,0     |
    | graphics | shadow_quality          | high    |
    | video    | window_mode             | Windowd |
    | video    | window_width            | -1920   |
    | world    | chunks_per_frame        | 0       |

Scenario: A key with no value is reported
  Given prefs.ini has a line "window_mode" with no "=" in [video]
  When preferences load
  Then a warning names prefs.ini, [video] and window_mode
  And window_mode has its default value

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

Scenario: An unknown section is reported
  Given prefs.ini spells the [video] header as [vidoe]
  When preferences load
  Then a warning names prefs.ini and [vidoe]
  And every [video] key has its default value

Scenario: "unbound" still unbinds a key
  Given prefs.ini sets key_jump to "unbound"
  When preferences load
  Then no warning is logged
  And key_jump is unbound

Scenario: A file that can't be parsed falls back entirely, loudly
  Given prefs.ini has a line "[video" with no closing bracket
  When preferences load
  Then a warning names prefs.ini and the parser's reason, with the line if known
  And every key has its default value

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

**Design.**
- New `moho_core/src/prefs/reader.rs` owns reading; `mod.rs` keeps the type,
  accessors and `save`. Load returns its problems as data and only `load()`
  logs them, so every scenario is a plain unit test (no log-capture
  dependency) and `load()`'s signature, used by `AppConfig::from_prefs` and
  `SettingsState::new`, does not change.
- Types (`pub`, re-exported from `prefs`):
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub enum PrefsIssue {
      Malformed { section: String, key: String, value: Option<String> }, // None: no '='
      UnknownKey { section: String, key: String },
      UnknownSection { section: String },
      Unparseable { reason: String },
      Unreadable { reason: String },
      NotCreated { reason: String },
  }
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct PrefsWarning { pub file: PathBuf, pub issue: PrefsIssue } // Display names file + issue
  ```
- Functions on `Prefs`:
  - `pub fn parse(content: &str) -> (Self, Vec<PrefsIssue>)`: pure.
  - `pub fn load_from(path: &Path) -> (Self, Vec<PrefsWarning>)`: I/O, wraps
    each issue with `path`. Missing file: create it with defaults; a failure
    of `create_dir_all` or the write is `NotCreated`.
  - `pub fn load() -> Self`: `load_from(&Self::config_path())`, then one
    `warn!` per warning, through the logging crate `moho_core` declares at
    the time (`tracing` with `file`/`issue` fields if #98, ENG-F2-05
    structured logging, has landed; `log::warn!("{w}")` otherwise, which
    #98's pass converts).
  - `pub fn to_ini_string(&self) -> String`: the text `save` writes today;
    `save` becomes create-dir + `fs::write(path, self.to_ini_string())`.
- `parse` reads the `ini::macro_safe_read` map once (today it parses the
  file five times). Known keys are one table, section → key → setter, so
  "unknown key" and "unknown section" fall out of the same lookup. A key
  present in the table is parsed with its type's parser; failure records
  `Malformed` and leaves the default.
- Section names: `[prefs]`, `[audio]`, `[graphics]`, `[video]`, `[world]`,
  plus `default` (keys before any header), which today is read as `[prefs]`
  when `[prefs]` is absent. Keep that alias and precedence unchanged.
- Value parsers (each returns `Option`):
  - binding: `parser::parse_binding` changes to `fn(&str) -> Option<Binding>`
    (empty or unknown name → `None`, `unbound` → `Some(Binding::new(0, 0))`).
    It is re-exported but has no caller outside `prefs` (text search);
    run GitNexus `impact` on it before the change.
  - `f32`: `parse` and `is_finite` (rejects `NaN`, `inf`).
  - `bool`: `true`/`false`/`1`/`0`, ASCII case-insensitive. Anything else is
    malformed (today it silently means `false`).
  - `u32`: `parse`; `chunks_per_frame` also rejects `0` (replaces the silent
    `.max(1)`; `set_world_chunks_per_frame` keeps its own clamp).
  - window mode: private `WindowMode::parse(&str) -> Option<Self>` replaces
    `from_str` (exact names, as `save` writes them).
- Parser facts this relies on (checked in configparser 1.0.0, under `ini`
  1.3.0): section and key names are lowercased; a line without `=` is a key
  with value `None`; `;` and `#` start a comment anywhere on a line; the only
  errors are `[` without `]` and an empty key, and their text carries a
  0-based line number.
- `tempfile` (already in `[workspace.dependencies]`) becomes a `moho_core`
  dev-dependency for the `load_from` tests.

**Out of scope.**
- Range checks: negative volumes, quality above 4, `window_width=0`.
- Showing warnings in game (toast, console); log only.
- The four discarded `save()` results (`event_processor.rs`, settings
  `render_ops.rs` and `mod.rs`); [ENG-F1-02](ENG-F1-02-error-handling-backlog.md) territory.
- Settings re-reading the file on open (see Notes).
- `ini` 1.3 → 2.0, TOML, or a homebrew reader.
- Key naming, `phf` removal and action bindings ([ENG-F12](../ENG-F12-input-actions/_feature.md), #82).
- Moving `[world]` streaming keys out of engine prefs.
- Splitting the rest of `prefs/mod.rs` ([ENG-F1-03](ENG-F1-03-god-module-splits.md)).

**Test map** (gate class: **glue**: prefs is config, so tests and code land together).

| Scenario | Test (`moho_core::prefs::reader::tests::…`) |
|---|---|
| Outline (10 rows) | `malformed_value_warns_and_only_that_key_falls_back` (table-driven, row label in the assert message) |
| Key with no value | `key_without_value_is_malformed` |
| Each malformed key warned | `each_malformed_key_gets_its_own_warning` |
| Valid file, no warnings | `saved_prefs_parse_back_unchanged_without_warnings` (proptest over bindings from the named set, finite sensitivity, volumes `n/10`, any `u32` sizes, `chunks_per_frame ≥ 1`) |
| Missing key | `missing_section_is_not_a_warning` |
| Unknown key | `unknown_key_is_reported` |
| Unknown section | `unknown_section_is_reported_and_its_keys_default` |
| `unbound` | `unbound_unbinds_without_warning` |
| Unparseable file | `unparseable_file_falls_back_with_parser_reason` |
| Unreadable file | `unreadable_file_falls_back_with_io_reason` (`load_from`, file of invalid UTF-8) |
| Default file not created | `failed_default_file_creation_is_reported` (`load_from`, parent path is a regular file) |
| Warning names prefs.ini | `warning_names_the_file` (`load_from`, asserts `Display`) |
| Edge: legacy `default` section | `keys_before_any_header_load_as_prefs` |
| Edge: `to_ini_string` is what `save` writes | `save_writes_to_ini_string` (`tempfile`) |

`test_video_round_trip_windowed` and `test_window_mode_from_str` in `mod.rs`
are replaced by the proptest and a `WindowMode::parse` case.

**Risks.**
- Behaviour change: hand-written values that used to pass silently now warn
  and take the default, e.g. `input_filtering_enabled=no` (was `false`, now
  `true`). Files written by `Prefs::save` are unaffected.
- Hand-written bindings to `;`, `#` or `[` can't survive INI (comment, or a
  section line that fails the whole file). Capture can't produce them
  (`moho_input` maps no punctuation), so only hand edits hit this.
- The parser's line number is 0-based; it is passed through as-is.
- Merge order with #98 (ENG-F2-05) is free; see Design.
- Blast radius: `moho_core::prefs` only; `load()` keeps its signature.

## Verification

- Edit `config/prefs.ini` to set `mouse_sensitivity=fast`, run `cargo run`,
  and see the warning in the console output. Other settings (window mode,
  bindings, volumes) are as set in the file.
- Edit `config/prefs.ini` so a line reads `[video` (no closing bracket), run
  `cargo run`, see the warning, and reach the main menu.

## Notes

- **Parser:** `ini` stays (ENG-F13 call, 2026-10-07).
- **Order:** land before [ENG-F12](../ENG-F12-input-actions/_feature.md) (#82)
  reworks binding persistence, so action bindings are read through this
  per-key reader instead of a second silent one.
- **Out of range is out of scope** (a negative volume, quality above 4); only
  unparseable or disallowed values are malformed. `chunks_per_frame=0` and
  non-finite floats count as disallowed.
- **Repeated warnings:** `AppConfig::from_prefs` and `SettingsState::new` each
  call `Prefs::load`, so the warnings repeat when settings open. Sharing one
  `Prefs` is UI wiring for [ENG-F18](../ENG-F18-shared-ui-shell/_feature.md).
- **Deferred:** showing load warnings to the player waits for
  [ENG-F18](../ENG-F18-shared-ui-shell/_feature.md)'s UI shell.
