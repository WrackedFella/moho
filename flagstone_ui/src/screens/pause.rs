use super::{FormControls, MenuAction, MenuItem, Screen, ScreenSpec, UiComponent};

pub struct PauseMenu {
    spec: ScreenSpec,
}

impl PauseMenu {
    pub fn new() -> Self {
        Self {
            spec: ScreenSpec::default(),
        }
    }
}

impl Default for PauseMenu {
    fn default() -> Self {
        Self::new()
    }
}

impl UiComponent for PauseMenu {
    fn name(&self) -> &'static str {
        "pause"
    }

    /// Draws a centred window with no panel behind it, so the frozen world stays visible.
    fn render(&mut self, ui: &mut egui::Ui) -> Vec<MenuItem> {
        let mut items = Vec::new();
        let entries = [
            ("Continue", MenuAction::Resume),
            ("Settings", MenuAction::ShowMenu("settings".to_string())),
            ("Exit to main menu", MenuAction::ExitToMainMenu),
        ];

        egui::Window::new("Paused")
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ui.ctx(), |ui| {
                ui.vertical_centered(|ui| {
                    ui.heading("Paused");
                    ui.add_space(12.0);
                    for (label, action) in entries {
                        let button = FormControls::menu_button(ui, label, true);
                        items.push(MenuItem {
                            action,
                            rect: Some(button.rect),
                            enabled: true,
                            clicked: button.clicked(),
                            hovered: button.hovered(),
                        });
                        ui.add_space(6.0);
                    }
                });
            });

        items
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Screen for PauseMenu {
    fn spec(&self) -> &ScreenSpec {
        &self.spec
    }
}
