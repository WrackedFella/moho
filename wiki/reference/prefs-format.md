# Preferences file

**Source:** `moho_core/src/prefs/` (`mod.rs`, `reader.rs`), `moho_input/src/bindings.rs`, `moho_input/src/key.rs`, `moho_input/src/pad.rs`,
`flagstone_ui/src/actions.rs`, `flagstone/src/main.rs` (`App::save_prefs`, `App::adopt_settings`),
`flagstone/src/app/initializer.rs` (`build_actions`), `moho_audio/src/audio_settings.rs` (`AudioSettings::from_prefs`), `flagstone_ui/src/adapter/mod.rs`. Path: `config/prefs.ini`,
relative to the working directory. The FPS (`searchlight`) keeps its own file,
`config/searchlight.ini` (`searchlight::prefs_path`), read with `Prefs::load_from`. It
splits the `console` entry from the game's own `[bindings]` (`split_console_binding`) so
neither loader warns about the other's key.

`Prefs::load` creates the file with defaults if it is missing. Missing sections or keys
take their defaults silently. `Prefs::save` writes every key below.

Anything else that can't be used is logged as a warning and falls back:

| Problem | Warning names | Falls back |
|---|---|---|
| Value that doesn't parse or isn't allowed, or a key with no `=` | file, section, key, raw value | that key |
| Unknown key | file, section, key | nothing; the other keys load |
| Unknown section | file, section | nothing; the other sections load |
| File the INI parser rejects (e.g. `[video` with no `]`) | file, parser reason (0-based line) | every key |
| File that can't be read as UTF-8 text | file, I/O reason | every key |
| Missing file that can't be created | file, I/O reason | every key |

One bad value never stops the other keys from loading. Section and key names are
case-insensitive; `;` and `#` start a comment anywhere on a line, so they can't appear
in a value. Keys before any section header are read as `[prefs]` when there is no
`[prefs]` section; when there is one, they are ignored and each is reported as an
unknown key of section `[default]`.

```mermaid
flowchart LR
    F["config/prefs.ini"] -->|"Prefs::load"| P["Prefs"]
    P --> B["[bindings] raw text"] -->|"ActionBindings::load"| I["input: bindings"]
    P --> M["input: sensitivity, invert-Y"]
    P --> Au["moho_audio: category volumes"]
    P --> Rn["moho_renderer: shadow / SSAO quality"]
    P --> W["window: mode, resolution"]
    P --> S["ChunkStreamer: radii, budget"]
    B -.->|"ActionBindings::to_section"| P
    P -->|"Prefs::save"| F
```

## 1 Running prefs and saving

`App::prefs` is the running copy; `App::prefs_path` is where the app saves it (`None` for
a headless `App`, so its console commands never touch the user's file; a Settings screen
opened there still saves to the default path).

- **Console graphics commands** (`SetShadowQuality`, `SetSsaoQuality`) update `App::prefs`
  and call `App::save_prefs`, with or without a window; a failed write is logged.
- **Settings screen** is seeded from `App::prefs` when it opens
  (`EguiAdapter::open_settings`), so a console change made earlier is not overwritten
  on save. The screen writes the file itself. Prefs never ride on the event bus
  ([ADR-0011](../../_todo/adr/0011-simulation-stays-network-ready.md)): on `UiEvent::SettingsSaved` the app takes the
  saved copy from the adapter (`EguiAdapter::take_saved_prefs`, once) and calls
  `App::adopt_settings`.
- **`adopt_settings`** replaces `App::prefs` and rebuilds the input `ActionMap` through
  `build_actions`, the same function startup uses, so new bindings, sensitivity,
  invert-Y, mouse smoothing and mouse acceleration apply without a restart. It also queues `AudioSettings::from_prefs`
  (`moho_audio`), which the next frame applies to the audio engine. It does not write the file.
- **Startup** builds the same `AudioSettings` from the loaded prefs and applies it on the
  first frame. Each category volume is its pref divided by ten and clamped to 0..=1; 0 is silent.

## 2 Keys

| Section | Key | Accepted values | Default |
|---|---|---|---|
| `[prefs]` | `mouse_sensitivity` | finite float, Source scale (see below) | `3` |
| | `invert_mouse_y` | `true` `false` `1` `0` (any case) | `false` |
| | `mouse_smoothing` | `true` `false` `1` `0` (any case) | `false` |
| | `mouse_acceleration` | `true` `false` `1` `0` (any case) | `false` |
| | `mouse_acceleration_strength` | finite float, Source `m_customaccel_scale` scale; negatives load unchanged and act as 0; the Settings screen clamps the value to 0 to 1 when shown | `0.04` |
| `[bindings]` | one key per action id | binding list | per action, below |
| `[audio]` | `sound_effect_volume` `music_volume` `ui_volume` `voice_volume` | finite float on a 0 to 10 scale (the UI offers 0.0 to 10.0), written with 1 decimal | `8.0` `6.0` `7.0` `9.0` |
| `[graphics]` | `shadow_quality` `ssao_quality` | unsigned integer; the UI offers 0 (off) to 4 (ultra) | `3` `3` |
| `[video]` | `window_mode` | `Windowed` \| `Fullscreen` \| `Borderless`, exact case | `Windowed` |
| | `window_width` `window_height` | unsigned integer pixels | `1920` `1080` |
| | `vsync` | `true` \| `false`; whether presented frames wait for the display refresh | `true` |
| | `fps_max` | unsigned integer frames per second applied while `vsync` is off; `0` is uncapped | `300` |
| `[world]` | `load_radius` `unload_radius` | integer chunks | `8` `12` |
| | `chunks_per_frame` | unsigned integer, 1 or more | `4` |
| `[debug]` | one key per game debug value | free text, see below | none |

`mouse_sensitivity` is on Source's scale: one count turns `0.022` degrees times the
value (`source_look_per_count`, `moho_input/src/action_map.rs`), so `3` is the default feel.
`invert_mouse_y` negates the mouse's vertical counts only; the stick is unaffected.
`mouse_acceleration_strength` applies only while `mouse_acceleration` is on. It adds
`source_look_per_count(strength)` look per count for each count per millisecond of mouse
speed. Values carry over from Source's `m_customaccel_scale` only approximately: Source's
acceleration varies with frame rate, and ours matches it at 1000 fps.

`[bindings]` is stored by `Prefs` as raw text (`Prefs::bindings`, `Prefs::set_bindings`);
the game interprets it. `ActionBindings::load` (`moho_input`) reads it against the
game's `Action` enum: each key is an action's stable `name()`, each value a binding
list. Absent actions keep their defaults; `ActionBindings::to_section` writes every
action, defaults included. Line problems are returned as `BindingWarning`s and the
action keeps its default (`flagstone_ui::actions::load_bindings` logs them): an unknown
action id, or a value with any unrecognised key. A `[bindings]` line with no `=` is a
malformed prefs line. `console` is reserved in every game's `[bindings]`: it is the
engine console's key (`moho_ui_shell`, default backtick; an unbound or unreadable value, or one
with a `Hold ` item, falls back to backtick), not a game action. The pre-`[bindings]` `key_*` lines under `[prefs]` are unknown
keys and ignored.

A binding list is comma-separated key names (`W`, `Spacebar, Enter`); an action may
have several, and is active when any of them is (see `Hold ` below). Empty or `Unbound` (any case) is an empty list. Names are
case-insensitive and come from `Key::name` in `moho_input/src/key.rs`: letters, digits,
punctuation spelled as a word (`Comma`, `Semicolon`, `LeftBracket`, ...), `ArrowUp`..,
`Escape`, `Tab`, `Backspace`, `Enter`, `Spacebar`, and the modifiers `Shift`, `Ctrl`,
`Alt` bound on their own. `Key::parse` also accepts the aliases `Up`, `Esc`, `Return`,
`Space`, `Control` and the punctuation glyphs, but in `prefs.ini` write punctuation as
words: the INI reader cuts a line at `;` or `#`, reads a line with `[` as a section
header, and `,` separates list items. There is no modifier syntax such as `Ctrl+W`
and no numeric key codes.

`[debug]` is a game-owned section of named values (a game's debug toggles), stored raw like
`[bindings]` and written only when non-empty. `Prefs::debug_value` reads one;
`Prefs::set_debug_value` stores one and returns `DebugValueError` for a name or value
that would not read back exactly. Names are non-empty lowercase letters, digits and
underscores. Values are non-empty, have no leading or trailing whitespace, and contain no
`;`, `#` or line break. The Settings screen's save keeps them.

A `Hold ` prefix (any case) makes a binding fire only after the input has been held for the game's hold threshold, e.g. `Hold R`, `Hold Pad West`; without it the binding fires on press, or on a quick release when the same input also has a `Hold` binding ([input](../architecture/input-and-state.md)). `Binding::name` writes the prefix back.

Gamepad bindings are written `Pad <name>` and mix freely with keys in one list
(`jump=Spacebar, Pad South`). Buttons (`PadButton`): `South` `East` `North` `West`
`LeftBumper` `RightBumper` `LeftTrigger` `RightTrigger` `Select` `Start` `Mode`
`LeftThumb` `RightThumb` `DPadUp` `DPadDown` `DPadLeft` `DPadRight`. Stick directions
are `Pad <Stick> <Dir>` with `LeftStick` or `RightStick` and `Up` `Down` `Left` `Right`
(`Pad LeftStick Up`). A malformed pad name is an unrecognised key and warns. The right
stick's analog look is not a binding.

Strategy actions (`StrategyAction`, `flagstone_ui/src/actions.rs`) and defaults:

| Id | Default |
|---|---|
| `move_forward` `move_back` `move_left` `move_right` | `W` `S` `A` `D`; `Pad LeftStick` `Up` `Down` `Left` `Right` |
| `ascend` | `Spacebar`, `Pad RightTrigger` |
| `jump` | `Spacebar`, `Pad South` |
| `descend` | `Ctrl`, `Pad LeftTrigger` |
| `sprint` | `Shift`, `Pad LeftThumb` |

Action ids are persisted and never renamed.

Audio sections map to `AudioCategory` in `moho_audio/src/audio_source.rs`: `SoundEffect`,
`Music`, `UserInterface`, `Voice`.

World keys drive [world streaming](../architecture/world-streaming.md).

## 3 Example

```ini
[prefs]
mouse_sensitivity=1.8
invert_mouse_y=false
mouse_smoothing=false

[bindings]
move_forward=W
jump=Spacebar, Enter
descend=Unbound

[audio]
music_volume=5.0

[graphics]
shadow_quality=3
ssao_quality=3

[video]
window_mode=Windowed
window_width=1280
window_height=720
vsync=true
fps_max=300

[world]
load_radius=8
unload_radius=12
chunks_per_frame=4
```
