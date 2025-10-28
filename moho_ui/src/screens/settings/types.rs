/// Represents the different tabs available in the Settings menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SettingsTab {
    #[default]
    Controls,
    Audio,
}

impl SettingsTab {
    /// Convert a tab index to a SettingsTab variant.
    ///
    /// # Arguments
    /// * `index` - Zero-based tab index (0 = Controls, 1 = Audio)
    ///
    /// # Returns
    /// The corresponding SettingsTab, defaulting to Controls if index is out of range
    pub fn from_index(index: usize) -> Self {
        match index {
            0 => SettingsTab::Controls,
            1 => SettingsTab::Audio,
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
        }
    }

    /// Get all available tabs in display order.
    pub fn all_tabs() -> &'static [&'static str] {
        &["Controls", "Audio"]
    }
}
