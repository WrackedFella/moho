//! Renderer and UI adapter initialization.
//!
//! This module contains the logic for creating the wgpu renderer, registering
//! shared mesh handles, wiring up egui, and registering input dispatcher
//! subscribers — all work that was previously inlined in `App::setup_renderer_and_ui`.

use std::sync::{Arc, Mutex};
use winit::window::Window;

/// Initialize the renderer and UI adapter for the given window.
///
/// Creates the wgpu renderer, registers the shared sphere and cube mesh handles,
/// builds the egui adapter, connects it to the renderer frame callback, and
/// registers all input-dispatcher subscribers. On success, `app.window_renderer`
/// and `app.ui_adapter` are populated.
#[allow(dead_code)]
pub fn setup_renderer_and_ui(
    app: &mut crate::App,
    window: Arc<Window>,
) -> Result<(), Box<dyn std::error::Error>> {
    // SAFETY: The renderer requires a &'static Window because Box<dyn RendererBackend>
    // is implicitly 'static. We clone the Arc<Window> before leaking it, so the Arc
    // refcount keeps the Window alive independently of `window`. This is a deliberate
    // one-time leak for the main window, which lives for the entire program lifetime.
    // If winit ever allows window recreation (e.g. fullscreen toggle) this should be
    // replaced by giving RendererBackend a lifetime parameter (TD-09 / TD-06).
    let window_ref: &'static Window = Box::leak(Box::new(window.clone()));
    let mut renderer = moho_renderer::create_renderer(Some(window_ref))?;

    // Apply initial quality settings
    renderer.set_shadow_quality(app.prefs.shadow_quality() as u8);
    renderer.set_ssao_quality(app.prefs.ssao_quality() as u8);

    // Create sphere mesh data using the proper sphere geometry
    let (vertices, normals, indices) = moho_core::actors::Sphere::unit_sphere_indexed(16, 16);
    let ao_data = vec![1.0; vertices.len()]; // Full brightness for non-voxel geometry
    let geo_type = vec![1; vertices.len()]; // Type 1 (blocky/non-voxel)
    let light_level = vec![1.0; vertices.len()]; // Full light for non-voxel geometry
    let mesh_handle = renderer.register_indexed_mesh(
        &vertices,
        &normals,
        &ao_data,
        &geo_type,
        &light_level,
        &indices,
    );

    let (cube_vertices, cube_normals, cube_indices) = moho_core::actors::Cube::unit_cube_indexed();
    let cube_ao = vec![1.0; cube_vertices.len()];
    let cube_geo_type = vec![1; cube_vertices.len()];
    let cube_light_level = vec![1.0; cube_vertices.len()];
    let cube_mesh_handle = renderer.register_indexed_mesh(
        &cube_vertices,
        &cube_normals,
        &cube_ao,
        &cube_geo_type,
        &cube_light_level,
        &cube_indices,
    );

    // UI setup
    {
        let adapter = moho_ui::build_adapter(Some(window.clone()), app.event_bus.clone());
        let ui_adapter = Arc::new(Mutex::new(adapter));

        if let Ok(mut a) = ui_adapter.lock() {
            a.set_surface_format(wgpu::TextureFormat::Bgra8UnormSrgb);
        }

        renderer.set_frame_callback_arc(Some(
            ui_adapter.clone() as Arc<Mutex<dyn moho_renderer::FrameCallback>>
        ));

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
        // Only forward events to egui when menus/console are visible;
        // during gameplay the overlays are passive and don't need input.
        let ui_adapter_clone = ui_adapter.clone();
        app.dispatcher
            .register(100, move |event: &winit::event::WindowEvent| {
                if let Ok(mut a) = ui_adapter_clone.lock() {
                    if a.is_visible() {
                        a.handle_winit_event(event)
                    } else {
                        false
                    }
                } else {
                    false
                }
            });

        // Create channel for simplified input events (e.g., mouse wheel) that
        // the game will process if the UI doesn't consume them.
        let (tx, rx) = crossbeam_channel::unbounded::<crate::input_event::InputEvent>();
        app.input.unconsumed_tx = Some(tx.clone());
        app.input.unconsumed_rx = Some(rx);

        // Prepare a ui_adapter clone to check visibility before forwarding wheel
        let ui_adapter_for_forward = ui_adapter.clone();

        // Register a low-priority subscriber that forwards mouse wheel events into
        // the channel so the game can act on them later (e.g., scroll-to-zoom).
        // We only forward when the UI overlay is not visible. Note: the
        // dispatcher already ensures this subscriber is only called when higher
        // priority handlers did not consume the event (i.e., egui didn't want it).
        app.dispatcher
            .register(0, move |event: &winit::event::WindowEvent| {
                use winit::event::WindowEvent as WEvent;
                if let WEvent::MouseWheel { delta, .. } = event {
                    // Use the conservative try_lock approach: if we cannot acquire the
                    // lock for any reason, treat as UI-visible and do not forward.
                    match ui_adapter_for_forward.try_lock() {
                        Ok(a) if a.is_visible() => return false,
                        Err(_) => return false,
                        _ => {}
                    }

                    // Convert delta to a simple numeric pair and forward via helper
                    let delta_y = match delta {
                        winit::event::MouseScrollDelta::LineDelta(_x, y) => *y,
                        winit::event::MouseScrollDelta::PixelDelta(p) => p.y as f32,
                    };

                    if crate::forward_wheel_if_allowed(&ui_adapter_for_forward, &tx, delta_y) {
                        return true;
                    }
                }
                false
            });
    }

    // Store everything together in the WindowRenderer
    app.window_renderer = Some(crate::WindowRenderer {
        window,
        renderer,
        mesh_handle,
        cube_mesh_handle,
    });

    Ok(())
}
