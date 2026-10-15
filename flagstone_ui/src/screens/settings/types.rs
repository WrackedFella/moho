use crate::actions::StrategyAction;

/// Represents the different tabs available in the Settings menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SettingsTab {
    #[default]
    Controls,
    Audio,
    Video,
}

impl SettingsTab {
    /// Convert a tab index to a SettingsTab variant.
    ///
    /// # Arguments
    /// * `index` - Zero-based tab index (0 = Controls, 1 = Audio, 2 = Video)
    ///
    /// # Returns
    /// The corresponding SettingsTab, defaulting to Controls if index is out of range
    pub fn from_index(index: usize) -> Self {
        match index {
            0 => SettingsTab::Controls,
            1 => SettingsTab::Audio,
            2 => SettingsTab::Video,
            _ => SettingsTab::Controls, // Default fallback
        }
    }

    /// Convert a SettingsTab to its corresponding index.
    ///
    /// # Returns
    /// Zero-based tab index
    pub fn to_index(self) -> usize {
        match self {
            SettingsTab::Controls => 0,
            SettingsTab::Audio => 1,
            SettingsTab::Video => 2,
        }
    }

    /// Get all available tabs in display order.
    pub fn all_tabs() -> &'static [&'static str] {
        &["Controls", "Audio", "Video"]
    }
}

/// The actions the settings screen lists, in row order. `Jump` is absent: it
/// shares Space with `Ascend`.
pub const BINDING_ROWS: [StrategyAction; 7] = [
    StrategyAction::MoveForward,
    StrategyAction::MoveLeft,
    StrategyAction::MoveBack,
    StrategyAction::MoveRight,
    StrategyAction::Ascend,
    StrategyAction::Descend,
    StrategyAction::Sprint,
];

/// The action for a numeric row/listen id, or `None` when out of range.
pub fn row_action(id: usize) -> Option<StrategyAction> {
    BINDING_ROWS.get(id).copied()
}

/// Name shown in the settings UI.
pub fn display_name(action: StrategyAction) -> &'static str {
    match action {
        StrategyAction::MoveForward => "Move Forward",
        StrategyAction::MoveLeft => "Move Left",
        StrategyAction::MoveBack => "Move Back",
        StrategyAction::MoveRight => "Move Right",
        StrategyAction::Ascend => "Move Up",
        StrategyAction::Descend => "Move Down",
        StrategyAction::Sprint => "Sprint",
        StrategyAction::Jump => "Jump",
    }
}
