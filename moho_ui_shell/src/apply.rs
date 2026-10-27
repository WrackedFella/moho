//! The default way a game applies the engine's commands.

use moho_app::{FrameContext, WindowMode};
use moho_audio::{AudioSettings, AudioSource};
use moho_core::prefs::WindowMode as SavedMode;
use moho_renderer::RendererBackend;

use crate::console::registry::EngineCommand;
use crate::settings::EngineSettings;

/// Applies `command`: `Quit` asks the runner to exit, as closing the window does.
pub fn apply_engine_command(command: &EngineCommand, ctx: &mut FrameContext<'_>) {
    match command {
        EngineCommand::Quit => ctx.request_exit(),
        EngineCommand::DebugView(mode) => {
            with_renderer(command, ctx, |r| r.set_debug_view(Some(*mode)));
        }
        EngineCommand::ShadowQuality(quality) => {
            with_renderer(command, ctx, |r| r.set_shadow_quality(*quality));
        }
        EngineCommand::SsaoQuality(quality) => {
            with_renderer(command, ctx, |r| r.set_ssao_quality(*quality));
        }
        EngineCommand::BloomQuality(quality) => {
            with_renderer(command, ctx, |r| r.set_bloom_quality(*quality));
        }
        EngineCommand::ToneMap(enabled) => {
            with_renderer(command, ctx, |r| r.set_tone_map(*enabled));
        }
        EngineCommand::PlaySoundAt {
            path,
            position,
            volume,
        } => {
            if let Some(audio) = ctx.audio() {
                let source = AudioSource::new(path)
                    .with_volume(*volume)
                    .with_position(position.to_array());
                if let Err(error) = audio.play_audio_source(&source) {
                    tracing::warn!(path = %path.display(), %error, "console sound failed to play");
                }
            } else {
                tracing::debug!(?command, "no audio; engine command dropped");
            }
        }
    }
}

/// Applies the engine's player settings: shadow and ambient-occlusion quality to the renderer,
/// the volumes to the audio system, and the window mode and size as runner requests. The look
/// rows are the game's, so they are left alone.
pub fn apply_engine_settings(settings: &EngineSettings, ctx: &mut FrameContext<'_>) {
    if let Some(renderer) = ctx.renderer() {
        renderer.set_shadow_quality(quality_level(settings.shadow_quality));
        renderer.set_ssao_quality(quality_level(settings.ssao_quality));
    }
    if let Some(audio) = ctx.audio() {
        let applied = audio_settings_for(settings, audio.settings());
        audio.set_settings(applied);
    }
    ctx.set_window_mode(match settings.window_mode {
        SavedMode::Windowed => WindowMode::Windowed,
        SavedMode::Fullscreen | SavedMode::Borderless => WindowMode::Borderless,
    });
    ctx.set_window_size(settings.resolution.0, settings.resolution.1);
}

/// `current` with the sound-effect, music and UI volumes of `settings`; the master and voice
/// levels and the mute stay as they were.
pub fn audio_settings_for(settings: &EngineSettings, current: &AudioSettings) -> AudioSettings {
    current
        .clone()
        .with_sound_effects_volume(settings.sound_effect_volume / VOLUME_SCALE)
        .with_music_volume(settings.music_volume / VOLUME_SCALE)
        .with_ui_volume(settings.ui_volume / VOLUME_SCALE)
}

/// The top of the prefs volume scale, which the audio system takes as 1.
const VOLUME_SCALE: f32 = 10.0;

fn quality_level(quality: u32) -> u8 {
    u8::try_from(quality).unwrap_or(u8::MAX)
}

fn with_renderer(
    command: &EngineCommand,
    ctx: &mut FrameContext<'_>,
    apply: impl FnOnce(&mut dyn RendererBackend),
) {
    if let Some(renderer) = ctx.renderer() {
        apply(renderer);
    } else {
        tracing::debug!(?command, "no renderer; engine command dropped");
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use glam::{Mat4, Vec3};
    use moho_app::GameClock;
    use moho_render_api::{InstanceGpu, MaterialGpu};
    use moho_renderer::{FrameError, LightingGpu, RendererBackend};

    use super::*;

    /// Records the engine's renderer settings; draws are never made.
    #[derive(Default)]
    struct FakeRenderer {
        shadow: Vec<u8>,
        ssao: Vec<u8>,
        debug_view: Vec<Option<u32>>,
        tone_map: Vec<bool>,
        bloom: Vec<u8>,
    }

    impl RendererBackend for FakeRenderer {
        fn resize(&mut self, _width: u32, _height: u32) {}
        fn register_mesh(&mut self, _vertices: &[[f32; 3]]) -> u32 {
            0
        }
        fn register_indexed_mesh(
            &mut self,
            _vertices: &[[f32; 3]],
            _normals: &[[f32; 3]],
            _ao: &[f32],
            _geometry_type: &[u32],
            _light_level: &[f32],
            _block_light_rgb: &[[f32; 3]],
            _sky_exposed: &[f32],
            _material: &[u32],
            _indices: &[u32],
        ) -> u32 {
            0
        }
        fn unregister_mesh(&mut self, _mesh: u32) {}
        fn begin_frame(&mut self, _camera: (Mat4, Mat4, Vec3)) -> Result<(), FrameError> {
            Ok(())
        }
        fn enqueue_draw(&mut self, _mesh: u32, _instances: &[InstanceGpu]) {}
        fn submit_frame(&mut self) {}
        fn set_materials(&mut self, _materials: &[MaterialGpu]) {}
        fn update_lighting(&mut self, _lighting: LightingGpu) {}
        fn add_point_light(&mut self, _p: Vec3, _c: Vec3, _intensity: f32, _range: f32) -> u32 {
            0
        }
        fn remove_light(&mut self, _id: u32) -> bool {
            true
        }
        fn set_light_position(&mut self, _id: u32, _position: Vec3) {}
        fn set_light_enabled(&mut self, _id: u32, _enabled: bool) {}
        fn set_shadow_quality(&mut self, quality: u8) {
            self.shadow.push(quality);
        }
        fn set_ssao_quality(&mut self, quality: u8) {
            self.ssao.push(quality);
        }
        fn set_debug_view(&mut self, mode: Option<u32>) {
            self.debug_view.push(mode);
        }
        fn set_tone_map(&mut self, enabled: bool) {
            self.tone_map.push(enabled);
        }
        fn set_bloom_quality(&mut self, quality: u8) {
            self.bloom.push(quality);
        }
    }

    #[test]
    fn bloom_quality_reaches_the_renderer() {
        let clock = GameClock::default();
        let mut renderer = FakeRenderer::default();
        {
            let backend: &mut dyn RendererBackend = &mut renderer;
            let mut ctx = FrameContext::new(Duration::from_millis(16), &clock, Some(backend), None);

            apply_engine_command(&EngineCommand::BloomQuality(0), &mut ctx);
            apply_engine_command(&EngineCommand::BloomQuality(2), &mut ctx);
        }

        assert_eq!(renderer.bloom, vec![0, 2]);
        assert!(renderer.shadow.is_empty());
        assert!(renderer.ssao.is_empty());
        assert!(renderer.tone_map.is_empty());
        assert!(renderer.debug_view.is_empty());
    }

    #[test]
    fn tone_map_toggle_reaches_the_renderer() {
        let clock = GameClock::default();
        let mut renderer = FakeRenderer::default();
        {
            let backend: &mut dyn RendererBackend = &mut renderer;
            let mut ctx = FrameContext::new(Duration::from_millis(16), &clock, Some(backend), None);

            apply_engine_command(&EngineCommand::ToneMap(false), &mut ctx);
            apply_engine_command(&EngineCommand::ToneMap(true), &mut ctx);
        }

        assert_eq!(renderer.tone_map, vec![false, true]);
        assert!(renderer.shadow.is_empty());
        assert!(renderer.ssao.is_empty());
        assert!(renderer.debug_view.is_empty());
    }

    #[test]
    fn quality_commands_reach_the_renderer() {
        let clock = GameClock::default();
        let mut renderer = FakeRenderer::default();
        {
            let backend: &mut dyn RendererBackend = &mut renderer;
            let mut ctx = FrameContext::new(Duration::from_millis(16), &clock, Some(backend), None);

            apply_engine_command(&EngineCommand::ShadowQuality(0), &mut ctx);
            apply_engine_command(&EngineCommand::SsaoQuality(4), &mut ctx);
        }

        assert_eq!(renderer.shadow, vec![0]);
        assert_eq!(renderer.ssao, vec![4]);
        assert!(renderer.debug_view.is_empty());
    }

    #[test]
    fn debug_view_sets_the_renderer_override() {
        let clock = GameClock::default();
        let mut renderer = FakeRenderer::default();
        {
            let backend: &mut dyn RendererBackend = &mut renderer;
            let mut ctx = FrameContext::new(Duration::from_millis(16), &clock, Some(backend), None);

            apply_engine_command(&EngineCommand::DebugView(3), &mut ctx);
        }

        assert_eq!(renderer.debug_view, vec![Some(3)]);
        assert!(renderer.shadow.is_empty());
        assert!(renderer.ssao.is_empty());
    }

    #[test]
    fn commands_without_renderer_or_audio_are_dropped() {
        let clock = GameClock::default();
        let mut ctx = FrameContext::new(Duration::from_millis(16), &clock, None, None);
        let commands = [
            EngineCommand::DebugView(3),
            EngineCommand::ShadowQuality(0),
            EngineCommand::SsaoQuality(4),
            EngineCommand::BloomQuality(2),
            EngineCommand::ToneMap(true),
            EngineCommand::PlaySoundAt {
                path: "searchlight/assets/rifle_shot.wav".into(),
                position: Vec3::new(2.0, 0.0, 0.0),
                volume: 1.0,
            },
        ];

        for command in &commands {
            apply_engine_command(command, &mut ctx);
        }

        assert!(!ctx.exit_requested());
    }

    #[test]
    fn quit_requests_exit() {
        let clock = GameClock::default();
        let mut ctx = FrameContext::new(Duration::from_millis(16), &clock, None, None);
        assert!(!ctx.exit_requested());

        apply_engine_command(&EngineCommand::Quit, &mut ctx);

        assert!(ctx.exit_requested());
    }

    mod engine_settings {
        use moho_app::WindowMode;
        use moho_audio::AudioCategory;
        use moho_core::prefs::WindowMode as Saved;

        use super::*;

        fn settings() -> EngineSettings {
            EngineSettings {
                mouse_sensitivity: 2.5,
                mouse_smoothing: true,
                invert_mouse_y: true,
                sound_effect_volume: 4.0,
                music_volume: 10.0,
                ui_volume: 2.5,
                shadow_quality: 2,
                ssao_quality: 4,
                window_mode: Saved::Windowed,
                resolution: (1280, 720),
            }
        }

        fn volume(audio: &AudioSettings, category: AudioCategory) -> f32 {
            audio.effective_volume(&category)
        }

        fn close(a: f32, b: f32) -> bool {
            (a - b).abs() < 1e-6
        }

        #[test]
        fn audio_settings_for_sets_the_three_volumes_and_keeps_the_rest() {
            let current = AudioSettings::new()
                .with_master_volume(0.5)
                .with_voice_volume(0.2)
                .with_sound_effects_volume(1.0)
                .with_music_volume(1.0)
                .with_ui_volume(1.0);
            let muted = current.clone().set_muted(true);

            let audio = audio_settings_for(&settings(), &current);
            let silenced = audio_settings_for(&settings(), &muted);

            let sfx = volume(&audio, AudioCategory::SoundEffect);
            let music = volume(&audio, AudioCategory::Music);
            let ui = volume(&audio, AudioCategory::UserInterface);
            let voice = volume(&audio, AudioCategory::Voice);
            assert!(close(sfx, 0.5 * 0.4), "sound effects {sfx}");
            assert!(close(music, 0.5 * 1.0), "music {music}");
            assert!(close(ui, 0.5 * 0.25), "ui {ui}");
            assert!(close(voice, 0.5 * 0.2), "voice {voice}");
            for category in [
                AudioCategory::SoundEffect,
                AudioCategory::Music,
                AudioCategory::UserInterface,
                AudioCategory::Voice,
            ] {
                assert_eq!(volume(&silenced, category), 0.0, "the mute is kept");
            }
        }

        #[test]
        fn apply_engine_settings_sets_shadow_and_ssao_quality() {
            let clock = GameClock::default();
            let mut renderer = FakeRenderer::default();
            {
                let backend: &mut dyn RendererBackend = &mut renderer;
                let mut ctx =
                    FrameContext::new(Duration::from_millis(16), &clock, Some(backend), None);

                apply_engine_settings(&settings(), &mut ctx);
            }

            assert_eq!(renderer.shadow, vec![2]);
            assert_eq!(renderer.ssao, vec![4]);
            assert!(renderer.bloom.is_empty(), "bloom is not a settings row");
            assert!(renderer.tone_map.is_empty());
            assert!(renderer.debug_view.is_empty());
        }

        #[test]
        fn apply_engine_settings_requests_window_mode_and_size() {
            let clock = GameClock::default();
            let cases = [
                (Saved::Windowed, WindowMode::Windowed, (1280, 720)),
                (Saved::Fullscreen, WindowMode::Borderless, (1920, 1080)),
                (Saved::Borderless, WindowMode::Borderless, (2560, 1440)),
            ];

            for (saved, expected_mode, size) in cases {
                let mut ctx = FrameContext::new(Duration::from_millis(16), &clock, None, None);
                let settings = EngineSettings {
                    window_mode: saved,
                    resolution: size,
                    ..settings()
                };

                apply_engine_settings(&settings, &mut ctx);

                assert_eq!(
                    ctx.requested_window_mode(),
                    Some(expected_mode),
                    "{saved:?}"
                );
                assert_eq!(ctx.requested_window_size(), Some(size), "{saved:?}");
            }
        }

        #[test]
        fn apply_engine_settings_without_a_renderer_or_audio_still_requests_the_window() {
            let clock = GameClock::default();
            let mut ctx = FrameContext::new(Duration::from_millis(16), &clock, None, None);

            apply_engine_settings(&settings(), &mut ctx);

            assert_eq!(ctx.requested_window_mode(), Some(WindowMode::Windowed));
            assert_eq!(ctx.requested_window_size(), Some((1280, 720)));
            assert!(!ctx.exit_requested());
        }
    }
}
