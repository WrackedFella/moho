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

/// Type-safe identifier for key bindings in the settings menu.
/// 
/// Replaces magic numbers (0, 1, 2, 3, 4, 5) with descriptive enum variants
/// to improve code clarity and prevent binding ID errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BindingId {
    KeyW = 0,
    KeyA = 1,
    KeyS = 2,
    KeyD = 3,
    KeyUp = 4,
    KeyDown = 5,
}

impl BindingId {
    /// Convert a BindingId to its corresponding numeric ID.
    ///
    /// # Returns
    /// The numeric ID (0-5) used internally for binding identification.
    pub fn to_usize(self) -> usize {
        self as usize
    }

    /// Convert a numeric ID to a BindingId.
    ///
    /// # Arguments
    /// * `id` - Numeric binding ID (0-5)
    ///
    /// # Returns
    /// The corresponding BindingId, or None if id is out of range.
    pub fn from_usize(id: usize) -> Option<Self> {
        match id {
            0 => Some(BindingId::KeyW),
            1 => Some(BindingId::KeyA),
            2 => Some(BindingId::KeyS),
            3 => Some(BindingId::KeyD),
            4 => Some(BindingId::KeyUp),
            5 => Some(BindingId::KeyDown),
            _ => None,
        }
    }

    /// Get the human-readable name for this binding.
    ///
    /// # Returns
    /// Display name used in UI (e.g., "Move Forward" for KeyW)
    pub fn display_name(self) -> &'static str {
        match self {
            BindingId::KeyW => "Move Forward",
            BindingId::KeyA => "Move Left",
            BindingId::KeyS => "Move Back",
            BindingId::KeyD => "Move Right",
            BindingId::KeyUp => "Move Up",
            BindingId::KeyDown => "Move Down",
        }
    }

    /// Get all binding IDs in order.
    ///
    /// # Returns
    /// Array of all BindingId variants in numeric order.
    #[allow(dead_code)] // Will be used in future refactoring increments
    pub fn all() -> [BindingId; 6] {
        [
            BindingId::KeyW,
            BindingId::KeyA,
            BindingId::KeyS,
            BindingId::KeyD,
            BindingId::KeyUp,
            BindingId::KeyDown,
        ]
    }
}

