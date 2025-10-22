//! Modern, modular egui adapter for moho UI
//!
//! This adapter manages menus and UI state in a scalable way,
//! allowing easy addition of new menus and menu types.

use crate::menus::{Menu, MenuAction, SettingsMenu, StartMenu};
use crate::modal::ModalManager;
use crate::prefs::Prefs;
use engine_renderer::FrameCallback;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use winit::event::WindowEvent;
use winit::window::Window;

/// UI events sent to the application
#[derive(Debug, Clone)]
pub enum UiEvent {
    LoadScene(PathBuf),
    NewWorld,
    ShowMenu(String),
    Exit,
    OverlayToggled(bool),
    SettingsSaved(Prefs),
}

pub type UiReceiver = crossbeam_channel::Receiver<UiEvent>;

/// Global flag for UI overlay visibility
pub static UI_OVERLAY_VISIBLE: AtomicBool = AtomicBool::new(true);

/// Modern egui adapter that manages multiple menus
pub struct EguiAdapter {
    // egui integration
    context: egui::Context,
    winit_state: Option<egui_winit::State>,
    renderer: Option<egui_wgpu::Renderer>,

    // Menu management
    menus: HashMap<String, Box<dyn Menu>>,
    active_menu: Option<String>,

    // Modal management
    pub modal_manager: ModalManager,

    // Communication
    sender: crossbeam_channel::Sender<UiEvent>,

    // State
    pub ui_visible: bool,
    window: Option<Arc<Window>>,

    // Rendering
    surface_config: Option<wgpu::SurfaceConfiguration>,
}

impl EguiAdapter {
    /// Create a new adapter with a communication channel
    pub fn new(window: Option<Arc<Window>>) -> (Self, UiReceiver) {
        let (sender, receiver) = crossbeam_channel::unbounded();

        let context = egui::Context::default();
        context.set_visuals(egui::Visuals::dark());

        // Initialize winit state if we have a window
        let winit_state = window.as_ref().map(|w| {
            egui_winit::State::new(
                context.clone(),
                egui::ViewportId::ROOT,
                w.as_ref(),
                None,
                None,
                None,
            )
        });

        // Create initial menu set
        let mut menus: HashMap<String, Box<dyn Menu>> = HashMap::new();
        menus.insert("start".to_string(), Box::new(StartMenu::new()));
        menus.insert("settings".to_string(), Box::new(SettingsMenu::new()));

        let adapter = Self {
            context,
            winit_state,
            renderer: None,
            menus,
            active_menu: Some("start".to_string()),
            modal_manager: ModalManager::new(),
            sender,
            ui_visible: true,
            window,
            surface_config: None,
        };

        (adapter, receiver)
    }

    /// Add a new menu to the manager
    pub fn add_menu(&mut self, name: String, menu: Box<dyn Menu>) {
        self.menus.insert(name, menu);
    }

    /// Show a specific menu
    pub fn show_menu(&mut self, name: &str) {
        if self.menus.contains_key(name) {
            if let Some(current) = &self.active_menu
                && let Some(menu) = self.menus.get_mut(current)
            {
                menu.on_hide();
            }

            self.active_menu = Some(name.to_string());
            if let Some(menu) = self.menus.get_mut(name) {
                menu.on_show();
            }
            self.ui_visible = true;
        }
    }

    /// Hide all menus
    pub fn hide_menus(&mut self) {
        if let Some(current) = &self.active_menu
            && let Some(menu) = self.menus.get_mut(current)
        {
            menu.on_hide();
        }
        self.active_menu = None;
        self.ui_visible = false;
    }

    /// Handle window events (mouse, keyboard)
    pub fn handle_winit_event(&mut self, event: &WindowEvent) {
        if let Some(state) = &mut self.winit_state {
            let _ = state.on_window_event(self.window.as_ref().unwrap(), event);
        }
    }

    /// Set surface format for renderer initialization
    pub fn set_surface_format(&mut self, format: wgpu::TextureFormat) {
        self.surface_config = Some(wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: 800,
            height: 600,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        });
    }

    /// Initialize the wgpu renderer
    fn init_renderer(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) {
        if self.renderer.is_none() {
            let renderer =
                egui_wgpu::Renderer::new(device, format, egui_wgpu::RendererOptions::default());
            self.renderer = Some(renderer);
            log::info!("Initializing egui_wgpu::Renderer with format {:?}", format);
        }
    }

    /// Process menu actions and convert to UI events
    fn process_menu_action(&mut self, action: MenuAction) {
        match action {
            MenuAction::LoadScene(path) => {
                let _ = self.sender.send(UiEvent::LoadScene(path));
            }
            MenuAction::NewWorld => {
                let _ = self.sender.send(UiEvent::NewWorld);
            }
            MenuAction::Exit => {
                let _ = self.sender.send(UiEvent::Exit);
            }
            MenuAction::ShowMenu(name) => {
                let _ = self.sender.send(UiEvent::ShowMenu(name));
            }
            MenuAction::Close => {
                self.hide_menus();
            }
            MenuAction::SettingsSaved(prefs) => {
                let _ = self.sender.send(UiEvent::SettingsSaved(prefs));
            }
            MenuAction::None => {
                // No action
            }
        }
    }

    /// Get input from egui_winit
    fn take_egui_input(&mut self) -> egui::RawInput {
        if let (Some(state), Some(window)) = (&mut self.winit_state, &self.window) {
            state.take_egui_input(window.as_ref())
        } else {
            egui::RawInput::default()
        }
    }

    /// Handle platform output from egui
    fn handle_platform_output(&mut self, platform_output: egui::PlatformOutput) {
        if let (Some(state), Some(window)) = (&mut self.winit_state, &self.window) {
            state.handle_platform_output(window.as_ref(), platform_output);
        }
    }

    /// Recall staging belt for memory management
    pub fn recall_staging_belt(&mut self) {
        // This is called by the main application after rendering
        // No-op for now, but could be used for cleanup
    }
}

impl FrameCallback for EguiAdapter {
    fn call(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        surface_width: u32,
        surface_height: u32,
    ) {
        // Skip rendering if UI is not visible
        if !self.ui_visible {
            return;
        }

        // Update global visibility flag
        UI_OVERLAY_VISIBLE.store(self.ui_visible, Ordering::SeqCst);

        // Initialize renderer if needed
        let format = self
            .surface_config
            .as_ref()
            .map(|c| c.format)
            .unwrap_or(wgpu::TextureFormat::Bgra8UnormSrgb);
        self.init_renderer(device, format);

        // Take input from winit integration
        let raw_input = self.take_egui_input();

        // Run egui and collect menu actions
        let mut menu_actions = Vec::new();
        let mut modal_result = crate::modal::ModalResult::None;
        
        let full_output = self.context.run(raw_input, |ctx| {
            // Render active menu if any
            if let Some(menu_name) = &self.active_menu.clone()
                && let Some(menu) = self.menus.get_mut(menu_name)
            {
                let items = menu.ui(ctx);

                // Collect clicked actions for processing outside the closure
                for item in items {
                    if item.clicked && item.enabled {
                        menu_actions.push(item.action);
                    }
                }
            }

            // Check if settings menu wants to show conflict modal
            if let Some(menu) = self.menus.get_mut("settings") {
                if let Some(settings) = menu.as_any_mut().downcast_mut::<SettingsMenu>() {
                    if settings.show_conflict_modal {
                        settings.show_conflict_modal = false;
                        
                        use crate::modals::KeybindConflictModal;
                        let modal = KeybindConflictModal::new(
                            settings.conflict_key_name.clone(),
                            settings.conflict_binding_desc.clone(),
                        );
                        
                        self.modal_manager.show(Box::new(modal));
                    }
                }
            }

            // Render modal on top of menu (if active)
            modal_result = self.modal_manager.render(ctx);
        });

        // Handle modal result
        use crate::modal::ModalResult;
        match modal_result {
            ModalResult::Confirm => {
                if let Some(menu) = self.menus.get_mut("settings") {
                    if let Some(settings) = menu.as_any_mut().downcast_mut::<SettingsMenu>() {
                        settings.apply_pending_binding();
                    }
                }
            }
            ModalResult::Cancel => {
                if let Some(menu) = self.menus.get_mut("settings") {
                    if let Some(settings) = menu.as_any_mut().downcast_mut::<SettingsMenu>() {
                        settings.cancel_pending_binding();
                    }
                }
            }
            ModalResult::None => {}
        }

        // Process menu actions outside the egui context
        for action in menu_actions {
            self.process_menu_action(action);
        }

        // Handle platform output
        self.handle_platform_output(full_output.platform_output);

        // Render to screen
        if let Some(renderer) = &mut self.renderer {
            let screen_descriptor = egui_wgpu::ScreenDescriptor {
                size_in_pixels: [surface_width, surface_height],
                pixels_per_point: self.context.pixels_per_point(),
            };

            let clipped_primitives = self
                .context
                .tessellate(full_output.shapes, full_output.pixels_per_point);

            // Update textures
            for (id, image_delta) in &full_output.textures_delta.set {
                renderer.update_texture(device, queue, *id, image_delta);
            }

            // Update GPU buffers
            renderer.update_buffers(
                device,
                queue,
                encoder,
                &clipped_primitives,
                &screen_descriptor,
            );

            // Create render pass and render
            {
                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui_render_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });

                // Use unsafe transmute to satisfy egui_wgpu lifetime requirements
                let render_pass_static: &mut wgpu::RenderPass<'static> =
                    unsafe { std::mem::transmute(&mut render_pass) };
                renderer.render(render_pass_static, &clipped_primitives, &screen_descriptor);
            }

            // Free textures
            for id in &full_output.textures_delta.free {
                renderer.free_texture(id);
            }
        }
    }
}

/// Build adapter function for compatibility with existing main.rs
pub fn build_adapter(window: Option<Arc<Window>>) -> (EguiAdapter, UiReceiver) {
    EguiAdapter::new(window)
}
