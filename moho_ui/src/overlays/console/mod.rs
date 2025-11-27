//! Debug console overlay for in-game debugging and commands.
//!
//! The console provides a text-based interface for executing commands,
//! viewing log output, and debugging game state during gameplay.

mod commands;
mod output;

use commands::CommandProcessor;
pub use commands::{CommandResult, ConsoleAction};
use output::ConsoleOutput;

/// Console overlay component for debugging and command execution.
///
/// The console does NOT manage its own visibility - that's handled by GameState.
/// This component only handles rendering and input when it's active.
///
/// # Example (usage sketch)
///
/// ```rust,ignore
/// // The adapter receives `GameState::ConsoleOpen` and calls `console.render(&ctx)` each frame.
/// let mut console = moho_ui::overlays::console::Console::new();
/// // User types 'god' and presses Enter — the adapter receives a ConsoleAction::ToggleGodMode
/// // and publishes `DebugEvent::ToggleGodMode` on the EventBus for subscribers to handle.
/// ```
pub struct Console {
    /// Current input text being typed
    input_buffer: String,

    /// Command processor for parsing and executing commands
    commands: CommandProcessor,

    /// Output manager for display and history
    output: ConsoleOutput,

    /// Whether input field should be focused
    focus_input: bool,

    /// Set to true when console is first opened, prevents immediate close
    just_opened: bool,
}

impl Console {
    /// Create a new console overlay.
    pub fn new() -> Self {
        Self {
            input_buffer: String::new(),
            commands: CommandProcessor::new(),
            output: ConsoleOutput::new(),
            focus_input: true,
            just_opened: true,
        }
    }

    /// Render the console overlay.
    ///
    /// Returns a `ConsoleAction` if a command was executed that needs
    /// to be processed by the application.
    ///
    /// # Arguments
    ///
    /// * `ctx` - The egui context to render into
    pub fn render(&mut self, ctx: &egui::Context) -> ConsoleAction {
        let mut action = ConsoleAction::None;

        // Check for backtick key press BEFORE egui processes input
        // This ensures backtick closes the console even when text field has focus
        // Skip on first frame to prevent immediate close when console is opened
        if !self.just_opened && ctx.input(|i| i.key_pressed(egui::Key::Backtick)) {
            return ConsoleAction::Close;
        }

        // Clear the just_opened flag after first frame
        self.just_opened = false;

        // Create a custom frame with semi-transparent background
        // Note: This transparency pattern can be reused for other overlays
        let frame = egui::Frame::new()
            .fill(egui::Color32::from_rgba_premultiplied(20, 20, 30, 180))
            .inner_margin(egui::Margin::same(8));

        // Console panel at bottom of screen
        egui::TopBottomPanel::bottom("console_panel")
            .resizable(false)
            .exact_height(300.0)
            .frame(frame)
            .show(ctx, |ui| {
                // Use vertical layout with bottom-to-top ordering
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    // Bottom padding to prevent input from being cut off
                    ui.add_space(4.0);

                    // Input field (rendered first, appears at bottom)
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
                            action = self.execute_command();
                            self.focus_input = true; // Re-focus for next command
                        }

                        // Handle history navigation
                        if response.has_focus() {
                            if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                                if let Some(cmd) = self.output.navigate_up() {
                                    self.input_buffer = cmd;
                                }
                            } else if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                                match self.output.navigate_down() {
                                    Some(cmd) => self.input_buffer = cmd,
                                    None => self.input_buffer.clear(),
                                }
                            }
                        }
                    });

                    ui.add_space(4.0);
                    ui.separator();
                    ui.add_space(8.0);

                    // Output area (scrollable) - rendered last, appears at top
                    egui::ScrollArea::vertical()
                        .stick_to_bottom(true)
                        .max_height(250.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.style_mut().spacing.item_spacing = egui::vec2(0.0, 2.0);

                            // Use full available width for output
                            ui.set_width(ui.available_width());

                            for line in self.output.lines() {
                                ui.label(
                                    egui::RichText::new(line).family(egui::FontFamily::Monospace),
                                );
                            }
                        });
                });
            });

        action
    }

    /// Execute the current command in the input buffer.
    fn execute_command(&mut self) -> ConsoleAction {
        let command = self.input_buffer.trim().to_string();

        if command.is_empty() {
            return ConsoleAction::None;
        }

        // Echo command to output
        self.output.add_line(format!("> {}", command));

        // Add to history
        self.output.add_to_history(command.clone());

        // Special case: handle clear command (needs direct access to output)
        if command.to_lowercase() == "clear" {
            self.output.clear_output();
            self.input_buffer.clear();
            return ConsoleAction::None;
        }

        // Execute command and add output messages
        let result = self.commands.execute(&command);
        for message in result.messages {
            self.output.add_line(message);
        }

        // Clear input
        self.input_buffer.clear();

        result.action
    }

    /// Add a message to the console output (for external logging).
    pub fn log(&mut self, message: String) {
        self.output.add_line(message);
    }

    /// Clear all output.
    pub fn clear(&mut self) {
        self.output.clear_output();
    }

    /// Reset the console state when reopening (prevents immediate close).
    pub fn reset_on_open(&mut self) {
        self.just_opened = true;
        self.focus_input = true;
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
        assert!(console.output.lines().len() >= 2); // Welcome messages
    }

    #[test]
    fn test_log() {
        let mut console = Console::new();
        let initial_len = console.output.lines().len();
        console.log("Test message".to_string());
        assert_eq!(console.output.lines().len(), initial_len + 1);
        assert_eq!(
            console.output.lines().back(),
            Some(&"Test message".to_string())
        );
    }

    #[test]
    fn test_clear() {
        let mut console = Console::new();
        console.log("Test 1".to_string());
        console.log("Test 2".to_string());

        console.clear();
        assert_eq!(console.output.lines().len(), 0);
    }

    #[test]
    fn test_execute_help_command() {
        let mut console = Console::new();
        console.output.clear_output();
        console.input_buffer = "help".to_string();

        let action = console.execute_command();

        assert_eq!(action, ConsoleAction::None);
        assert!(
            console
                .output
                .lines()
                .iter()
                .any(|line| line.contains("Available commands"))
        );
        assert!(console.input_buffer.is_empty());
    }

    #[test]
    fn test_execute_quit_command() {
        let mut console = Console::new();
        console.input_buffer = "quit".to_string();

        let action = console.execute_command();

        assert_eq!(action, ConsoleAction::Quit);
        assert!(console.input_buffer.is_empty());
    }

    #[test]
    fn test_execute_clear_command() {
        let mut console = Console::new();
        console.log("Test 1".to_string());
        console.log("Test 2".to_string());
        console.input_buffer = "clear".to_string();

        let action = console.execute_command();

        assert_eq!(action, ConsoleAction::None);
        assert_eq!(console.output.lines().len(), 0);
    }

    #[test]
    fn test_execute_unknown_command() {
        let mut console = Console::new();
        console.output.clear_output();
        console.input_buffer = "unknowncommand".to_string();

        let action = console.execute_command();

        assert_eq!(action, ConsoleAction::None);
        assert!(
            console
                .output
                .lines()
                .iter()
                .any(|line| line.contains("Unknown command"))
        );
    }

    #[test]
    fn test_reset_on_open() {
        let mut console = Console::new();
        console.just_opened = false;
        console.focus_input = false;

        console.reset_on_open();

        assert!(console.just_opened);
        assert!(console.focus_input);
    }
}
