#[cfg(feature = "backend-wgpu")]
use engine_core::actors::Cube;
use engine_core::actors::Sphere;
use engine_core::scene_builders::random_scene;
// glam types are used via fully-qualified names where needed
// Renderer has been moved int                                            app_state.window.set_cursor_visible(true); the `engine_renderer` crate to provide a
// reusable rendering API.
#[cfg(feature = "backend-wgpu")]
use engine_renderer::create_renderer;
use legion::World;
use legion::query::IntoQuery;

// `winit` is an optional dependency used by the GPU backends. Guard its
// usage behind the `backend-wgpu` feature. When the backend is disabled we
// fall back to a single-frame placeholder renderer to keep builds fast.
#[cfg(feature = "backend-wgpu")]
use std::time::{Duration, Instant};
#[cfg(feature = "backend-wgpu")]
use winit::event::{Event, StartCause, WindowEvent};
#[cfg(feature = "backend-wgpu")]
use winit::event_loop::{ControlFlow, EventLoop};
#[cfg(feature = "backend-wgpu")]
use winit::window::WindowAttributes;

// a component is any type that is 'static, sized, send and sync
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq)]
struct Position {
    x: f32,
    y: f32,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq)]
struct Velocity {
    dx: f32,
    dy: f32,
}

fn main() {
    // Initialize logging. Use RUST_LOG to control verbosity, default to info.
    // Example: set RUST_LOG=debug to see more verbose renderer logs.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let mut world = World::default();
    // Parse CLI args for --scene <path>. Default to ./scene.bin
    let args: Vec<String> = std::env::args().collect();
    let scene_path_buf = if let Some(i) = args.iter().position(|a| a == "--scene" || a == "-s") {
        args.get(i + 1)
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("scene.bin"))
    } else {
        std::path::PathBuf::from("scene.bin")
    };

    // Create the Scene manager once and try to load the persisted scene.
    let mut scene = engine_renderer::Scene::new();
    load_or_generate_scene(&mut scene, &mut world, &scene_path_buf);

    // create a simple PlayerController entity and derive the camera from it
    let start_pos = glam::Vec3::new(13.0, 2.0, 3.0);
    let mut controller = engine_core::controller::PlayerController::new(start_pos);
    // Set initial yaw and pitch to look towards the origin
    // direction = (-13, -2, -3), normalized ≈ (-0.94, -0.14, -0.22)
    // yaw = atan2(-0.94, -0.22) ≈ -1.76, pitch = asin(-0.14) ≈ -0.14
    controller.yaw = -1.76;
    controller.pitch = -0.14;
    world.push((
        controller,
        engine_core::controller::ControllerInput::default(),
    ));
    // initial camera value (will be computed from controller each frame)
    #[allow(unused_mut)]
    let mut camera = make_camera();

    // Helper to make the camera clearer - kept local for now.
    fn make_camera() -> (glam::Mat4, glam::Mat4, glam::Vec3) {
        let eye = glam::Vec3::new(13.0, 2.0, 3.0);
        let center = glam::Vec3::new(0.0, 0.0, 0.0);
        let up = glam::Vec3::new(0.0, 1.0, 0.0);
        let view = glam::Mat4::look_at_rh(eye, center, up);
        let proj = glam::Mat4::perspective_rh(45f32.to_radians(), 16.0 / 9.0, 0.1f32, 100.0f32);
        (view, proj, eye)
    }

    // If `backend-wgpu` (and therefore `winit`) is enabled, create the event loop
    // and run a proper per-frame loop. Otherwise use the placeholder
    // renderer and run a single frame to keep builds fast.
    #[cfg(feature = "backend-wgpu")]
    #[allow(deprecated)]
    {
        let event_loop = match EventLoop::new() {
            Ok(el) => el,
            Err(e) => {
                log::error!("Failed to create event loop: {:?}", e);
                return;
            }
        };
        let window_attributes = WindowAttributes::default();

        // Create the window on the main thread then move ownership into the renderer.
        // The renderer now owns the Window and exposes control methods
        // (request_redraw, set_cursor_visible, set_cursor_grab).
        #[allow(deprecated)]
        let window = match event_loop.create_window(window_attributes.clone()) {
            Ok(w) => w,
            Err(e) => {
                log::error!("Failed to create window: {:?}", e);
                return;
            }
        };
        window.set_title("moho - wgpu renderer");

        // Wrap the window in an Arc so we can share ownership between the
        // renderer and the event loop without unsafe or leaking memory.
        use std::sync::Arc;
        let arc_window = Arc::new(window);

        let mut renderer = match create_renderer(Some(&*arc_window)) {
            Ok(r) => r,
            Err(e) => {
                log::error!("Failed to create renderer: {}", e);
                return;
            }
        };

        // Register meshes up-front.
        let (vertices, normals, indices) = collect_indexed_vertices(&mut world);
        let mesh_handle = renderer.register_indexed_mesh(&vertices, &normals, &indices);
        let (cube_vertices, cube_normals, cube_indices) = Cube::unit_cube_indexed();
        let cube_mesh_handle =
            renderer.register_indexed_mesh(&cube_vertices, &cube_normals, &cube_indices);

        // AppState holds the renderer trait object; the renderer now owns
        // an `Arc<Window>` so the trait object can be `'static`.
        struct AppState<'a> {
            renderer: Box<dyn engine_renderer::RendererBackend + 'a>,
            mesh_handle: u32,
            cube_mesh_handle: u32,
            cursor_grabbed: bool,
            // Remember whether the cursor was grabbed before showing an overlay
            // so we can restore it when the overlay is closed.
            was_cursor_grabbed: bool,
            // keep the Window so event handling can manipulate cursor
            window: std::sync::Arc<winit::window::Window>,
        }

        // Create the UI adapter (optional feature)
        #[cfg(feature = "ui-egui")]
        // Priority: Medium
        //     TODO: Consider unifying UI adapter construction behind a
        //           small factory or builder to simplify initialization and
        //           make testing easier (avoids conditional compilation at call sites).
        let (ui_adapter, ui_receiver) = moho_ui::build_adapter(Some(Arc::clone(&arc_window)));
        #[cfg(feature = "ui-egui")]
        // Wrap the adapter in an Arc<Mutex<..>> so we can register it safely
        // with the renderer and call into it from the event loop.
        let ui_adapter = std::sync::Arc::new(std::sync::Mutex::new(ui_adapter));

        // When the `ui-egui` feature is not enabled we don't create a UI
        // adapter. All references to the adapter are already feature-gated
        // so this keeps the main binary independent of the optional crate.

        let mut app_state = AppState {
            renderer,
            mesh_handle,
            cube_mesh_handle,
            cursor_grabbed: false,
            was_cursor_grabbed: false,
            window: Arc::clone(&arc_window),
        };

        // If UI adapter is present, give it the surface format (so the
        // egui GPU renderer can be initialized) and register it with the
        // renderer so the renderer will call it during frame finalization.
        #[cfg(feature = "ui-egui")]
        {
            if let Some(fmt) = app_state.renderer.surface_format() {
                // lock the adapter briefly to set the surface format
                if let Ok(mut a) = ui_adapter.lock() {
                    a.set_surface_format(fmt);
                }
            }
            // Register the adapter with the renderer using the safe Arc<Mutex<..>> API.
            app_state
                .renderer
                .set_frame_callback_arc(Some(std::sync::Arc::clone(&ui_adapter)
                    as std::sync::Arc<std::sync::Mutex<dyn engine_renderer::FrameCallback>>));
        }
        // use winit::window::CursorGrabMode;

        // Frame timing: aim for ~60 FPS.
        let frame_duration = Duration::from_secs_f64(1.0 / 60.0);
        let mut last_frame = Instant::now();
        // Simulation timestep (fixed) and accumulator for decoupled updates.
        let sim_dt = frame_duration; // simulation uses the same fixed timestep by default
        let mut sim_acc = Duration::from_secs(0);
        // Track wall-clock time to accumulate simulation time.
        let mut last_time = Instant::now();
        // simple frame counter for debug correlation with renderer logs
        let mut frame_count: u64 = 0;

        // Run the event loop. The leaked window will remain valid for the
        // duration of the event loop. It will be dropped when the process
        // terminates (acceptable for this demo app) or can be handled more
        // explicitly if desired.
        let _ = event_loop.run(move |event, active_event_loop| {
            match event {
                Event::NewEvents(start_cause) => {
                    if matches!(start_cause, StartCause::Init) {
                        // On Init, perform an initial render pass so the window
                        // displays content immediately.
                        scene.render(
                            &mut *app_state.renderer,
                            &world,
                            app_state.mesh_handle,
                            app_state.cube_mesh_handle,
                            camera,
                        );

                        active_event_loop.set_control_flow(ControlFlow::Wait);
                    } else if matches!(
                        start_cause,
                        StartCause::ResumeTimeReached { .. } | StartCause::Poll
                    ) {
                        // This is called when the event loop wakes at our scheduled time.
                        // Mirror the previous MainEventsCleared behavior: step simulation and
                        // request a redraw when appropriate.
                        let now = Instant::now();
                        let elapsed = now - last_time;
                        last_time = now;

                        // Accumulate time for simulation updates.
                        sim_acc += elapsed;
                        let mut sim_steps = 0;
                        while sim_acc >= sim_dt && sim_steps < 20 {
                            // Advance simulation by exactly sim_dt.
                            simulate(&mut world, sim_dt);
                            sim_acc -= sim_dt;
                            sim_steps += 1;
                        }

                        // Now handle rendering on the fixed render cadence.
                        if now >= last_frame + frame_duration {
                            // Advance the render timestamp by one fixed frame.
                            last_frame += frame_duration;
                            // Request a redraw via the Window (renderer.request_redraw is a no-op)
                            app_state.window.request_redraw();
                            // Process UI events if the ui feature is enabled
                            #[cfg(feature = "ui-egui")]
                            {
                                use moho_ui::UiEvent;
                                // Drain events non-blockingly
                                while let Ok(ev) = ui_receiver.try_recv() {
                                    match ev {
                                        UiEvent::LoadScene(path) => {
                                            log::info!("UI requested load scene: {:?}", path);
                                            // Attempt to load scene into world and scene manager
                                            if let Err(e) = scene.load_from_file(&path, &mut world)
                                            {
                                                log::warn!(
                                                    "Failed to load scene from {:?}: {}",
                                                    path,
                                                    e
                                                );
                                            }
                                        }
                                        UiEvent::Exit => {
                                            log::info!("UI requested exit");
                                            active_event_loop.exit();
                                        }
                                        UiEvent::OverlayToggled(visible) => {
                                            // Adapter requested overlay visibility change.
                                            // When showing an overlay we want to release
                                            // the application's cursor grab so the OS
                                            // cursor can be presented for UI interaction.
                                            // When hiding the overlay, restore the
                                            // previous grab state if it was grabbed.
                                            use winit::window::CursorGrabMode;
                                            log::info!("Overlay visibility -> {}", visible);
                                            if visible {
                                                // Save previous grabbed state and release
                                                app_state.was_cursor_grabbed =
                                                    app_state.cursor_grabbed;
                                                if app_state.cursor_grabbed {
                                                    let _ = app_state
                                                        .window
                                                        .set_cursor_grab(CursorGrabMode::None);
                                                    app_state.cursor_grabbed = false;
                                                }
                                                app_state.window.set_cursor_visible(true);
                                            } else if app_state.was_cursor_grabbed {
                                                // Overlay hidden: restore previous grab if needed
                                                let r = app_state
                                                    .window
                                                    .set_cursor_grab(CursorGrabMode::Locked)
                                                    .or_else(|_| {
                                                        app_state.window.set_cursor_grab(
                                                            CursorGrabMode::Confined,
                                                        )
                                                    });
                                                log::debug!("Restore grab -> {:?}", r);
                                                if r.is_ok() {
                                                    app_state.cursor_grabbed = true;
                                                    app_state.window.set_cursor_visible(false);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            // Schedule next wake at the next fixed step boundary.
                            let next = last_frame + frame_duration;
                            active_event_loop.set_control_flow(ControlFlow::WaitUntil(next));
                        } else {
                            let next = last_frame + frame_duration;
                            active_event_loop.set_control_flow(ControlFlow::WaitUntil(next));
                        }
                    }
                }
                Event::WindowEvent { event, .. } => {
                    // Forward input events to the UI adapter when available
                    #[cfg(feature = "ui-egui")]
                    {
                        use winit::event::WindowEvent as WEvent;
                        // Forward pointer, modifier and keyboard events so the
                        // embedded UI can react (for example Escape to toggle
                        // the overlay). The adapter will ignore events it
                        // doesn't care about.
                        match &event {
                            WEvent::CursorMoved { .. }
                            | WEvent::MouseInput { .. }
                            | WEvent::ModifiersChanged(_)
                            | WEvent::KeyboardInput { .. } => {
                                if let Ok(mut a) = ui_adapter.lock() {
                                    a.handle_winit_event(&event);
                                }
                            }
                            _ => {}
                        }
                    }
                    // Process any UI events immediately so overlay visibility
                    // changes take effect without waiting for the NewEvents drain.
                    #[cfg(feature = "ui-egui")]
                    {
                        use moho_ui::UiEvent;
                        while let Ok(ev) = ui_receiver.try_recv() {
                            match ev {
                                UiEvent::LoadScene(path) => {
                                    log::info!("UI requested load scene: {:?}", path);
                                    if let Err(e) = scene.load_from_file(&path, &mut world) {
                                        log::warn!("Failed to load scene from {:?}: {}", path, e);
                                    }
                                }
                                UiEvent::Exit => {
                                    log::info!("UI requested exit");
                                    active_event_loop.exit();
                                }
                                UiEvent::OverlayToggled(visible) => {
                                    use winit::window::CursorGrabMode;
                                    log::info!("Overlay visibility -> {}", visible);
                                    if visible {
                                        app_state.was_cursor_grabbed = app_state.cursor_grabbed;
                                        if app_state.cursor_grabbed {
                                            let _ = app_state
                                                .window
                                                .set_cursor_grab(CursorGrabMode::None);
                                            app_state.cursor_grabbed = false;
                                        }
                                        app_state.window.set_cursor_visible(true);
                                        // Ensure the window redraws so the UI is rendered now
                                        app_state.window.request_redraw();
                                    } else if app_state.was_cursor_grabbed {
                                        let r = app_state
                                            .window
                                            .set_cursor_grab(CursorGrabMode::Locked)
                                            .or_else(|_| {
                                                app_state
                                                    .window
                                                    .set_cursor_grab(CursorGrabMode::Confined)
                                            });
                                        log::debug!("Restore grab -> {:?}", r);
                                        if r.is_ok() {
                                            app_state.cursor_grabbed = true;
                                            app_state.window.set_cursor_visible(false);
                                            // Update the window so UI hides immediately
                                            app_state.window.request_redraw();
                                        }
                                    }
                                }
                            }
                        }
                    }
                    match event {
                        WindowEvent::CloseRequested => {
                            active_event_loop.exit();
                        }
                        WindowEvent::Resized(size) => {
                            app_state.renderer.resize(size.width, size.height);
                        }
                        // Keyboard input is forwarded to the UI adapter above.
                        // We intentionally do not toggle cursor grab here on
                        // Escape because the embedded UI handles showing the
                        // cursor and enabling interaction without requiring the
                        // application to release its grab. This avoids the app
                        // "losing" the cursor when the menu is opened.
                        WindowEvent::KeyboardInput { .. } => {}
                        WindowEvent::Focused(gained) => {
                            if gained && !app_state.cursor_grabbed {
                                // If the UI overlay is visible, don't force a
                                // cursor grab here because the user may be
                                // interacting with the UI. Query the adapter's
                                // visibility state when present.
                                #[cfg(feature = "ui-egui")]
                                let ui_visible = match ui_adapter.lock() {
                                    Ok(a) => a.is_visible(),
                                    Err(_) => false,
                                };
                                #[cfg(not(feature = "ui-egui"))]
                                let ui_visible = false;

                                if !ui_visible {
                                    let r = app_state
                                        .window
                                        .set_cursor_grab(winit::window::CursorGrabMode::Locked)
                                        .or_else(|_| {
                                            app_state.window.set_cursor_grab(
                                                winit::window::CursorGrabMode::Confined,
                                            )
                                        });
                                    log::debug!("Focused -> window.set_cursor_grab -> {:?}", r);
                                    if r.is_ok() {
                                        app_state.cursor_grabbed = true;
                                        app_state.window.set_cursor_visible(false);
                                    }
                                } else {
                                    log::debug!(
                                        "Focused event ignored because UI overlay is visible"
                                    );
                                }
                            }
                        }
                        WindowEvent::RedrawRequested => {
                            // NOTE: in winit 0.30 RedrawRequested may also arrive as a
                            // top-level Event::RedrawRequested(window_id). We still
                            // handle the WindowEvent form when present for
                            // completeness.
                            let mut qc = <&mut engine_core::controller::PlayerController>::query();
                            if let Some(pc) = qc.iter_mut(&mut world).next() {
                                camera = engine_core::controller::controller_to_camera(pc);
                            }
                            // Debug: print camera eye and frame id to correlate with renderer logs
                            log::debug!(
                                "[main] frame={} WindowEvent::RedrawRequested -> camera_eye={:?}",
                                frame_count,
                                camera.2
                            );
                            scene.render(
                                &mut *app_state.renderer,
                                &world,
                                app_state.mesh_handle,
                                app_state.cube_mesh_handle,
                                camera,
                            );

                            // After a frame has been rendered the renderer will
                            // submit the encoder to the GPU queue. If we created
                            // a staging belt for egui uploads we must recall it
                            // after the submission so resources are reclaimed.
                            #[cfg(feature = "ui-egui")]
                            {
                                // Call recall on the adapter to allow the staging belt
                                // to free its memory. This must be done after queue.submit().
                                if let Ok(mut a) = ui_adapter.lock() {
                                    a.recall_staging_belt();
                                }
                            }
                            frame_count = frame_count.wrapping_add(1);
                            active_event_loop.set_control_flow(ControlFlow::Wait);
                        }
                        _ => {}
                    }
                }
                Event::DeviceEvent {
                    event: winit::event::DeviceEvent::MouseMotion { delta },
                    ..
                } => {
                    // Only apply mouse motion when the cursor is grabbed for
                    // FPS look and the UI overlay is not visible. Use the
                    // atomic flag to avoid locking the adapter here.
                    let overlay_visible = {
                        #[cfg(feature = "ui-egui")]
                        {
                            use moho_ui::UI_OVERLAY_VISIBLE;
                            UI_OVERLAY_VISIBLE.load(std::sync::atomic::Ordering::SeqCst)
                        }
                        #[cfg(not(feature = "ui-egui"))]
                        {
                            false
                        }
                    };

                    let cursor_is_grabbed = app_state.cursor_grabbed;
                    if !cursor_is_grabbed || overlay_visible {
                        // ignore mouse motion when cursor is not grabbed or
                        // when the UI overlay is visible.
                    } else {
                        log::debug!("MouseMotion delta={:?}", delta);
                        // Apply mouse motion as small yaw/pitch deltas
                        let mut q = <&mut engine_core::controller::ControllerInput>::query();
                        if let Some(ci) = q.iter_mut(&mut world).next() {
                            let sensitivity = 0.0025f32;
                            ci.yaw_delta += -(delta.0 as f32) * sensitivity;
                            ci.pitch_delta += -(delta.1 as f32) * sensitivity;
                        }

                        // For debugging: run one simulation step immediately after
                        // applying controller input so we can observe the effect
                        // in the simulate() debug prints. This is temporary and
                        // helps verify that mouse deltas are being consumed.
                        simulate(&mut world, sim_dt);
                        // Ensure the renderer draws the updated camera state.
                        // Request a redraw via the owned window so the renderer
                        // will obtain an up-to-date SurfaceTexture during its
                        // next render call.
                        app_state.window.request_redraw();
                    }
                }
                // MainEventsCleared is no longer a top-level Event variant in winit 0.30.
                // Its previous responsibilities are handled via StartCause::ResumeTimeReached
                // in the NewEvents branch above.
                _ => {}
            }
        });

        // event_loop.run returned due to exit(); nothing special to do here
        // as the application retains ownership of the Window via Arc.
    }

    #[cfg(not(feature = "backend-wgpu"))]
    {
        // Use the compatibility wrapper so this code compiles regardless of
        // whether the workspace was built with `backend-wgpu` enabled.
        let mut renderer = match engine_renderer::create_renderer_any(None) {
            Ok(r) => r,
            Err(e) => {
                log::error!("Failed to create placeholder renderer: {}", e);
                return;
            }
        };
        // Register indexed mesh once with placeholder renderer (no-op) and use render_mesh
        let (vertices, normals, indices) = collect_indexed_vertices(&mut world);
        let mesh_handle = renderer.register_indexed_mesh(&vertices, &normals, &indices);
        let instances = collect_instances(&mut world);
        renderer.render_mesh(mesh_handle, &instances, camera, true);
        log::info!("Rendered one frame (exiting).");
    }
}

fn load_or_generate_scene(
    scene: &mut engine_renderer::Scene,
    world: &mut World,
    scene_path: &std::path::Path,
) {
    if scene_path.exists() {
        match scene.load_from_file(scene_path, world) {
            Ok(_) => log::info!("Loaded scene from scene.bin"),
            Err(e) => {
                log::warn!("Failed to load scene.bin: {}. Generating new scene.", e);
                random_scene(world);
                if let Err(e) = scene.save_to_file(scene_path, world) {
                    log::warn!("Failed to save generated scene: {}", e);
                }
            }
        }
    } else {
        random_scene(world);
        if let Err(e) = scene.save_to_file(scene_path, world) {
            log::warn!("Failed to save generated scene: {}", e);
        }
    }
}

#[allow(dead_code)]
fn collect_instances(world: &mut World) -> Vec<engine_core::actors::InstanceGpu> {
    let mut out: Vec<engine_core::actors::InstanceGpu> = Vec::new();
    // Query all entities that have a Sphere component
    let mut q = <&Sphere>::query();
    for s in q.iter(world) {
        // Fallback helper used by the placeholder renderer path: assign
        // material index 0 for all instances.
        out.push(s.to_instance_with_material(0));
    }
    out
}

// `collect_vertices` was removed; the renderer uses indexed meshes via
// `collect_indexed_vertices` above.

fn collect_indexed_vertices(_world: &mut World) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u32>) {
    // Return an indexed unit-sphere mesh with normals.
    Sphere::unit_sphere_indexed(16, 16)
}

// Simple fixed-step simulation function. Advance the ECS world by `dt`.
// Currently this is a placeholder — it can be extended to run physics,
// animate entities, or mutate components. `dt` is provided as a Duration
// to match our fixed-step scheduling.
#[allow(dead_code)]
fn simulate(_world: &mut World, dt: std::time::Duration) {
    // Apply controller inputs to any PlayerController components.
    let seconds = dt.as_secs_f32();
    let mut q = <(
        &mut engine_core::controller::PlayerController,
        &mut engine_core::controller::ControllerInput,
    )>::query();
    use std::sync::atomic::{AtomicU64, Ordering};
    static SIM_COUNT: AtomicU64 = AtomicU64::new(0);
    for (pc, ci) in q.iter_mut(_world) {
        let n = SIM_COUNT.fetch_add(1, Ordering::Relaxed);
        if n.is_multiple_of(60) {
            log::debug!(
                "simulate[#{}]: before pc={:?} ci={:?} dt={}",
                n,
                *pc,
                *ci,
                seconds
            );
        }
        pc.apply_input(ci, seconds);
        if n.is_multiple_of(60) {
            log::debug!("simulate[#{}]: after  pc={:?} ci={:?}", n, *pc, *ci);
        }
        // yaw/pitch deltas are one-shot; clear after applying so they act per-frame
        ci.yaw_delta = 0.0;
        ci.pitch_delta = 0.0;
    }
}
