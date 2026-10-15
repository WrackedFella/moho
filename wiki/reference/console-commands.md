# Console commands

**Source:** `flagstone_ui/src/overlays/console/commands.rs` (`CommandProcessor::execute`).
The in-game `help` command is the authority; this page mirrors it.
Open the console with `` ` ``. Flow: [console](../architecture/console.md).

Commands are case-insensitive on the first token; arguments are whitespace-separated.

| Command | Args | Effect |
|---|---|---|
| `help` | | List commands |
| `clear` | | Clear console output |
| `quit` | | Auto-save and exit |
| `god` | | Toggle god mode (event published; **no effect yet**) |
| `noclip` | | Enable noclip (**cannot be turned off from the console**) |
| `sun` | `<yaw> <pitch>` | Sun direction in degrees |
| `time` | `<0-24>` | Time of day in hours: 0 midnight, 6 dawn, 12 noon, 18 dusk |
| `r_debug_view` | `<mode>` | 0 None, 1 Normals, 2 Bias, 3 Shadows, 4 LightLevel, 5 PointLights; other values are rejected with a message |
| `r_shadow_quality` | `<0-4>` | Off, Low, Med, High, Ultra |
| `r_ssao_quality` | `<0-4>` | Off, Low, Med, High, Ultra |
| `spawn` | `<type> [args]` | `torch`, `light` (args `r g b`), `cube`, `sphere`; placed by raycast from the camera |
| `snd_play_at` | `<path> <x> <y> <z> [volume]` | Play a sound at a world position in metres (volume defaults to 1.0); non-finite numbers are rejected |

Out-of-range quality values are rejected with a message. Unknown commands print a hint.

```text
> time 6              # dawn
> sun 45 60
> r_shadow_quality 4
> spawn light 1 0.5 0
> snd_play_at searchlight/assets/rifle_shot.wav 1 0 -5 0.8
```
