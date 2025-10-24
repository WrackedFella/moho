# engine_audio

This crate provides audio functionality for the Moho game engine. It handles audio playback, sound effects, background music, and spatial audio positioning.

## Features

- Cross-platform audio playback using rodio
- Multiple audio format support (WAV, OGG, MP3, FLAC)
- Audio asset management and caching
- Volume and audio settings control
- Event-driven audio system for UI interactions
- Future: 3D spatial audio positioning

## Usage

```rust
use engine_audio::{AudioSystem, AudioEvent};

// Initialize the audio system
let mut audio_system = AudioSystem::new()?;

// Load and play a sound effect
audio_system.play_sound("click.wav", 1.0)?;

// Handle audio events
audio_system.handle_event(AudioEvent::ButtonClick);
```

## Architecture

The audio system follows the modular pattern used throughout the Moho engine:

- `AudioSystem` - Main audio management and playback coordinator
- `AudioSource` - Individual audio file representation and metadata
- `AudioEvent` - Event-driven audio triggers for UI and game events
- `AudioSettings` - User configuration for volume levels and audio preferences