//! Console command processing and execution.
//!
//! Handles command parsing, execution, and help text generation
//! for the debug console.

/// Console command action to be processed by the application
#[derive(Debug, Clone, PartialEq)]
pub enum ConsoleAction {
    /// Quit the application
    Quit,
    /// Toggle god mode
    ToggleGodMode,
    /// Toggle noclip mode
    ToggleNoclip,
    /// Close the console (triggered by backtick key)
    Close,
    /// Set sun direction (yaw, pitch in degrees)
    SetSunDirection(f32, f32),
    /// Set time of day in hours (0.0 = midnight, 6.0 = dawn, 12.0 = noon, 18.0 = dusk, 24.0 = midnight)
    SetTimeOfDay(f32),
    /// Set debug view mode (0=None, 1=Normals, 2=Bias, 3=Cascades, 4=Shadows)
    SetDebugView(u32),
    /// Set shadow quality (0=Off, 1=Low, 2=Medium, 3=High, 4=Ultra)
    SetShadowQuality(u32),
    /// Set SSAO quality (0=Off, 1=Low, 2=Medium, 3=High, 4=Ultra)
    SetSsaoQuality(u32),
    /// Spawn an entity or block (type, optional material/args)
    Spawn(String, Option<String>),
    /// No action
    None,
}

/// Command execution result with output messages and action
#[derive(Debug)]
pub struct CommandResult {
    /// Output messages to display in console
    pub messages: Vec<String>,
    /// Action to be processed by application
    pub action: ConsoleAction,
}

impl CommandResult {
    /// Create a new command result with messages and action
    pub fn new(messages: Vec<String>, action: ConsoleAction) -> Self {
        Self { messages, action }
    }

    /// Create a command result with a single message and no action
    pub fn message(msg: String) -> Self {
        Self {
            messages: vec![msg],
            action: ConsoleAction::None,
        }
    }

    /// Create a command result with multiple messages and no action
    pub fn messages(messages: Vec<String>) -> Self {
        Self {
            messages,
            action: ConsoleAction::None,
        }
    }

    /// Create a command result with no messages and an action
    pub fn action(action: ConsoleAction) -> Self {
        Self {
            messages: Vec::new(),
            action,
        }
    }

    /// Create a command result with a message and an action
    pub fn with_action(msg: String, action: ConsoleAction) -> Self {
        Self {
            messages: vec![msg],
            action,
        }
    }
}

/// Command processor for parsing and executing console commands
pub struct CommandProcessor;

impl CommandProcessor {
    /// Create a new command processor
    pub fn new() -> Self {
        Self
    }

    /// Parse and execute a command string
    pub fn execute(&self, command: &str) -> CommandResult {
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return CommandResult::action(ConsoleAction::None);
        }

        match parts[0].to_lowercase().as_str() {
            "help" => self.help_command(),
            "clear" => CommandResult::action(ConsoleAction::None), // Clear handled by Console
            "quit" => self.quit_command(),
            "god" => self.god_command(),
            "noclip" => self.noclip_command(),
            "sun" => self.sun_command(&parts),
            "time" => self.time_command(&parts),
            "r_debug_view" => self.debug_view_command(&parts),
            "r_shadow_quality" => self.shadow_quality_command(&parts),
            "r_ssao_quality" => self.ssao_quality_command(&parts),
            "spawn" => self.spawn_command(&parts),
            _ => self.unknown_command(parts[0]),
        }
    }

    /// Generate help text for all available commands
    fn help_command(&self) -> CommandResult {
        CommandResult::messages(vec![
            "Available commands:".to_string(),
            "  help - Show this help message".to_string(),
            "  clear - Clear the console output".to_string(),
            "  quit - Exit the application".to_string(),
            "  god - Toggle god mode (invincibility)".to_string(),
            "  noclip - Toggle noclip mode (fly through walls)".to_string(),
            "  sun <yaw> <pitch> - Set sun direction (degrees)".to_string(),
            "  time <0-24> - Set time of day (0=midnight, 6=dawn, 12=noon, 18=dusk)".to_string(),
            "  r_debug_view <mode> - Set debug view mode".to_string(),
            "  r_shadow_quality <0-4> - Set shadow quality (Off/Low/Med/High/Ultra)".to_string(),
            "  r_ssao_quality <0-4> - Set SSAO quality (Off/Low/Med/High/Ultra)".to_string(),
        ])
    }

    /// Handle quit command
    fn quit_command(&self) -> CommandResult {
        CommandResult::with_action("Exiting application...".to_string(), ConsoleAction::Quit)
    }

    /// Handle god mode toggle command
    fn god_command(&self) -> CommandResult {
        CommandResult::with_action(
            "Toggling god mode...".to_string(),
            ConsoleAction::ToggleGodMode,
        )
    }

    /// Handle noclip toggle command
    fn noclip_command(&self) -> CommandResult {
        CommandResult::with_action(
            "Toggling noclip mode...".to_string(),
            ConsoleAction::ToggleNoclip,
        )
    }

    /// Handle sun direction command
    fn sun_command(&self, parts: &[&str]) -> CommandResult {
        if parts.len() != 3 {
            return CommandResult::messages(vec![
                "Usage: sun <yaw> <pitch>".to_string(),
                "  Example: sun 45 60".to_string(),
            ]);
        }

        match (parts[1].parse::<f32>(), parts[2].parse::<f32>()) {
            (Ok(yaw), Ok(pitch)) => CommandResult::with_action(
                format!("Setting sun direction: yaw={}, pitch={}", yaw, pitch),
                ConsoleAction::SetSunDirection(yaw, pitch),
            ),
            _ => CommandResult::message("Error: yaw and pitch must be numbers".to_string()),
        }
    }

    /// Handle time of day command
    fn time_command(&self, parts: &[&str]) -> CommandResult {
        if parts.len() != 2 {
            return CommandResult::messages(vec![
                "Usage: time <0-24>".to_string(),
                "  0 = midnight, 6 = dawn, 12 = noon, 18 = dusk, 24 = midnight".to_string(),
            ]);
        }

        match parts[1].parse::<f32>() {
            Ok(time) => {
                let clamped = time.clamp(0.0, 24.0);
                let hours = clamped.floor() as u32;
                let minutes = ((clamped.fract() * 60.0) as u32).min(59);
                CommandResult::with_action(
                    format!("Setting time to {:02}:{:02}", hours, minutes),
                    ConsoleAction::SetTimeOfDay(clamped),
                )
            }
            Err(_) => {
                CommandResult::message("Error: time must be a number between 0 and 24".to_string())
            }
        }
    }

    /// Handle debug view command
    fn debug_view_command(&self, parts: &[&str]) -> CommandResult {
        if parts.len() < 2 {
            return CommandResult::message("Usage: r_debug_view <mode> (0=None, 1=Normals, 2=Bias, 3=Shadows, 4=LightLevel, 5=PointLights)".to_string());
        }

        match parts[1].parse::<u32>() {
            Ok(mode) => {
                let mode_name = match mode {
                    0 => "None",
                    1 => "Normals",
                    2 => "Bias",
                    3 => "Shadows",
                    4 => "Light Level",
                    5 => "Point Lights",
                    _ => "Unknown",
                };
                CommandResult::with_action(
                    format!("Debug view set to: {} ({})", mode_name, mode),
                    ConsoleAction::SetDebugView(mode),
                )
            }
            Err(_) => CommandResult::message("Invalid mode. Must be a number.".to_string()),
        }
    }

    /// Handle shadow quality command
    fn shadow_quality_command(&self, parts: &[&str]) -> CommandResult {
        if parts.len() < 2 {
            return CommandResult::message(
                "Usage: r_shadow_quality <0-4> (0=Off, 1=Low, 2=Med, 3=High, 4=Ultra)".to_string(),
            );
        }

        match parts[1].parse::<u32>() {
            Ok(quality) => {
                if quality > 4 {
                    return CommandResult::message("Quality must be between 0 and 4".to_string());
                }
                let quality_name = match quality {
                    0 => "Off",
                    1 => "Low",
                    2 => "Medium",
                    3 => "High",
                    4 => "Ultra",
                    _ => "Unknown",
                };
                CommandResult::with_action(
                    format!("Shadow quality set to: {} ({})", quality_name, quality),
                    ConsoleAction::SetShadowQuality(quality),
                )
            }
            Err(_) => CommandResult::message("Invalid quality. Must be a number.".to_string()),
        }
    }

    /// Handle SSAO quality command
    fn ssao_quality_command(&self, parts: &[&str]) -> CommandResult {
        if parts.len() < 2 {
            return CommandResult::message(
                "Usage: r_ssao_quality <0-4> (0=Off, 1=Low, 2=Med, 3=High, 4=Ultra)".to_string(),
            );
        }

        match parts[1].parse::<u32>() {
            Ok(quality) => {
                if quality > 4 {
                    return CommandResult::message("Quality must be between 0 and 4".to_string());
                }
                let quality_name = match quality {
                    0 => "Off",
                    1 => "Low",
                    2 => "Medium",
                    3 => "High",
                    4 => "Ultra",
                    _ => "Unknown",
                };
                CommandResult::with_action(
                    format!("SSAO quality set to: {} ({})", quality_name, quality),
                    ConsoleAction::SetSsaoQuality(quality),
                )
            }
            Err(_) => CommandResult::message("Invalid quality. Must be a number.".to_string()),
        }
    }

    /// Handle spawn command
    fn spawn_command(&self, parts: &[&str]) -> CommandResult {
        if parts.len() < 2 {
            return CommandResult::message("Usage: spawn <type> [material/args]".to_string());
        }

        let entity_type = parts[1].to_string();
        let args = if parts.len() > 2 {
            Some(parts[2..].join(" "))
        } else {
            None
        };

        CommandResult::with_action(
            format!("Spawning {}...", entity_type),
            ConsoleAction::Spawn(entity_type, args),
        )
    }

    /// Handle unknown command
    fn unknown_command(&self, command: &str) -> CommandResult {
        CommandResult::messages(vec![
            format!("Unknown command: '{}'", command),
            "Type 'help' for available commands".to_string(),
        ])
    }
}

impl Default for CommandProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_help_command() {
        let processor = CommandProcessor::new();
        let result = processor.execute("help");

        assert!(!result.messages.is_empty());
        assert!(
            result
                .messages
                .iter()
                .any(|m| m.contains("Available commands"))
        );
        assert_eq!(result.action, ConsoleAction::None);
    }

    #[test]
    fn test_quit_command() {
        let processor = CommandProcessor::new();
        let result = processor.execute("quit");

        assert!(!result.messages.is_empty());
        assert_eq!(result.action, ConsoleAction::Quit);
    }

    #[test]
    fn test_god_command() {
        let processor = CommandProcessor::new();
        let result = processor.execute("god");

        assert!(!result.messages.is_empty());
        assert_eq!(result.action, ConsoleAction::ToggleGodMode);
    }

    #[test]
    fn test_noclip_command() {
        let processor = CommandProcessor::new();
        let result = processor.execute("noclip");

        assert!(!result.messages.is_empty());
        assert_eq!(result.action, ConsoleAction::ToggleNoclip);
    }

    #[test]
    fn test_sun_command_valid() {
        let processor = CommandProcessor::new();
        let result = processor.execute("sun 45 60");

        assert!(!result.messages.is_empty());
        assert_eq!(result.action, ConsoleAction::SetSunDirection(45.0, 60.0));
    }

    #[test]
    fn test_sun_command_invalid_args() {
        let processor = CommandProcessor::new();
        let result = processor.execute("sun 45");

        assert!(result.messages.iter().any(|m| m.contains("Usage:")));
        assert_eq!(result.action, ConsoleAction::None);
    }

    #[test]
    fn test_sun_command_invalid_numbers() {
        let processor = CommandProcessor::new();
        let result = processor.execute("sun abc def");

        assert!(result.messages.iter().any(|m| m.contains("Error:")));
        assert_eq!(result.action, ConsoleAction::None);
    }

    #[test]
    fn test_time_command_valid() {
        let processor = CommandProcessor::new();
        let result = processor.execute("time 12.5");

        assert!(!result.messages.is_empty());
        assert_eq!(result.action, ConsoleAction::SetTimeOfDay(12.5));
    }

    #[test]
    fn test_time_command_clamping() {
        let processor = CommandProcessor::new();
        let result = processor.execute("time 25");

        // Should clamp to 24.0
        assert_eq!(result.action, ConsoleAction::SetTimeOfDay(24.0));
    }

    #[test]
    fn test_time_command_invalid() {
        let processor = CommandProcessor::new();
        let result = processor.execute("time abc");

        assert!(result.messages.iter().any(|m| m.contains("Error:")));
        assert_eq!(result.action, ConsoleAction::None);
    }

    #[test]
    fn test_unknown_command() {
        let processor = CommandProcessor::new();
        let result = processor.execute("unknowncommand");

        assert!(
            result
                .messages
                .iter()
                .any(|m| m.contains("Unknown command"))
        );
        assert_eq!(result.action, ConsoleAction::None);
    }

    #[test]
    fn test_empty_command() {
        let processor = CommandProcessor::new();
        let result = processor.execute("");

        assert_eq!(result.action, ConsoleAction::None);
    }

    #[test]
    fn test_case_insensitive() {
        let processor = CommandProcessor::new();
        let result1 = processor.execute("QUIT");
        let result2 = processor.execute("Quit");
        let result3 = processor.execute("quit");

        assert_eq!(result1.action, ConsoleAction::Quit);
        assert_eq!(result2.action, ConsoleAction::Quit);
        assert_eq!(result3.action, ConsoleAction::Quit);
    }

    #[test]
    fn test_spawn_command_valid() {
        let processor = CommandProcessor::new();
        let result = processor.execute("spawn tree oakwood");

        assert!(!result.messages.is_empty());
        assert_eq!(
            result.action,
            ConsoleAction::Spawn("tree".to_string(), Some("oakwood".to_string()))
        );
    }

    #[test]
    fn test_spawn_command_missing_type() {
        let processor = CommandProcessor::new();
        let result = processor.execute("spawn");

        assert!(result.messages.iter().any(|m| m.contains("Usage:")));
        assert_eq!(result.action, ConsoleAction::None);
    }

    #[test]
    fn test_spawn_command_invalid_type() {
        let processor = CommandProcessor::new();
        let result = processor.execute("spawn 123");

        assert!(
            result
                .messages
                .iter()
                .any(|m| m.contains("Spawning 123"))
        );
        assert_eq!(result.action, ConsoleAction::Spawn("123".to_string(), None));
    }
}
