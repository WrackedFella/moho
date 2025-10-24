# Preferences File Format

The `config/prefs.ini` file stores user preferences for the Moho game engine.

## File Structure

The preferences file is organized into sections:

### [prefs] - Controls Section
Control bindings and input settings.

```ini
[prefs]
key_w=W
key_a=A
key_s=S
key_d=D
mouse_sensitivity=1.0
input_filtering_enabled=true
```

### [audio] - Audio Section
Volume levels for different audio categories (1.0 to 10.0).

```ini
[audio]
sound_effect_volume=7.0
music_volume=5.0
ui_volume=8.0
voice_volume=7.0
```

## Audio Categories

The audio settings correspond to the `AudioCategory` enum in `engine_audio/src/audio_source.rs`:

- **SoundEffect**: General game sound effects (default: 7.0)
- **Music**: Background music (default: 5.0)
- **UserInterface**: UI interaction sounds like button clicks (default: 8.0)
- **Voice**: Character voice and dialogue (default: 7.0)

## Value Ranges

- **Key bindings**: Human-readable format (e.g., "W", "Ctrl+W", "ArrowUp")
- **mouse_sensitivity**: 0.01 to 10.0 (decimal values, 2 decimal places)
- **Audio volumes**: 1.0 to 10.0 (decimal values, 1 decimal place)
- **Booleans**: "true" or "false"

## Example Complete File

```ini
[prefs]
key_w=W
key_a=A
key_s=S
key_d=D
mouse_sensitivity=1.2
input_filtering_enabled=true

[audio]
sound_effect_volume=7.0
music_volume=5.0
ui_volume=8.0
voice_volume=7.0
```
