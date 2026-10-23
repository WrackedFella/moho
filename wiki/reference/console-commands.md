# Console commands

**Source:** `flagstone_ui/src/console_commands.rs` (strategy), `moho_ui_shell/src/console/registry.rs` (engine).
The in-game `help` command is the authority; this page mirrors it.
Open the console with `` ` `` (rebindable: the `console` entry of `[bindings]`). Flow: [console](../architecture/console.md).

Commands are case-insensitive on the first token; arguments are whitespace-separated.

| Command | Args | Effect |
|---|---|---|
| `help` | | List commands |
| `clear` | | Clear console output |
| `quit` | | Auto-save and exit |
| `god` | | Toggle god mode (event published; **no effect yet**) |
| `noclip` | | Toggle noclip |
| `sun` | `<yaw> <pitch>` | Sun direction in degrees |
| `time` | `<0-24>` | Time of day in hours: 0 midnight, 6 dawn, 12 noon, 18 dusk |
| `r_debug_view` | `<mode>` | 0 None, 1 Normals, 2 Bias, 3 Shadows, 4 LightLevel, 5 PointLights, 6 SkyAmbient, 7 SkyReflection; other values are rejected with a message |
| `r_shadow_quality` | `<0-4>` | Off, Low, Med, High, Ultra |
| `r_ssao_quality` | `<0-4>` | Off, Low, Med, High, Ultra |
| `spawn` | `<type> [args]` | `torch`, `light` (args `r g b`), `cube`, `sphere`; placed by raycast from the camera |
| `audio_cue` | `<path> <x> <y> <z> [volume]` | Play a sound at a world position in metres (volume defaults to 1.0); non-finite numbers are rejected |
| `marker` | `<id> <x> <y> <z> [size] [fill\|outline]` | Box from the corner `x y z` to `+size` (default 1) on each axis, in metres, drawn translucent (`fill`, default) or as edges only (`outline`); the same id replaces the marker; id is a non-negative integer, size must be finite and above zero |
| `marker_clear` | `[id]` | Remove one marker, or all |
| `r_view_height` | `<metres\|off>` | Draw nothing in the world above a world-y height (markers and shadows included); `off` clears; non-finite numbers are rejected |

Out-of-range quality values are rejected with a message. Unknown commands print a hint.

```text
> time 6              # dawn
> sun 45 60
> r_shadow_quality 4
> spawn light 1 0.5 0
> marker 1 10 4 -3 2    # 2 m box
> marker 2 10 4 -3 2 outline
> marker_clear 1
> audio_cue searchlight/assets/rifle_shot.wav 1 0 -5 0.8
```

## Engine console (`moho_ui_shell`)

**Source:** `moho_ui_shell/src/console/registry.rs`, `moho_ui_shell/src/apply.rs`. Shared by both games; strategy's commands above are registered on it.

| Command | Args | Effect |
|---|---|---|
| `help`, `clear`, `quit` | | As above |
| `r_debug_view` | `<0-7>` | Debug view, same modes as above; overrides the game's lighting value in every later lighting update (`Renderer::set_debug_view`) |
| `r_shadow_quality` | `<0-4>` | As above |
| `r_ssao_quality` | `<0-4>` | As above |
| `audio_cue` | `<path> <x> <y> <z> [volume]` | Play a sound at a world position in metres (volume defaults to 1.0); non-finite numbers are rejected |

## Searchlight console (`searchlight`)

**Source:** `searchlight/src/console_commands.rs`. Adds to the engine commands above.

| Command | Args | Effect |
|---|---|---|
| `tgm` | `[0\|1]` | God mode: no argument toggles, `1` on, `0` off. Enemy hits stop damaging the player. Stored as `god_mode` under `[debug]` in the game's prefs file, so it persists across runs |
