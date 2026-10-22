# moho_audio

Cross-platform audio system for the Moho engine. Provides sound effect playback, background music management, and event-driven audio triggers.

## 1 Architecture

The audio system is built around five core components:

- **`AudioSystem`** - Main coordinator for audio playback and asset management
- **`AudioSource`** - Individual audio file representation with category metadata
- **`AudioEvent`** - Event types for triggering UI and game sounds
- **`AudioSettings`** - User-configurable volume controls per category
- **`AudioOutput`** - The port `AudioSystem` plays through: rodio on the default device in game, a recording fake in tests

## 2 Quick Start

```rust
use moho_audio::{AudioSystem, AudioEvent};

// Initialize the audio system (opens default audio device)
let mut audio_system = AudioSystem::new()?;

// Play a sound effect at full volume
audio_system.play_sound("assets/sfx/explosion.wav", 1.0)?;

// Handle event-driven audio (automatically uses cached UI sounds)
audio_system.handle_event(AudioEvent::ButtonClick);

// Play background music (loops automatically)
audio_system.play_music("assets/music/menu_theme.ogg")?;
```

## 3 Event-Driven Audio

Use `AudioEvent` for UI interactions to ensure responsive, low-latency playback:

```rust
use moho_audio::AudioEvent;

// Common UI events (automatically mapped to pre-loaded sounds)
audio_system.handle_event(AudioEvent::ButtonClick);
audio_system.handle_event(AudioEvent::MenuOpen);
audio_system.handle_event(AudioEvent::Error);
```

UI sounds are pre-loaded during initialization to eliminate loading delays.

## 4 Volume Control

Adjust volume per audio category:

```rust
use moho_audio::AudioSettings;

let mut settings = AudioSettings::default();
settings.master_volume = 0.8;      // 80% overall volume
settings.music_volume = 0.5;        // 50% music volume
settings.sfx_volume = 1.0;          // 100% sound effects volume
settings.ui_volume = 0.7;           // 70% UI sound volume

audio_system.update_settings(settings);
```

## 5 Audio Categories

Audio sources are categorized for volume control:

- **Music** - Background music, ambient loops
- **SFX** - In-game sound effects (explosions, footsteps, etc.)
- **UI** - Interface sounds (clicks, menu transitions)
- **Voice** - Dialog and voiceover (future use)

## 6 Supported Formats

- WAV (uncompressed, lowest latency)
- OGG Vorbis (compressed, good quality)
- MP3 (widely supported)
- FLAC (lossless, larger files)

## 7 Performance Characteristics

- **UI Sound Latency**: <5ms (pre-loaded)
- **Other Sound Latency**: 10-50ms (loaded on-demand)
- **Memory Usage**: ~1-5MB for cached UI sounds
- **Background Music**: Streamed (minimal memory overhead)

## 8 Thread Safety

`AudioSystem` is **not** `Send` or `Sync` because `rodio` requires staying on the same thread. Always create and use `AudioSystem` on the main thread.

## 9 Testing

```bash
# Run unit tests
cargo test --package moho_audio

# Test audio playback (requires audio output device)
cargo run --example audio_test
```

## 10 Future Plans

- Audio mixing and dynamic volume adjustment
- Real-time audio effects (reverb, echo)
- Streaming large audio files
- Voice chat integration

