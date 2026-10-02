use super::{FormControls, MenuAction, Screen, ScreenSpec, UiComponent};
use std::path::PathBuf;

pub struct StartMenu {
    spec: ScreenSpec,
}

impl StartMenu {
    pub fn new() -> Self {
        Self {
            spec: ScreenSpec::default(),
        }
    }
}

impl Default for StartMenu {
    fn default() -> Self {
        Self::new()
    }
}

// Implement UiComponent (base trait)
impl UiComponent for StartMenu {
    fn name(&self) -> &str {
        "start"
    }

    fn render(&mut self, ctx: &egui::Context) -> Vec<super::MenuItem> {
        let mut items: Vec<super::MenuItem> = Vec::new();

        // Determine whether a saved scene exists
        let save_path = PathBuf::from("saves/scene.bin");
        let save_exists = std::path::Path::new(&save_path).exists();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(16.0);
                ui.heading("Moho");
                ui.add_space(12.0);

                // Continue button (disabled when no save exists)
                let cont = FormControls::menu_button(ui, "Continue", save_exists);
                let cont_clicked = cont.clicked() && save_exists;
                items.push(super::MenuItem {
                    action: MenuAction::LoadScene(save_path.clone()),
                    rect: Some(cont.rect),
                    enabled: save_exists,
                    clicked: cont_clicked,
                    hovered: cont.hovered(),
                });
                ui.add_space(6.0);

                // New World button - always enabled
                let nw = FormControls::menu_button(ui, "New World", true);
                let nw_clicked = nw.clicked();
                items.push(super::MenuItem {
                    action: MenuAction::NewWorld,
                    rect: Some(nw.rect),
                    enabled: true,
                    clicked: nw_clicked,
                    hovered: nw.hovered(),
                });
                ui.add_space(6.0);

                // Settings button
                let st = FormControls::menu_button(ui, "Settings", true);
                let st_clicked = st.clicked();
                items.push(super::MenuItem {
                    action: MenuAction::ShowMenu("settings".to_string()),
                    rect: Some(st.rect),
                    enabled: true,
                    clicked: st_clicked,
                    hovered: st.hovered(),
                });
                ui.add_space(6.0);

                let ex = FormControls::menu_button(ui, "Exit", true);
                let ex_clicked = ex.clicked();
                items.push(super::MenuItem {
                    action: MenuAction::Exit,
                    rect: Some(ex.rect),
                    enabled: true,
                    clicked: ex_clicked,
                    hovered: ex.hovered(),
                });
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

// Implement Screen (specialized trait)
impl Screen for StartMenu {
    fn spec(&self) -> &ScreenSpec {
        &self.spec
    }
}
