//! Debug console overlay for in-game debugging and commands.
//!
//! The console provides a text-based interface for executing commands,
//! viewing log output, and debugging game state during gameplay.

use std::collections::VecDeque;

/// Maximum number of history entries to keep
const MAX_HISTORY: usize = 100;

/// Maximum number of output lines to display
const MAX_OUTPUT_LINES: usize = 20;

/// Console overlay component for debugging and command execution.
///
/// The console does NOT manage its own visibility - that's handled by GameState.
/// This component only handles rendering and input when it's active.
pub struct Console {
    /// Current input text being typed
    input_buffer: String,

    /// Command history (recent commands)
    history: VecDeque<String>,

    /// Current position in history (for up/down arrow navigation)
    history_index: Option<usize>,

    /// Output lines to display (commands + results)
    output: VecDeque<String>,

    /// Whether input field should be focused
    focus_input: bool,
}

impl Console {
    /// Create a new console overlay.
    pub fn new() -> Self {
        let mut console = Self {
            input_buffer: String::new(),
            history: VecDeque::with_capacity(MAX_HISTORY),
            history_index: None,
            output: VecDeque::with_capacity(MAX_OUTPUT_LINES),
            focus_input: true,
        };

        // Add welcome message
        console.output.push_back("Debug Console".to_string());
        console
            .output
            .push_back("Type 'help' for available commands".to_string());
        console.output.push_back("".to_string());

        console
    }

    /// Render the console overlay.
    ///
    /// # Arguments
    ///
    /// * `ctx` - The egui context to render into
    pub fn render(&mut self, ctx: &egui::Context) {
        // Console panel at bottom of screen
        egui::TopBottomPanel::bottom("console_panel")
            .resizable(true)
            .default_height(300.0)
            .min_height(100.0)
            .max_height(600.0)
            .show(ctx, |ui| {
                // Dark background
                ui.visuals_mut().window_fill =
                    egui::Color32::from_rgba_premultiplied(20, 20, 30, 240);

                // Output area (scrollable)
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .max_height(250.0)
                    .show(ui, |ui| {
                        ui.style_mut().spacing.item_spacing = egui::vec2(0.0, 2.0);

                        for line in &self.output {
                            ui.label(egui::RichText::new(line).family(egui::FontFamily::Monospace));
                        }
                    });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // Input field
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(">")
                            .family(egui::FontFamily::Monospace)
                            .strong(),
                    );

                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.input_buffer)
                            .font(egui::FontId::monospace(14.0))
                            .desired_width(f32::INFINITY),
                    );

                    // Auto-focus input on first frame
                    if self.focus_input {
                        response.request_focus();
                        self.focus_input = false;
                    }

                    // Handle enter key
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.execute_command();
                        self.focus_input = true; // Re-focus for next command
                    }

                    // Handle history navigation
                    if response.has_focus() {
                        if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                            self.history_up();
                        } else if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                            self.history_down();
                        }
                    }
                });
            });
    }

    /// Execute the current command in the input buffer.
    fn execute_command(&mut self) {
        let command = self.input_buffer.trim().to_string();

        if command.is_empty() {
            return;
        }

        // Add command to output
        self.add_output(format!("> {}", command));

        // Add to history
        if self.history.is_empty() || self.history.back() != Some(&command) {
            self.history.push_back(command.clone());
            if self.history.len() > MAX_HISTORY {
                self.history.pop_front();
            }
        }
        self.history_index = None;

        // Parse and handle command
        self.handle_command(&command);

        // Clear input
        self.input_buffer.clear();
    }

    /// Handle a parsed command.
    fn handle_command(&mut self, command: &str) {
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return;
        }

        match parts[0].to_lowercase().as_str() {
            "help" => {
                self.add_output("Available commands:".to_string());
                self.add_output("  help - Show this help message".to_string());
                self.add_output("  clear - Clear the console output".to_string());
                self.add_output("  quit - Exit the application".to_string());
                self.add_output("  god - Toggle god mode (invincibility)".to_string());
                self.add_output("  noclip - Toggle noclip mode (fly through walls)".to_string());
            }
            "clear" => {
                self.output.clear();
            }
            "quit" => {
                self.add_output("Quit command recognized (event will be published)".to_string());
                // TODO: Publish quit event via EventBus (Phase 3)
            }
            "god" => {
                self.add_output(
                    "God mode command recognized (event will be published)".to_string(),
                );
                // TODO: Publish god mode toggle event (Phase 3)
            }
            "noclip" => {
                self.add_output("Noclip command recognized (event will be published)".to_string());
                // TODO: Publish noclip toggle event (Phase 3)
            }
            _ => {
                self.add_output(format!("Unknown command: '{}'", parts[0]));
                self.add_output("Type 'help' for available commands".to_string());
            }
        }
    }

    /// Add a line to the output buffer.
    fn add_output(&mut self, line: String) {
        self.output.push_back(line);
        if self.output.len() > MAX_OUTPUT_LINES {
            self.output.pop_front();
        }
    }

    /// Navigate up in command history.
    fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }

        let new_index = match self.history_index {
            None => Some(self.history.len() - 1),
            Some(idx) if idx > 0 => Some(idx - 1),
            Some(idx) => Some(idx),
        };

        if let Some(idx) = new_index {
            self.history_index = Some(idx);
            self.input_buffer = self.history[idx].clone();
        }
    }

    /// Navigate down in command history.
    fn history_down(&mut self) {
        if self.history.is_empty() {
            return;
        }

        let new_index = match self.history_index {
            None => None,
            Some(idx) if idx < self.history.len() - 1 => Some(idx + 1),
            Some(_) => {
                // At the end of history, clear input
                self.input_buffer.clear();
                None
            }
        };

        self.history_index = new_index;
        if let Some(idx) = new_index {
            self.input_buffer = self.history[idx].clone();
        }
    }

    /// Add a message to the console output (for external logging).
    pub fn log(&mut self, message: String) {
        self.add_output(message);
    }

    /// Clear all output.
    pub fn clear(&mut self) {
        self.output.clear();
    }
}

impl Default for Console {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_console() {
        let console = Console::new();
        assert!(console.input_buffer.is_empty());
        assert!(console.history.is_empty());
        assert_eq!(console.history_index, None);
        assert!(console.output.len() >= 2); // Welcome messages
    }

    #[test]
    fn test_add_output() {
        let mut console = Console::new();
        let initial_len = console.output.len();
        console.add_output("Test message".to_string());
        assert_eq!(console.output.len(), initial_len + 1);
        assert_eq!(console.output.back(), Some(&"Test message".to_string()));
    }

    #[test]
    fn test_output_max_lines() {
        let mut console = Console::new();
        console.output.clear();

        // Add more than MAX_OUTPUT_LINES
        for i in 0..MAX_OUTPUT_LINES + 10 {
            console.add_output(format!("Line {}", i));
        }

        assert_eq!(console.output.len(), MAX_OUTPUT_LINES);
        // Should have kept the most recent lines
        assert_eq!(
            console.output.back(),
            Some(&format!("Line {}", MAX_OUTPUT_LINES + 9))
        );
    }

    #[test]
    fn test_history() {
        let mut console = Console::new();
        console.input_buffer = "command1".to_string();
        console.execute_command();
        console.input_buffer = "command2".to_string();
        console.execute_command();

        assert_eq!(console.history.len(), 2);
        assert_eq!(console.history[0], "command1");
        assert_eq!(console.history[1], "command2");
    }

    #[test]
    fn test_history_navigation() {
        let mut console = Console::new();
        console.input_buffer = "cmd1".to_string();
        console.execute_command();
        console.input_buffer = "cmd2".to_string();
        console.execute_command();

        // Navigate up (should get cmd2)
        console.history_up();
        assert_eq!(console.input_buffer, "cmd2");

        // Navigate up again (should get cmd1)
        console.history_up();
        assert_eq!(console.input_buffer, "cmd1");

        // Navigate down (should get cmd2)
        console.history_down();
        assert_eq!(console.input_buffer, "cmd2");

        // Navigate down again (should clear)
        console.history_down();
        assert_eq!(console.input_buffer, "");
    }

    #[test]
    fn test_clear_command() {
        let mut console = Console::new();
        console.add_output("Test 1".to_string());
        console.add_output("Test 2".to_string());

        console.handle_command("clear");
        assert_eq!(console.output.len(), 0);
    }

    #[test]
    fn test_help_command() {
        let mut console = Console::new();
        console.output.clear();

        console.handle_command("help");
        assert!(console.output.len() > 0);
        assert!(
            console
                .output
                .iter()
                .any(|line| line.contains("Available commands"))
        );
    }

    #[test]
    fn test_unknown_command() {
        let mut console = Console::new();
        console.output.clear();

        console.handle_command("unknowncommand");
        assert!(
            console
                .output
                .iter()
                .any(|line| line.contains("Unknown command"))
        );
    }
}
