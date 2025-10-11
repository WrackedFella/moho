use crate::menus::menu::{Menu, MenuAction, MenuSpec};
use std::path::PathBuf;

pub struct StartMenu {
    spec: MenuSpec,
}

impl StartMenu {
    pub fn new() -> Self {
        Self {
            spec: MenuSpec::default(),
        }
    }
}

impl Default for StartMenu {
    fn default() -> Self {
        Self::new()
    }
}

impl Menu for StartMenu {
    fn name(&self) -> &str {
        "start"
    }
    fn spec(&self) -> &MenuSpec {
        &self.spec
    }
    fn ui(&mut self, ctx: &egui::Context) -> (MenuAction, Option<(egui::Rect, egui::Rect)>) {
        let mut load_rect: Option<egui::Rect> = None;
        let mut exit_rect: Option<egui::Rect> = None;
        let mut action = MenuAction::None;
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(16.0);
                ui.heading("Moho");
                ui.add_space(8.0);
                let load_resp = ui.add(egui::Button::new("Start"));
                if load_resp.clicked() {
                    action = MenuAction::LoadScene(PathBuf::from("saves/scene.bin"));
                }
                ui.add_space(4.0);
                let exit_resp = ui.add(egui::Button::new("Exit"));
                if exit_resp.clicked() {
                    action = MenuAction::Exit;
                }
                load_rect = Some(load_resp.rect);
                exit_rect = Some(exit_resp.rect);
            });
        });
        (action, load_rect.and_then(|l| exit_rect.map(|e| (l, e))))
    }
}
