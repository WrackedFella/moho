//! Drawing the console with egui.

use super::Console;

impl<G> Console<G> {
    /// Draws the console over the frame; draws nothing while it is closed. Enter submits the
    /// input line; Up and Down recall earlier lines.
    pub fn show(&mut self, _ctx: &egui::Context) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::tests::{Cmd, tgm};

    fn pass(ctx: &egui::Context, console: &mut Console<Cmd>, events: Vec<egui::Event>) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            events,
            ..egui::RawInput::default()
        };
        let mut output = ctx.run_ui(input, |ui| console.show(&ui.ctx().clone()));
        output.textures_delta.clear();
    }

    fn key(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn open_console() -> (egui::Context, Console<Cmd>) {
        let ctx = egui::Context::default();
        let (mut console, _) = Console::new(&std::collections::BTreeMap::new());
        console.register(tgm()).unwrap();
        console.open();
        pass(&ctx, &mut console, vec![]);
        (ctx, console)
    }

    #[test]
    fn typed_text_goes_to_the_input_line() {
        let (ctx, mut console) = open_console();

        pass(&ctx, &mut console, vec![egui::Event::Text("w".to_owned())]);

        assert_eq!(console.input(), "w");
    }

    #[test]
    fn enter_submits_the_input_line_and_clears_it() {
        let (ctx, mut console) = open_console();
        pass(
            &ctx,
            &mut console,
            vec![egui::Event::Text("tgm 1".to_owned())],
        );

        pass(&ctx, &mut console, vec![key(egui::Key::Enter)]);

        assert!(console.output().lines().iter().any(|l| l == "> tgm 1"));
        assert_eq!(console.take_commands(), vec![Cmd::God(true)]);
        assert_eq!(console.input(), "");
    }

    #[test]
    fn up_recalls_the_previous_line() {
        let (ctx, mut console) = open_console();
        pass(
            &ctx,
            &mut console,
            vec![egui::Event::Text("tgm 1".to_owned())],
        );
        pass(&ctx, &mut console, vec![key(egui::Key::Enter)]);
        pass(&ctx, &mut console, vec![]);

        pass(&ctx, &mut console, vec![key(egui::Key::ArrowUp)]);

        assert_eq!(console.input(), "tgm 1");
    }
}
