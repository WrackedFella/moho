//! Console output and history management.
//!
//! Handles output buffer management, command history navigation,
//! and line limiting for the debug console.

use std::collections::VecDeque;

/// Maximum number of history entries to keep
const MAX_HISTORY: usize = 100;

/// Maximum number of output lines to display
const MAX_OUTPUT_LINES: usize = 20;

/// Console output manager for handling output display and command history.
///
/// This module manages:
/// - Output line buffer with automatic limiting
/// - Command history with navigation (up/down arrows)
/// - History index tracking for navigation state
pub struct ConsoleOutput {
    /// Output lines to display (commands + results)
    output: VecDeque<String>,
    
    /// Command history (recent commands)
    history: VecDeque<String>,
    
    /// Current position in history (for up/down arrow navigation)
    history_index: Option<usize>,
}

impl ConsoleOutput {
    /// Create a new console output manager with welcome message.
    pub fn new() -> Self {
        let mut output = VecDeque::with_capacity(MAX_OUTPUT_LINES);
        output.push_back("Debug Console".to_string());
        output.push_back("Type 'help' for available commands".to_string());
        output.push_back("".to_string());
        
        Self {
            output,
            history: VecDeque::with_capacity(MAX_HISTORY),
            history_index: None,
        }
    }
    
    /// Add a line to the output buffer.
    ///
    /// If the output exceeds MAX_OUTPUT_LINES, the oldest line is removed.
    pub fn add_line(&mut self, line: String) {
        self.output.push_back(line);
        if self.output.len() > MAX_OUTPUT_LINES {
            self.output.pop_front();
        }
    }
    
    /// Add a command to the history buffer.
    ///
    /// If the history exceeds MAX_HISTORY, the oldest command is removed.
    /// Resets the history navigation index.
    pub fn add_to_history(&mut self, command: String) {
        self.history.push_back(command);
        if self.history.len() > MAX_HISTORY {
            self.history.pop_front();
        }
        self.history_index = None;
    }
    
    /// Navigate up in command history.
    ///
    /// Returns the command at the new position, or None if history is empty.
    pub fn navigate_up(&mut self) -> Option<String> {
        if self.history.is_empty() {
            return None;
        }
        
        let new_index = match self.history_index {
            None => Some(self.history.len() - 1),
            Some(idx) if idx > 0 => Some(idx - 1),
            Some(idx) => Some(idx),
        };
        
        self.history_index = new_index;
        new_index.map(|idx| self.history[idx].clone())
    }
    
    /// Navigate down in command history.
    ///
    /// Returns the command at the new position, or None to indicate
    /// the user has navigated past the end of history (clear input).
    pub fn navigate_down(&mut self) -> Option<String> {
        if self.history.is_empty() {
            return None;
        }
        
        let new_index = match self.history_index {
            None => None,
            Some(idx) if idx < self.history.len() - 1 => Some(idx + 1),
            Some(_) => None, // At the end of history
        };
        
        self.history_index = new_index;
        new_index.map(|idx| self.history[idx].clone())
    }
    
    /// Get a reference to all output lines.
    pub fn lines(&self) -> &VecDeque<String> {
        &self.output
    }
    
    /// Clear all output lines.
    pub fn clear_output(&mut self) {
        self.output.clear();
    }
    
    /// Get the number of commands in history.
    #[cfg(test)]
    pub fn history_len(&self) -> usize {
        self.history.len()
    }
    
    /// Get the current history index.
    #[cfg(test)]
    pub fn history_index(&self) -> Option<usize> {
        self.history_index
    }
}

impl Default for ConsoleOutput {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_new_output() {
        let output = ConsoleOutput::new();
        assert!(output.lines().len() >= 2); // Welcome messages
        assert_eq!(output.history_len(), 0);
        assert_eq!(output.history_index(), None);
    }
    
    #[test]
    fn test_add_line() {
        let mut output = ConsoleOutput::new();
        let initial_len = output.lines().len();
        
        output.add_line("Test message".to_string());
        assert_eq!(output.lines().len(), initial_len + 1);
        assert_eq!(output.lines().back(), Some(&"Test message".to_string()));
    }
    
    #[test]
    fn test_output_max_lines() {
        let mut output = ConsoleOutput::new();
        output.clear_output();
        
        // Add more than MAX_OUTPUT_LINES
        for i in 0..MAX_OUTPUT_LINES + 10 {
            output.add_line(format!("Line {}", i));
        }
        
        assert_eq!(output.lines().len(), MAX_OUTPUT_LINES);
        // Should have kept the most recent lines
        assert_eq!(
            output.lines().back(),
            Some(&format!("Line {}", MAX_OUTPUT_LINES + 9))
        );
    }
    
    #[test]
    fn test_add_to_history() {
        let mut output = ConsoleOutput::new();
        
        output.add_to_history("cmd1".to_string());
        output.add_to_history("cmd2".to_string());
        
        assert_eq!(output.history_len(), 2);
        assert_eq!(output.history_index(), None); // Reset after adding
    }
    
    #[test]
    fn test_history_navigation() {
        let mut output = ConsoleOutput::new();
        output.add_to_history("cmd1".to_string());
        output.add_to_history("cmd2".to_string());
        
        // Navigate up (should get cmd2)
        let cmd = output.navigate_up();
        assert_eq!(cmd, Some("cmd2".to_string()));
        assert_eq!(output.history_index(), Some(1));
        
        // Navigate up again (should get cmd1)
        let cmd = output.navigate_up();
        assert_eq!(cmd, Some("cmd1".to_string()));
        assert_eq!(output.history_index(), Some(0));
        
        // Navigate up again (should stay at cmd1)
        let cmd = output.navigate_up();
        assert_eq!(cmd, Some("cmd1".to_string()));
        assert_eq!(output.history_index(), Some(0));
        
        // Navigate down (should get cmd2)
        let cmd = output.navigate_down();
        assert_eq!(cmd, Some("cmd2".to_string()));
        assert_eq!(output.history_index(), Some(1));
        
        // Navigate down again (should return None, indicating clear input)
        let cmd = output.navigate_down();
        assert_eq!(cmd, None);
        assert_eq!(output.history_index(), None);
    }
    
    #[test]
    fn test_clear_output() {
        let mut output = ConsoleOutput::new();
        output.add_line("Test 1".to_string());
        output.add_line("Test 2".to_string());
        
        output.clear_output();
        assert_eq!(output.lines().len(), 0);
    }
    
    #[test]
    fn test_history_max_size() {
        let mut output = ConsoleOutput::new();
        
        // Add more than MAX_HISTORY commands
        for i in 0..MAX_HISTORY + 10 {
            output.add_to_history(format!("cmd{}", i));
        }
        
        assert_eq!(output.history_len(), MAX_HISTORY);
    }
}
