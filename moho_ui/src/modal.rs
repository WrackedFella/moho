/// Result returned from modal rendering
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalResult {
    None,
    Confirm,
    Cancel,
}

/// Trait for modal dialogs
pub trait Modal {
    /// Returns the modal title
    fn title(&self) -> &str;

    /// Render the modal content and buttons. Returns the user's action.
    fn render(&mut self, ui: &mut egui::Ui) -> ModalResult;

    /// Called when the modal is confirmed (optional override)
    fn on_confirm(&mut self) {}

    /// Called when the modal is cancelled (optional override)
    fn on_cancel(&mut self) {}
}

/// Manages modal dialogs with backdrop and rendering
pub struct ModalManager {
    active_modal: Option<Box<dyn Modal>>,
    backdrop_color: egui::Color32,
}

impl Default for ModalManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ModalManager {
    pub fn new() -> Self {
        Self {
            active_modal: None,
            backdrop_color: egui::Color32::from_rgba_premultiplied(0, 0, 0, 180),
        }
    }

    /// Show a modal dialog
    pub fn show(&mut self, modal: Box<dyn Modal>) {
        self.active_modal = Some(modal);
    }

    /// Check if a modal is currently active
    pub fn is_active(&self) -> bool {
        self.active_modal.is_some()
    }

    /// Close the active modal
    pub fn close(&mut self) {
        self.active_modal = None;
    }

    /// Render the active modal (call this in your UI code)
    pub fn render(&mut self, ctx: &egui::Context) -> ModalResult {
        if let Some(ref mut modal) = self.active_modal {
            // Draw backdrop
            egui::Area::new(egui::Id::new("modal_backdrop"))
                .fixed_pos(egui::pos2(0.0, 0.0))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    let screen_rect = ctx.screen_rect();
                    ui.painter().rect_filled(
                        screen_rect,
                        0.0,
                        self.backdrop_color,
                    );
                });

            // Draw modal window
            let mut result = ModalResult::None;
            egui::Window::new(modal.title())
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    result = modal.render(ui);
                });

            // Handle result
            match result {
                ModalResult::Confirm => {
                    modal.on_confirm();
                    self.active_modal = None;
                }
                ModalResult::Cancel => {
                    modal.on_cancel();
                    self.active_modal = None;
                }
                ModalResult::None => {}
            }

            result
        } else {
            ModalResult::None
        }
    }
}
