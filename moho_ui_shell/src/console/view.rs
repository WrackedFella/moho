//! Drawing the console with egui.

use super::Console;

const HEIGHT: f32 = 300.0;
const MARGIN: f32 = 8.0;

impl<G> Console<G> {
    /// Draws the console over the frame; draws nothing while it is closed. Enter submits the
    /// input line; Up and Down recall earlier lines.
    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let screen_rect = ctx.input(egui::InputState::viewport_rect);
        let frame = egui::Frame::new()
            .fill(egui::Color32::from_rgba_premultiplied(20, 20, 30, 180))
            .inner_margin(egui::Margin::same(MARGIN as _));
        let mut submitted = None;

        // Foreground order so the console paints over anything else the pass draws.
        egui::Area::new("console_panel".into())
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(screen_rect.min.x, screen_rect.max.y - HEIGHT))
            .show(ctx, |ui| {
                frame.show(ui, |ui| {
                    let inner_height = HEIGHT - MARGIN * 2.0;
                    ui.set_min_width(screen_rect.width() - MARGIN * 2.0);
                    ui.set_height(inner_height);

                    // Bottom-up, so the input line sits at the bottom and the output above it.
                    ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(">")
                                    .family(egui::FontFamily::Monospace)
                                    .strong(),
                            );
                            let response = ui.add(
                                egui::TextEdit::singleline(&mut self.input)
                                    .font(egui::FontId::monospace(14.0))
                                    .desired_width(f32::INFINITY),
                            );
                            if self.focus_input {
                                response.request_focus();
                                self.focus_input = false;
                            }
                            if response.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                submitted = Some(std::mem::take(&mut self.input));
                                self.focus_input = true;
                            }
                            if response.has_focus() {
                                if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                                    if let Some(line) = self.output.navigate_up() {
                                        self.input = line;
                                    }
                                } else if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                                    self.input = self.output.navigate_down().unwrap_or_default();
                                }
                            }
                        });
                        ui.add_space(4.0);
                        ui.separator();
                        ui.add_space(8.0);
                        egui::ScrollArea::vertical()
                            .stick_to_bottom(true)
                            .max_height(inner_height - 50.0)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                // Top-down inside the bottom-up layout, so the newest line is last.
                                ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                                    ui.style_mut().spacing.item_spacing = egui::vec2(0.0, 2.0);
                                    ui.set_width(ui.available_width());
                                    for line in self.output.lines() {
                                        ui.label(
                                            egui::RichText::new(line)
                                                .family(egui::FontFamily::Monospace),
                                        );
                                    }
                                });
                            });
                    });
                });
            });

        // An area draws nothing on its first pass while egui measures it: run the pass again so
        // the first frame after the console opens is not blank.
        if !self.sized {
            self.sized = true;
            ctx.request_discard("console area sizing pass");
        }
        if let Some(line) = submitted {
            self.submit(&line);
        }
    }
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
