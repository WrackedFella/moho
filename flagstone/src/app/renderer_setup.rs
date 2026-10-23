//! Renderer and UI adapter initialization.
//!
//! This module contains the game-side setup that runs once the engine has created
//! the window and renderer: quality settings, shared mesh handles, the egui adapter
//! and the input dispatcher subscribers.

use std::sync::{Arc, Mutex};
use winit::window::Window;

/// Whether the UI is capturing input right now, checked conservatively: if the
/// lock can't be acquired for any reason, treat that as "capturing" so game
/// input is never forwarded underneath a UI panel by mistake.
fn ui_is_capturing_input(ui_adapter: &Arc<Mutex<flagstone_ui::EguiAdapter>>) -> bool {
    match ui_adapter.try_lock() {
        Ok(a) => a.is_visible(),
        Err(_) => true,
    }
}

/// Applies the saved quality settings, registers the shared sphere and cube mesh
/// handles, builds the egui adapter, connects it to the renderer frame callback, and
/// registers all input-dispatcher subscribers. Populates `app.window`, `app.mesh_handle`,
/// `app.cube_mesh_handle` and `app.ui_adapter`.
pub fn init_renderer_and_ui(
    app: &mut crate::App,
    window: &Arc<Window>,
    renderer: &mut dyn moho_renderer::RendererBackend,
) {
    apply_saved_render_settings(&app.prefs, renderer);

    let (vertices, normals, indices) = flagstone_game::actors::Sphere::unit_sphere_indexed(16, 16);
    let mesh_handle = register_entity_mesh(renderer, &vertices, &normals, &indices);
    let (cube_vertices, cube_normals, cube_indices) =
        flagstone_game::actors::Cube::unit_cube_indexed();
    let cube_mesh_handle =
        register_entity_mesh(renderer, &cube_vertices, &cube_normals, &cube_indices);

    // UI setup
    {
        let adapter = flagstone_ui::build_adapter(
            Some(window.clone()),
            app.event_bus.clone(),
            app.prefs.clone(),
        );
        let ui_adapter = Arc::new(Mutex::new(adapter));

        renderer.set_ui_source(Some(ui_adapter.clone() as moho_renderer::UiSource));

        // Store adapter
        app.ui_adapter = Some(ui_adapter.clone());

        // Register KeybindCapture subscriber at higher priority so it can intercept
        // events while the active screen is actively listening for raw input.
        let kb_adapter = ui_adapter.clone();
        app.dispatcher
            .register(200, move |event: &winit::event::WindowEvent| {
                if let Ok(mut a) = kb_adapter.lock() {
                    // Quick check then forward to screen input handler
                    if a.active_screen_captures_input() {
                        return a.try_handle_screen_input(event);
                    }
                }
                false
            });

        // Register UI adapter as a normal UI subscriber.
        // Menus and the console get every event. During gameplay egui only tracks the
        // pointer and mouse buttons (so HUD widgets can be dragged) and takes a press that
        // lands on a panel.
        let ui_adapter_clone = ui_adapter.clone();
        app.dispatcher
            .register(100, move |event: &winit::event::WindowEvent| {
                if let Ok(mut a) = ui_adapter_clone.lock() {
                    if a.is_visible() {
                        a.handle_winit_event(event)
                    } else {
                        a.handle_gameplay_event(event)
                    }
                } else {
                    false
                }
            });

        // Create channel for simplified input events (e.g., mouse wheel) that
        // the game will process if the UI doesn't consume them.
        let (tx, rx) = std::sync::mpsc::channel::<crate::input_event::InputEvent>();
        app.input.unconsumed_tx = Some(tx.clone());
        app.input.unconsumed_rx = Some(rx);

        // Register a low-priority subscriber that forwards mouse wheel events into
        // the channel so the game can act on them later (e.g., scroll-to-zoom).
        // We only forward when the UI overlay is not visible. Note: the
        // dispatcher already ensures this subscriber is only called when higher
        // priority handlers did not consume the event (i.e., egui didn't want it).
        let ui_adapter_for_wheel = ui_adapter.clone();
        let wheel_tx = tx.clone();
        app.dispatcher
            .register(0, move |event: &winit::event::WindowEvent| {
                let winit::event::WindowEvent::MouseWheel { delta, .. } = event else {
                    return false;
                };
                if ui_is_capturing_input(&ui_adapter_for_wheel) {
                    return false;
                }

                let delta_y = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_x, y) => *y,
                    winit::event::MouseScrollDelta::PixelDelta(p) => p.y as f32,
                };
                crate::forward_wheel_if_allowed(&ui_adapter_for_wheel, &wheel_tx, delta_y)
            });

        // Register a separate low-priority subscriber for the mine action: a
        // left click, not captured by UI, requests a mine attempt via the same
        // channel. It leaves the press unconsumed so it also reaches the action map
        // (`Select`). Kept as its own registration rather than folded into the
        // wheel handler above — each input kind gets its own subscriber so a
        // future input type is a new `register` call, not another branch on
        // an existing one.
        let ui_adapter_for_mine = ui_adapter.clone();
        let mine_tx = tx.clone();
        app.dispatcher
            .register(0, move |event: &winit::event::WindowEvent| {
                let winit::event::WindowEvent::MouseInput {
                    state: winit::event::ElementState::Pressed,
                    button: winit::event::MouseButton::Left,
                    ..
                } = event
                else {
                    return false;
                };
                if ui_is_capturing_input(&ui_adapter_for_mine) {
                    return false;
                }

                if mine_tx
                    .send(crate::input_event::InputEvent::MineRequested)
                    .is_err()
                {
                    tracing::debug!("Mine request dropped: input channel closed");
                }
                false
            });

        // Register a low-priority subscriber for hotbar slot selection:
        // number keys 1-8 (not captured by UI) select the corresponding
        // 0-based slot. Slot 0 (key "1") is the reserved tool slot.
        let ui_adapter_for_slots = ui_adapter.clone();
        let slot_tx = tx.clone();
        app.dispatcher
            .register(0, move |event: &winit::event::WindowEvent| {
                let winit::event::WindowEvent::KeyboardInput {
                    event:
                        winit::event::KeyEvent {
                            physical_key: winit::keyboard::PhysicalKey::Code(keycode),
                            state: winit::event::ElementState::Pressed,
                            repeat: false,
                            ..
                        },
                    ..
                } = event
                else {
                    return false;
                };
                let slot = match keycode {
                    winit::keyboard::KeyCode::Digit1 => 0,
                    winit::keyboard::KeyCode::Digit2 => 1,
                    winit::keyboard::KeyCode::Digit3 => 2,
                    winit::keyboard::KeyCode::Digit4 => 3,
                    winit::keyboard::KeyCode::Digit5 => 4,
                    winit::keyboard::KeyCode::Digit6 => 5,
                    winit::keyboard::KeyCode::Digit7 => 6,
                    winit::keyboard::KeyCode::Digit8 => 7,
                    _ => return false,
                };
                if ui_is_capturing_input(&ui_adapter_for_slots) {
                    return false;
                }

                slot_tx
                    .send(crate::input_event::InputEvent::SlotSelected { slot })
                    .is_ok()
            });
    }

    app.window = Some(window.clone());
    app.mesh_handle = mesh_handle;
    app.cube_mesh_handle = cube_mesh_handle;
}

/// Registers a non-voxel entity mesh: no vertex AO, blocky shading, full sky exposure
/// and no block light, so it draws under sun and ambient alone.
fn register_entity_mesh(
    renderer: &mut dyn moho_renderer::RendererBackend,
    vertices: &[[f32; 3]],
    normals: &[[f32; 3]],
    indices: &[u32],
) -> u32 {
    let n = vertices.len();
    renderer.register_indexed_mesh(
        vertices,
        normals,
        &vec![1.0; n],
        &vec![1; n],
        &vec![1.0; n],
        &vec![[0.0; 3]; n],
        &vec![1.0; n],
        &[],
        indices,
    )
}

/// Applies the saved render settings (quality, Bloom and Brightness) to `renderer` at start.
pub fn apply_saved_render_settings(
    prefs: &moho_core::prefs::Prefs,
    renderer: &mut dyn moho_renderer::RendererBackend,
) {
    renderer.set_shadow_quality(prefs.shadow_quality() as u8);
    renderer.set_ssao_quality(prefs.ssao_quality() as u8);
    renderer.set_bloom_quality(prefs.bloom() as u8);
    renderer.set_brightness(prefs.brightness());
}

pub fn apply_video_settings(prefs: &moho_core::prefs::Prefs, window: &Window) {
    use moho_core::prefs::WindowMode;
    use winit::dpi::PhysicalSize;
    use winit::window::Fullscreen;

    match prefs.window_mode() {
        WindowMode::Fullscreen | WindowMode::Borderless => {
            window.set_fullscreen(Some(Fullscreen::Borderless(None)));
        }
        WindowMode::Windowed => {
            let (w, h) = prefs.window_resolution();
            let _ = window.request_inner_size(PhysicalSize::new(w, h));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_saved_render_settings, register_entity_mesh};
    use moho_core::prefs::Prefs;
    use moho_renderer::{FrameError, InstanceGpu, LightingGpu, MaterialGpu, RendererBackend};

    /// Records the per-vertex channels of each indexed-mesh registration.
    #[derive(Default)]
    struct ChannelRecorder {
        meshes: Vec<RecordedMesh>,
        brightnesses: Vec<f32>,
        shadow_qualities: Vec<u8>,
        bloom_qualities: Vec<u8>,
    }

    struct RecordedMesh {
        vertex_count: usize,
        ao: Vec<f32>,
        block_light_rgb: Vec<[f32; 3]>,
        sky_exposed: Vec<f32>,
    }

    impl RendererBackend for ChannelRecorder {
        fn resize(&mut self, _width: u32, _height: u32) {}
        fn register_mesh(&mut self, _vertices: &[[f32; 3]]) -> u32 {
            unreachable!("entity meshes are indexed")
        }
        fn register_indexed_mesh(
            &mut self,
            vertices: &[[f32; 3]],
            _normals: &[[f32; 3]],
            ao: &[f32],
            _geometry_type: &[u32],
            _light_level: &[f32],
            block_light_rgb: &[[f32; 3]],
            sky_exposed: &[f32],
            _material: &[u32],
            _indices: &[u32],
        ) -> u32 {
            self.meshes.push(RecordedMesh {
                vertex_count: vertices.len(),
                ao: ao.to_vec(),
                block_light_rgb: block_light_rgb.to_vec(),
                sky_exposed: sky_exposed.to_vec(),
            });
            u32::try_from(self.meshes.len()).expect("few meshes")
        }
        fn unregister_mesh(&mut self, _mesh: u32) {}
        fn begin_frame(
            &mut self,
            _camera: (glam::Mat4, glam::Mat4, glam::Vec3),
        ) -> Result<(), FrameError> {
            Ok(())
        }
        fn enqueue_draw(&mut self, _mesh: u32, _instances: &[InstanceGpu]) {}
        fn submit_frame(&mut self) {}
        fn set_materials(&mut self, _materials: &[MaterialGpu]) {}
        fn update_lighting(&mut self, _lighting: LightingGpu) {}
        fn add_point_light(
            &mut self,
            _position: glam::Vec3,
            _color: glam::Vec3,
            _intensity: f32,
            _range: f32,
        ) -> u32 {
            0
        }
        fn remove_light(&mut self, _id: u32) -> bool {
            true
        }
        fn set_light_position(&mut self, _id: u32, _position: glam::Vec3) {}
        fn set_light_enabled(&mut self, _id: u32, _enabled: bool) {}
        fn set_shadow_quality(&mut self, quality: u8) {
            self.shadow_qualities.push(quality);
        }
        fn set_ssao_quality(&mut self, _quality: u8) {}
        fn set_bloom_quality(&mut self, quality: u8) {
            self.bloom_qualities.push(quality);
        }
        fn set_brightness(&mut self, brightness: f32) {
            self.brightnesses.push(brightness);
        }
    }

    #[test]
    fn startup_applies_the_saved_bloom() {
        let mut prefs = Prefs::default();
        prefs.set_bloom(1);
        let mut renderer = ChannelRecorder::default();

        apply_saved_render_settings(&prefs, &mut renderer);

        assert_eq!(renderer.bloom_qualities, vec![1]);
    }

    #[test]
    fn startup_applies_the_saved_brightness() {
        let mut prefs = Prefs::default();
        prefs.set_brightness(1.5);
        prefs.set_shadow_quality(2);
        let mut renderer = ChannelRecorder::default();

        apply_saved_render_settings(&prefs, &mut renderer);

        assert_eq!(renderer.brightnesses, vec![1.5]);
        assert_eq!(renderer.shadow_qualities, vec![2]);
    }

    /// Block light adds to surface colour, so an entity carrying any would draw
    /// brighter than the sun and ambient light it.
    #[test]
    fn entity_meshes_carry_no_block_light_and_full_sky() {
        let (vertices, normals, indices) = flagstone_game::actors::Cube::unit_cube_indexed();
        let mut renderer = ChannelRecorder::default();

        let handle = register_entity_mesh(&mut renderer, &vertices, &normals, &indices);

        assert_eq!(handle, 1);
        let [mesh] = renderer.meshes.as_slice() else {
            panic!("one registration, got {}", renderer.meshes.len());
        };
        assert_eq!(mesh.vertex_count, vertices.len());
        assert_eq!(mesh.block_light_rgb, vec![[0.0; 3]; vertices.len()]);
        assert_eq!(mesh.sky_exposed, vec![1.0; vertices.len()]);
        assert_eq!(mesh.ao, vec![1.0; vertices.len()]);
    }
}
