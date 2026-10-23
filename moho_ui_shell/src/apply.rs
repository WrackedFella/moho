//! The default way a game applies the engine's commands.

use moho_app::FrameContext;
use moho_audio::AudioSource;
use moho_renderer::RendererBackend;

use crate::console::registry::EngineCommand;

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
}
