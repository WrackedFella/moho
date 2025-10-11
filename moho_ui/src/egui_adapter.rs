//! Minimal egui UI adapter for moho.
//!
//! This module is feature-gated behind `ui-egui`. It provides a very small
//! adapter that implements `engine_renderer::FrameCallback` and exposes
//! a channel of `UiEvent` messages the application can poll. This mirrors
//! the old iced-based API so the rest of the engine does not need to
//! change while the UI backend is swapped.

#[cfg(feature = "ui-egui")]
pub mod egui_adapter {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Instant;

    use engine_renderer::FrameCallback;
    use once_cell::sync::OnceCell;
    use winit::raw_window_handle::HasDisplayHandle;
    use winit::window::Window;

    /// Events sent from the UI to the application.
    #[derive(Debug, Clone)]
    pub enum UiEvent {
        LoadScene(PathBuf),
        Exit,
        /// Overlay visibility changed: true = shown, false = hidden
        OverlayToggled(bool),
    }

    static UI_SENDER: OnceCell<crossbeam_channel::Sender<UiEvent>> = OnceCell::new();

    /// Receiver type for UI events.
    pub type UiReceiver = crossbeam_channel::Receiver<UiEvent>;

    /// A minimal egui-backed UI adapter. For now it does not render
    /// any egui content but it provides a channel the app can poll to
    /// receive UI intent.
    pub struct EguiUi {
        sender: crossbeam_channel::Sender<UiEvent>,
        renderer_initialized: bool,
        window: Option<Arc<Window>>,
        // Simple input state for embedded UI (used until egui_winit is wired)
        last_cursor: Option<(f32, f32)>,
        mouse_pressed: bool,
        mouse_was_pressed: bool,
    // Whether the UI overlay is currently visible. Hidden by default.
    ui_visible: bool,
        // egui runtime fields (lazy initialized)
        egui_ctx: Option<egui::Context>,
        egui_winit: Option<egui_winit::State>,
        egui_renderer: Option<egui_wgpu::Renderer>,
        staging_belt: Option<wgpu::util::StagingBelt>,
        surface_format: Option<engine_renderer::TextureFormatRepr>,
    // If true, prefer consuming raw_input on the next FrameCallback even
    // if egui_winit is available. This is set when the overlay is shown
    // so synthetic PointerMoved events are honoured immediately.
    force_raw_input_next_frame: bool,
        // RawInput accumulator for egui (used until egui_winit is wired)
        raw_input: egui::RawInput,
        // reduce logging spam for cursor move forwarding
        last_cursor_log: Option<Instant>,
    }

    impl EguiUi {
        /// Provide the surface format used by the swapchain so we can
        /// initialize the egui GPU renderer with the correct format.
        pub fn set_surface_format(&mut self, fmt: engine_renderer::TextureFormatRepr) {
            self.surface_format = Some(fmt);
        }
        pub fn new(window: Option<Arc<Window>>) -> (Self, UiReceiver) {
            let (s, r) = crossbeam_channel::unbounded();
            let _ = UI_SENDER.set(s.clone());
            if cfg!(feature = "ui-egui-debug") {
                log::info!("Initializing embedded egui UI state");
            }
            // Eagerly create an egui Context so the UI can be shown
            // and receive input immediately after construction.
            let egui_ctx = Some(egui::Context::default());

            // If a window is available, also create egui_winit::State
            // now so input translation is ready on first use.
            let mut egui_winit_state = None;
            if let Some(win) = window.as_ref() {
                let display_target: &dyn HasDisplayHandle = &**win;
                let state = egui_winit::State::new(
                    egui_ctx.clone().unwrap(),
                    egui::ViewportId::ROOT,
                    display_target,
                    Some(win.scale_factor() as f32),
                    None,
                    None,
                );
                egui_winit_state = Some(state);
            }

            let ui = EguiUi {
                sender: s,
                renderer_initialized: false,
                window,
                // input state defaults
                last_cursor: None,
                mouse_pressed: false,
                mouse_was_pressed: false,
                ui_visible: false,
                egui_ctx: egui_ctx,
                egui_winit: egui_winit_state,
                egui_renderer: None,
                staging_belt: None,
                surface_format: None,
                raw_input: egui::RawInput::default(),
                last_cursor_log: None,
                force_raw_input_next_frame: false,
            };

            (ui, r)
        }

        pub fn send_load_scene<P: Into<PathBuf>>(&self, p: P) {
            let _ = self.sender.send(UiEvent::LoadScene(p.into()));
        }

        pub fn send_exit(&self) {
            let _ = self.sender.send(UiEvent::Exit);
        }

        /// Ask the staging belt to recall freed memory. Call this after the
        /// application submits the recorded command buffer(s) to the GPU queue.
        ///
        /// Important: The engine must call this immediately after calling
        /// `queue.submit(...)` so the staging belt can reclaim its memory safely.
        pub fn recall_staging_belt(&mut self) {
            if let Some(belt) = &mut self.staging_belt {
                // `recall` is safe to call multiple times; if the belt is empty
                // this is a no-op. We don't have access to the queue here; the
                // staging belt implementation used accepts no args for recall.
                belt.recall();
            }
        }

        pub fn handle_winit_event(&mut self, _event: &winit::event::WindowEvent) {
            use winit::event::{WindowEvent, ElementState, MouseButton};

            // No pre-processing here; events will be forwarded to egui_winit
            // when available. Escape detection and overlay toggling are
            // handled after forwarding so egui_winit can see the same event.

            // Lazy-init egui_winit::State when we have both egui context and a window.
            if self.egui_winit.is_none() {
                if let (Some(ctx), Some(win)) = (self.egui_ctx.clone(), self.window.as_ref()) {
                    // build a display target from the winit Window
                    let display_target: &dyn HasDisplayHandle = &**win;
                    let state = egui_winit::State::new(
                        ctx,
                        egui::ViewportId::ROOT,
                        display_target,
                        Some(win.scale_factor() as f32),
                        None,
                        None,
                    );
                    self.egui_winit = Some(state);
                }
            }

            // If egui_winit is available, forward the event to it. It will
            // translate and consume events appropriately. After forwarding we
            // inspect keyboard events for Escape so we can toggle overlay in a
            // consistent way. Otherwise fall back to our simple pointer
            // handling (only while UI visible).
            if let (Some(state), Some(win)) = (self.egui_winit.as_mut(), self.window.as_ref()) {
                // Capture cursor position even when forwarding to egui_winit so we
                // can bootstrap raw_input when showing the overlay.
                if let winit::event::WindowEvent::CursorMoved { position, .. } = _event {
                    let px = position.x as f32;
                    let py = position.y as f32;
                    // Convert to logical points using the window's scale factor
                    if let Some(win) = &self.window {
                        let ppp = win.scale_factor() as f32;
                        self.last_cursor = Some((px / ppp, py / ppp));
                    } else {
                        self.last_cursor = Some((px, py));
                    }
                    if cfg!(feature = "ui-egui-debug") {
                        log::debug!("EguiUi: recorded cursor while forwarding to egui_winit to=({}, {})", px, py);
                    }
                }
                // Forward general events to egui_winit. CursorMoved can be
                // very noisy (lots of events per second) so keep the
                // forwarding log at debug level to avoid flooding INFO logs.
                log::debug!("EguiUi: forwarding event to egui_winit: {:?}", _event);
                let _ = state.on_window_event(win, _event);

                // If this was a KeyboardInput for Escape, toggle overlay.
                if let WindowEvent::KeyboardInput { event, .. } = _event {
                    if event.state == ElementState::Pressed {
                        let is_escape = event
                            .text
                            .as_deref()
                            .map(|s| s == "\u{1b}")
                            .unwrap_or(false)
                            || format!("{:?}", event.logical_key).contains("Escape");

                        if is_escape {
                            self.ui_visible = !self.ui_visible;
                            if cfg!(feature = "ui-egui-debug") {
                                log::info!("EguiUi: Escape pressed -> ui_visible={}", self.ui_visible);
                            }
                            if let Some(win) = &self.window {
                                let _ = win.set_cursor_visible(self.ui_visible);
                            }
                            // Notify the app about the overlay visibility change so
                            // it can reconcile cursor-grab state if needed.
                            let _ = self.sender.send(UiEvent::OverlayToggled(self.ui_visible));
                                if self.ui_visible {
                                    // Clear any accumulated input when showing UI.
                                    self.raw_input = egui::RawInput::default();
                                    // If we know the last cursor position, push a synthetic
                                    // pointer move so egui has an initial pointer position
                                    // for hover and click detection on first show. `last_cursor`
                                    // is stored in logical points already.
                                    if let Some((x, y)) = self.last_cursor {
                                        self.raw_input.events.push(egui::Event::PointerMoved(egui::pos2(x, y)));
                                    } else if let Some(win) = &self.window {
                                        // No known cursor position; inject center-of-window
                                        // in logical points so egui has a reasonable initial pointer location.
                                        let size = win.inner_size();
                                        let ppp = win.scale_factor() as f32;
                                        let cx = (size.width as f32) / 2.0 / ppp;
                                        let cy = (size.height as f32) / 2.0 / ppp;
                                        self.last_cursor = Some((cx, cy));
                                        self.raw_input.events.push(egui::Event::PointerMoved(egui::pos2(cx, cy)));
                                    }
                                // Force raw_input to be preferred on the next frame so
                                // the synthetic event is consumed even when egui_winit
                                // is present.
                                self.force_raw_input_next_frame = true;
                                // Request an immediate repaint from egui so the UI
                                // layout and paint jobs are generated on the next
                                // FrameCallback::call invocation.
                                if let Some(ctx) = &self.egui_ctx {
                                    ctx.request_repaint();
                                }
                            }
                        }
                    }
                }
            } else {
                // Only forward pointer events while the UI is visible.
                match _event {
                    // If we don't have egui_winit yet, still allow Escape to
                    // toggle the overlay so the user can bring up the UI early.
                    WindowEvent::KeyboardInput { event, .. } => {
                        if event.state == ElementState::Pressed {
                            let is_escape = event
                                .text
                                .as_deref()
                                .map(|s| s == "\u{1b}")
                                .unwrap_or(false)
                                || format!("{:?}", event.logical_key).contains("Escape");

                            if is_escape {
                                self.ui_visible = !self.ui_visible;
                                if cfg!(feature = "ui-egui-debug") {
                                    log::info!("EguiUi: Escape (fallback) -> ui_visible={}", self.ui_visible);
                                }
                                if let Some(win) = &self.window {
                                    let _ = win.set_cursor_visible(self.ui_visible);
                                }
                                let _ = self.sender.send(UiEvent::OverlayToggled(self.ui_visible));
                                if self.ui_visible {
                                    self.raw_input = egui::RawInput::default();
                                    if let Some((x, y)) = self.last_cursor {
                                        self.raw_input.events.push(egui::Event::PointerMoved(egui::pos2(x, y)));
                                    }
                                    if let Some(ctx) = &self.egui_ctx {
                                        ctx.request_repaint();
                                    }
                                }
                            }
                        }
                    }
                    WindowEvent::CursorMoved { position, .. } if self.ui_visible => {
                        // Avoid logging each cursor movement; only occasionally
                        // update an internal timestamp for debug logs. We still
                        // forward the pointer to the raw_input accumulator.
                        let px = position.x as f32;
                        let py = position.y as f32;
                        if let Some(win) = &self.window {
                            let ppp = win.scale_factor() as f32;
                            self.last_cursor = Some((px / ppp, py / ppp));
                            self.raw_input.events.push(egui::Event::PointerMoved(egui::pos2(px / ppp, py / ppp)));
                        } else {
                            self.last_cursor = Some((px, py));
                            self.raw_input.events.push(egui::Event::PointerMoved(egui::pos2(px, py)));
                        }
                        // Throttle any debug logging to ~100ms.
                        if self.last_cursor_log.map_or(true, |t| t.elapsed().as_millis() >= 100) {
                            self.last_cursor_log = Some(Instant::now());
                            log::debug!("EguiUi: CursorMoved while ui_visible to=({}, {})", px, py);
                        }
                    }
                    WindowEvent::MouseInput { state, button, .. } if self.ui_visible => {
                        if *button == MouseButton::Left {
                            let pressed = *state == ElementState::Pressed;
                            self.mouse_pressed = pressed;
                            let pos = self.last_cursor.map(|(x, y)| egui::pos2(x, y)).unwrap_or(egui::pos2(0.0, 0.0));
                            self.raw_input.events.push(egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: egui::Modifiers::default(),
                            });
                        }
                    }
                    WindowEvent::ModifiersChanged(_mods) if self.ui_visible => {
                        self.raw_input.modifiers = egui::Modifiers::default();
                    }
                    _ => {}
                }
            }
        }
    }

    impl Default for EguiUi {
        fn default() -> Self {
            let (s, _) = crossbeam_channel::unbounded();
            EguiUi {
                sender: s,
                renderer_initialized: false,
                window: None,
                last_cursor: None,
                mouse_pressed: false,
                mouse_was_pressed: false,
                egui_ctx: None,
                egui_winit: None,
                egui_renderer: None,
                staging_belt: None,
                surface_format: None,
                raw_input: egui::RawInput::default(),
                last_cursor_log: None,
                ui_visible: false,
                force_raw_input_next_frame: false,
            }
        }
    }

    impl FrameCallback for EguiUi {
        fn call(
            &mut self,
            _device: &wgpu::Device,
            _queue: &wgpu::Queue,
            _view: &wgpu::TextureView,
            _encoder: &mut wgpu::CommandEncoder,
        ) {
            if cfg!(feature = "ui-egui-debug") {
                log::info!("EguiUi::call invoked; ui_visible={}", self.ui_visible);
            }

            // Lazy init placeholder
            if !self.renderer_initialized {
                // Initialize basic egui runtime pieces lazily so we have
                // access to the wgpu device/queue. Full renderer creation
                // will require the surface format; attempt to query it from
                // the engine renderer via the global `create_renderer` path
                // or the stored window. For now leave it as None and mark
                // initialized so subsequent passes can populate renderer.
                self.egui_ctx = Some(egui::Context::default());
                if cfg!(feature = "ui-egui-debug") {
                    log::info!("EguiUi: created egui::Context");
                }
                // Defer creating `egui_winit::State` here because the
                // constructor changed in egui-winit v0.33. Initialize it
                // later using the examples in `/examples` which show the
                // correct signature and required parameters.
                // if let Some(win) = &self.window {
                //     self.egui_winit = Some(egui_winit::State::new(/* ... */));
                // }
                // staging belt and egui_renderer will be created in step C
                self.renderer_initialized = true;
            }

            // Only drive egui when the UI overlay is visible. When hidden we
            // skip rendering and hit-testing so the 3D scene receives input
            // as normal.
            if !self.ui_visible {
                log::debug!("EguiUi::call skipping because ui_visible=false");
            }

            if self.ui_visible {
                log::info!("EguiUi::call driving egui frame");
                // Drive egui: begin a frame with accumulated raw_input so egui
                // consumes forwarded events. We build a minimal menu with two
                // buttons (Load Scene, Exit) that send UiEvent messages.
                if let Some(ctx) = &self.egui_ctx {
                    // Determine input via egui_winit when available (recommended),
                    // otherwise fall back to the manually-accumulated RawInput.
                    // If we have any manually-injected RawInput events (for
                    // example a synthetic PointerMoved pushed when the overlay
                    // was toggled), prefer consuming them this frame so egui
                    // receives the initial pointer position immediately. This
                    // makes the UI interactive on the first show instead of
                    // waiting a frame.
                    if cfg!(feature = "ui-egui-debug") {
                        log::info!("EguiUi: raw_input.events.len() = {}", self.raw_input.events.len());
                    }

                    let input = if self.force_raw_input_next_frame {
                        // Prefer to *merge* our synthetic raw_input events into
                        // egui_winit's input when possible so we retain the
                        // correct pixels_per_point and screen size information.
                        if cfg!(feature = "ui-egui-debug") {
                            log::info!("EguiUi: forcing raw_input consumption this frame");
                        }
                        self.force_raw_input_next_frame = false;
                        if let (Some(state), Some(win)) = (self.egui_winit.as_mut(), self.window.as_ref()) {
                            // Take the normally-produced input and append our
                            // synthetic events so egui has both accurate
                            // DPI/screen info and the injected pointer.
                            let mut input = state.take_egui_input(win);
                            let mut manual = std::mem::take(&mut self.raw_input);
                            input.events.append(&mut manual.events);
                            input
                        } else {
                            // No egui_winit available, fall back to raw_input.
                            std::mem::take(&mut self.raw_input)
                        }
                    } else if !self.raw_input.events.is_empty() {
                        if cfg!(feature = "ui-egui-debug") {
                            log::info!("EguiUi: taking input via raw_input (direct)");
                        }
                        std::mem::take(&mut self.raw_input)
                    } else if let (Some(state), Some(win)) = (self.egui_winit.as_mut(), self.window.as_ref()) {
                        if cfg!(feature = "ui-egui-debug") {
                            log::info!("EguiUi: taking input via egui_winit::State");
                        }
                        state.take_egui_input(win)
                    } else {
                        if cfg!(feature = "ui-egui-debug") {
                            log::info!("EguiUi: taking input via raw_input accumulator");
                        }
                        std::mem::take(&mut self.raw_input)
                    };

                    // Run egui using the collected input. `ctx.run` returns a `FullOutput`
                    // containing shapes, textures_delta and other platform output.
                    if cfg!(feature = "ui-egui-debug") {
                        log::info!(
                            "EguiUi: about to call ctx.run; input.events.len()={}, screen_rect={:?}",
                            input.events.len(),
                            input.screen_rect
                        );
                        log::info!("EguiUi: input.events = {:?}", input.events);
                    }

                    let full_output = ctx.run(input, |ctx| {
                        if cfg!(feature = "ui-egui-debug") {
                            log::info!("EguiUi: running UI closure (building widgets)");
                        }
                        // Build a minimal UI using a Top panel so we get a stable
                        // visible area on the first frame. This is a debugging
                        // change to ensure egui actually produces shapes.
                        egui::TopBottomPanel::top("moho_top_panel").show(ctx, |ui| {
                            ui.horizontal(|ui| {
                                if ui.button("Load Scene").clicked() {
                                    if cfg!(feature = "ui-egui-debug") {
                                        log::info!("EguiUi: Load Scene button clicked");
                                    }
                                    let _ = self.sender.send(UiEvent::LoadScene(PathBuf::from("saves/scene.bin")));
                                }
                                if ui.button("Exit").clicked() {
                                    if cfg!(feature = "ui-egui-debug") {
                                        log::info!("EguiUi: Exit button clicked");
                                    }
                                    let _ = self.sender.send(UiEvent::Exit);
                                }
                            });
                        });
                    });

                // Break out the FullOutput into parts we need so we can inspect
                // shapes without moving them prematurely.
                let pixels_per_point = full_output.pixels_per_point;
                let textures_delta = full_output.textures_delta;
                let shapes = full_output.shapes;

                if cfg!(feature = "ui-egui-debug") {
                    log::info!(
                        "EguiUi: full_output shapes={}, textures_set={}, textures_free={}, ppp={}",
                        shapes.len(),
                        textures_delta.set.len(),
                        textures_delta.free.len(),
                        pixels_per_point
                    );
                }

                // Ensure egui_wgpu renderer is created when we know the surface format
                    if self.egui_renderer.is_none() {
                        if let Some(fmt) = self.surface_format {
                            if cfg!(feature = "ui-egui-debug") {
                                log::info!("Initializing egui_wgpu::Renderer with surface format {:?}", fmt);
                            }
                            // Use default options; egui-wgpu exposes a Renderer::new that accepts defaults
                            self.egui_renderer = Some(egui_wgpu::Renderer::new(_device, fmt, Default::default()));
                        } else {
                            if cfg!(feature = "ui-egui-debug") {
                                log::info!("EguiUi: egui_renderer not created yet; surface_format missing");
                            }
                        }
                    }

                // Handle textures delta (upload / free)
                if let Some(renderer) = &mut self.egui_renderer {
                    for (id, image_delta) in textures_delta.set {
                        renderer.update_texture(_device, _queue, id, &image_delta);
                    }
                    for id in textures_delta.free {
                        renderer.free_texture(&id);
                    }

                    // Tessellate shapes into paint jobs. Check whether shapes
                    // are empty before consuming them so we can trigger the
                    // auto-debug clear when needed.
                    let auto_debug = self.ui_visible && shapes.is_empty();
                    let clipped_primitives = ctx.tessellate(shapes, pixels_per_point);

                        // Prepare screen descriptor from the window if available
                    let screen_desc = if let Some(win) = &self.window {
                        let size = win.inner_size();
                        egui_wgpu::ScreenDescriptor {
                            size_in_pixels: [size.width as u32, size.height as u32],
                            pixels_per_point: pixels_per_point,
                        }
                    } else {
                        egui_wgpu::ScreenDescriptor {
                            size_in_pixels: [800, 600],
                            pixels_per_point: pixels_per_point,
                        }
                    };

                    // Upload buffers (writes to `encoder`). It may return user command buffers which we ignore here.
                    // Ensure a staging belt exists for uploads and buffer staging.
                        if self.staging_belt.is_none() {
                            if cfg!(feature = "ui-egui-debug") {
                                log::info!("EguiUi: creating StagingBelt");
                            }
                            self.staging_belt = Some(wgpu::util::StagingBelt::new(1024));
                        }

                    let _user_cmds = renderer.update_buffers(_device, _queue, _encoder, &clipped_primitives, &screen_desc);

                        // Finish the staging belt so its work is submitted with the
                        // command encoder. The application must call
                        // `recall_staging_belt()` on this adapter immediately after
                        // it calls `queue.submit(...)` so the belt can reclaim memory.
                        if let Some(belt) = &mut self.staging_belt {
                            if cfg!(feature = "ui-egui-debug") {
                                log::info!("EguiUi: finishing staging belt");
                            }
                            belt.finish();
                        }

                    // Optional debug: clear the target to a bright color so we can
                    // visually confirm whether the egui render pass or another pass
                    // is occluding the UI. Controlled by env var MOHO_EGUI_DEBUG_CLEAR=1
                    // OR automatically triggered when the overlay is visible but
                    // egui produced zero shapes (one-shot test to detect overwrites).
                    let env_debug = std::env::var("MOHO_EGUI_DEBUG_CLEAR").ok().map(|v| v == "1").unwrap_or(false);
                    let do_debug_clear = env_debug || auto_debug;
                    if do_debug_clear {
                        if auto_debug && cfg!(feature = "ui-egui-debug") {
                            log::info!("EguiUi: auto magenta clear triggered because overlay visible but no shapes were produced");
                        }
                        if cfg!(feature = "ui-egui-debug") {
                            log::info!("EguiUi: issuing bright magenta clear");
                        }
                        let _debug_clear = _encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("egui_debug_clear_pass"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: _view,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color { r: 1.0, g: 0.0, b: 1.0, a: 1.0 }),
                                    store: wgpu::StoreOp::Store,
                                },
                                depth_slice: None,
                            })],
                            depth_stencil_attachment: None,
                            occlusion_query_set: None,
                            timestamp_writes: None,
                        });
                        // drop immediately so subsequent passes can overwrite
                        drop(_debug_clear);
                    }

                    // Create a render pass and hand it to egui renderer. We create the same render pass
                    // used earlier for the placeholder; to avoid duplicating clear, we create a fresh one
                    // here that assumes the view already has the scene drawn behind it.
                        let mut egui_rpass = _encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("egui_ui_pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: _view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            },
                            depth_slice: None,
                        })],
                        depth_stencil_attachment: None,
                        occlusion_query_set: None,
                        timestamp_writes: None,
                    });

                    // egui-wgpu expects a 'static render pass reference. Do the unsafe transmute trick
                    // (the render pass remains valid until dropped below).
                        let egui_rpass_static: &mut wgpu::RenderPass<'static> = unsafe { std::mem::transmute(&mut egui_rpass) };
                        if cfg!(feature = "ui-egui-debug") {
                            log::info!("EguiUi: calling egui_wgpu::Renderer::render");
                        }
                        renderer.render(egui_rpass_static, &clipped_primitives, &screen_desc);
                    // Drop the render pass (and transmute alias) by letting `egui_rpass` go out of scope.
                    drop(egui_rpass);
                    }
                }

                // If egui_wgpu is not initialized yet, don't clear the
                // scene — instead create a no-op load pass so the 3D
                // scene remains visible beneath the soon-to-be-painted UI.
                let rpass = _encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui_ui_placeholder_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: _view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                });
                drop(rpass);

                // Simple, non-egui UI hit-testing: two rectangular buttons in the
                // top-left corner. Coordinates are in logical pixels and assume the
                // window's origin at (0,0). This allows the engine to receive
                // actionable UI events before full egui rendering is wired.
                //
                // Button layout:
                //  - Load Scene: rect [8,8 .. 128,40]
                //  - Exit:       rect [8,48 .. 128,80]
                if let Some((x, y)) = self.last_cursor {
                    let in_load = x >= 8.0 && x <= 128.0 && y >= 8.0 && y <= 40.0;
                    let in_exit = x >= 8.0 && x <= 128.0 && y >= 48.0 && y <= 80.0;

                    // Detect click (pressed -> released) within the same frame window.
                    if self.mouse_was_pressed && !self.mouse_pressed {
                        if in_load {
                            // Send a LoadScene with a default path (can be changed later)
                            let _ = self.sender.send(UiEvent::LoadScene(PathBuf::from("saves/scene.bin")));
                        } else if in_exit {
                            let _ = self.sender.send(UiEvent::Exit);
                        }
                    }
                }
            }

            // Track previous press state for click detection on next frame.
            self.mouse_was_pressed = self.mouse_pressed;
        }
    }
}
