use super::{MenuItem, Screen, ScreenSpec, UiComponent};

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

    fn render(&mut self, _ui: &mut egui::Ui) -> Vec<MenuItem> {
        Vec::new()
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
