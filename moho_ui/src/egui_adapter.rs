//! Simplified egui adapter for moho UI
//! Focused on getting the start menu working reliably

use engine_renderer::FrameCallback;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use winit::window::Window;

/// Simple UI events
#[derive(Debug, Clone)]
pub enum UiEvent {
    LoadScene(PathBuf),
    NewWorld,
    ShowMenu(String),
    Exit,
    OverlayToggled(bool),
}

pub type UiReceiver = crossbeam_channel::Receiver<UiEvent>;

/// Global flag for UI overlay visibility (for compatibility)
pub static UI_OVERLAY_VISIBLE: AtomicBool = AtomicBool::new(true);

/// Menu actions
#[derive(Debug, Clone)]
enum MenuAction {
    Continue,
    NewWorld,
    Settings,
    Exit,
}

/// Render start menu and return clicked action (if any)
fn render_start_menu_ui(ui: &mut egui::Ui) -> Option<MenuAction> {
    let mut action = None;

    ui.vertical_centered(|ui| {
        ui.add_space(40.0);
        ui.heading("Moho");
        ui.add_space(20.0);

        // Check if save exists
        let save_exists = std::path::Path::new("saves/scene.bin").exists();

        // Continue button
        if ui
            .add_enabled(
                save_exists,
                egui::Button::new("Continue").min_size(egui::vec2(160.0, 32.0)),
            )
            .clicked()
        {
            action = Some(MenuAction::Continue);
        }
        ui.add_space(8.0);

        // New World button
        if ui
            .add(egui::Button::new("New World").min_size(egui::vec2(160.0, 32.0)))
            .clicked()
        {
            action = Some(MenuAction::NewWorld);
        }
        ui.add_space(8.0);

        // Settings button (placeholder)
        if ui
            .add(egui::Button::new("Settings").min_size(egui::vec2(160.0, 32.0)))
            .clicked()
        {
            action = Some(MenuAction::Settings);
        }
        ui.add_space(8.0);

        // Exit button
        if ui
            .add(egui::Button::new("Exit").min_size(egui::vec2(160.0, 32.0)))
            .clicked()
        {
            action = Some(MenuAction::Exit);
        }
    });

    action
}

/// Simplified egui UI adapter - aliased as EguiUi for compatibility
pub struct EguiUi {
    pub ui_visible: bool,
    egui_ctx: egui::Context,
    egui_winit: Option<egui_winit::State>,
    window: Option<Arc<Window>>,
    sender: crossbeam_channel::Sender<UiEvent>,

    // egui renderer
    egui_renderer: Option<egui_wgpu::Renderer>,
    staging_belt: Option<wgpu::util::StagingBelt>,
    surface_format: Option<engine_renderer::TextureFormatRepr>,
}

impl EguiUi {
    pub fn new(window: Option<Arc<Window>>) -> (Self, UiReceiver) {
        let (sender, receiver) = crossbeam_channel::unbounded();

        let egui_ctx = egui::Context::default();

        // Create egui_winit state if window is available
        let egui_winit = window.as_ref().map(|win| egui_winit::State::new(
                egui_ctx.clone(),
                egui::ViewportId::ROOT,
                win.as_ref(),
                None,
                None,
                Some(1024), // max texture side
            ));

        let ui = Self {
            ui_visible: true, // Start with menu visible
            egui_ctx,
            egui_winit,
            window,
            sender: sender.clone(),
            egui_renderer: None,
            staging_belt: None,
            surface_format: None,
        };

        // Set the atomic flag and send initial overlay event since menu starts visible
        UI_OVERLAY_VISIBLE.store(true, std::sync::atomic::Ordering::SeqCst);
        let _ = sender.send(UiEvent::OverlayToggled(true));

        (ui, receiver)
    }

    /// Handle winit events - returns true if egui consumed the event
    pub fn handle_event(&mut self, event: &winit::event::WindowEvent) -> bool {
        if let Some(state) = &mut self.egui_winit {
            let response = state.on_window_event(self.window.as_ref().unwrap(), event);

            // Note: Escape key handling removed to prevent interference with menu

            response.consumed
        } else {
            false
        }
    }

    /// Toggle UI visibility (e.g., on Escape key)
    pub fn toggle_ui(&mut self) {
        self.ui_visible = !self.ui_visible;
        log::info!("UI visibility toggled: {}", self.ui_visible);
        UI_OVERLAY_VISIBLE.store(self.ui_visible, Ordering::SeqCst);
        let _ = self.sender.send(UiEvent::OverlayToggled(self.ui_visible));
    }

    /// Set surface format for renderer initialization
    pub fn set_surface_format(&mut self, format: engine_renderer::TextureFormatRepr) {
        self.surface_format = Some(format);
    }

    /// Handle winit event (for compatibility - same as handle_event)
    pub fn handle_winit_event(&mut self, event: &winit::event::WindowEvent) -> bool {
        self.handle_event(event)
    }

    /// Check if UI is visible
    pub fn is_visible(&self) -> bool {
        self.ui_visible
    }

    /// Recall staging belt memory after queue.submit()
    pub fn recall_staging_belt(&mut self) {
        if let Some(belt) = &mut self.staging_belt {
            belt.recall();
        }
    }
}

impl FrameCallback for EguiUi {
    fn call(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        surface_width: u32,
        surface_height: u32,
    ) {
        if !self.ui_visible {
            return;
        }

        log::debug!("EguiUi::call - rendering frame");

        // Initialize renderer if needed
        if self.egui_renderer.is_none() {
            if let Some(format) = self.surface_format {
                log::info!("Initializing egui_wgpu::Renderer with format {:?}", format);
                self.egui_renderer = Some(egui_wgpu::Renderer::new(
                    device,
                    format,
                    egui_wgpu::RendererOptions::default(),
                ));
                self.staging_belt = Some(wgpu::util::StagingBelt::new(1024));
            } else {
                log::warn!("No surface format set, cannot initialize egui renderer");
                return;
            }
        }

        // Get input for egui
        let input = if let (Some(state), Some(window)) = (&mut self.egui_winit, &self.window) {
            state.take_egui_input(window)
        } else {
            egui::RawInput::default()
        };

        // Check for button clicks in a separate scope to avoid borrow issues
        let mut button_clicked = None;

        // Run egui with our menu
        let full_output = self.egui_ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                button_clicked = render_start_menu_ui(ui);
            });
        });

        // Handle button clicks outside the egui closure
        if let Some(action) = button_clicked {
            match action {
                MenuAction::Continue => {
                    log::info!("Continue clicked - button interaction successful!");
                    let _ = self
                        .sender
                        .send(UiEvent::LoadScene("saves/scene.bin".into()));
                    // Keep menu visible for testing button interactions
                }
                MenuAction::NewWorld => {
                    log::info!("New World clicked - button interaction successful!");
                    let _ = self.sender.send(UiEvent::NewWorld);
                    // Keep menu visible for testing button interactions
                }
                MenuAction::Settings => {
                    log::info!("Settings clicked (placeholder) - button interaction successful!");
                }
                MenuAction::Exit => {
                    log::info!("Exit clicked - button interaction successful!");
                    let _ = self.sender.send(UiEvent::Exit);
                }
            }
        }

        // Handle platform output (cursor changes, etc.)
        if let (Some(state), Some(window)) = (&mut self.egui_winit, &self.window) {
            state.handle_platform_output(window, full_output.platform_output);
        }

        // Render egui graphics
        if let (Some(renderer), Some(belt)) = (&mut self.egui_renderer, &mut self.staging_belt) {
            let screen_descriptor = egui_wgpu::ScreenDescriptor {
                size_in_pixels: [surface_width, surface_height],
                pixels_per_point: 1.0,
            };

            // Update textures
            for (id, image_delta) in &full_output.textures_delta.set {
                renderer.update_texture(device, queue, *id, image_delta);
            }

            // Render egui - convert shapes to primitives first
            let primitives = self
                .egui_ctx
                .tessellate(full_output.shapes, full_output.pixels_per_point);

            // Upload vertex/index buffers using the staging belt
            if !primitives.is_empty() {
                renderer.update_buffers(device, queue, encoder, &primitives, &screen_descriptor);
            }

            // Begin render pass and use unsafe transmute to work around egui_wgpu lifetime requirements
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui_render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Don't clear, draw over existing content
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // egui-wgpu expects a 'static render pass reference. Use unsafe transmute
            // (the render pass remains valid until dropped below).
            let render_pass_static: &mut wgpu::RenderPass<'static> =
                unsafe { std::mem::transmute(&mut render_pass) };
            renderer.render(render_pass_static, &primitives, &screen_descriptor);
            drop(render_pass);

            // Free unused textures
            for id in &full_output.textures_delta.free {
                renderer.free_texture(id);
            }

            // Finish staging belt
            belt.finish();
        }
    }
}

/// Build adapter function for compatibility with existing main.rs
pub fn build_adapter(window: Option<Arc<Window>>) -> (EguiUi, UiReceiver) {
    EguiUi::new(window)
}
