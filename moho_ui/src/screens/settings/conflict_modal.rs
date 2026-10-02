/// State for the keybind conflict modal.
///
/// This module manages the state needed to display and resolve keybinding conflicts.
/// When a user attempts to bind a key that's already assigned to another action,
/// this modal prompts them to confirm whether they want to replace the existing binding.
use crate::prefs::Binding;

/// Represents a pending binding that triggered a conflict.
#[derive(Clone)]
pub struct PendingBinding {
    /// The ID of the binding slot being configured (0-5 for key_w through key_down)
    pub target_id: usize,
    /// The new binding the user wants to assign
    pub binding: Binding,
    /// The ID of the conflicting binding slot, if any
    pub conflicting_id: Option<usize>,
}

/// State for managing the keybind conflict modal.
///
/// This struct encapsulates all the information needed to display and resolve
/// a keybinding conflict.
pub struct ConflictModalState {
    /// Whether the modal should be shown
    show: bool,
    /// The pending binding that caused the conflict
    pending: Option<PendingBinding>,
    /// The display name of the conflicting key (e.g., "Move Forward")
    conflict_key_name: String,
    /// The binding description (e.g., "Ctrl+W")
    conflict_binding_desc: String,
}

impl ConflictModalState {
    /// Create a new conflict modal state (initially hidden).
    pub fn new() -> Self {
        Self {
            show: false,
            pending: None,
            conflict_key_name: String::new(),
            conflict_binding_desc: String::new(),
        }
    }

    /// Show the conflict modal with the given information.
    ///
    /// # Arguments
    /// * `pending` - The pending binding that caused the conflict
    /// * `conflict_key_name` - Display name of the conflicting key
    /// * `conflict_binding_desc` - Description of the binding (e.g., "Ctrl+W")
    pub fn show(
        &mut self,
        pending: PendingBinding,
        conflict_key_name: String,
        conflict_binding_desc: String,
    ) {
        self.show = true;
        self.pending = Some(pending);
        self.conflict_key_name = conflict_key_name;
        self.conflict_binding_desc = conflict_binding_desc;
    }

    /// Check if the modal is currently visible.
    pub fn is_visible(&self) -> bool {
        self.show
    }

    /// Hide the modal.
    pub fn hide(&mut self) {
        self.show = false;
    }

    /// Take the pending binding (removes it from state).
    ///
    /// # Returns
    /// The pending binding if one exists, None otherwise
    pub fn take_pending(&mut self) -> Option<PendingBinding> {
        self.pending.take()
    }

    /// Get the conflict key name.
    pub fn conflict_key_name(&self) -> &str {
        &self.conflict_key_name
    }

    /// Get the conflict binding description.
    pub fn conflict_binding_desc(&self) -> &str {
        &self.conflict_binding_desc
    }

    /// Clear all modal state (used when confirming or canceling).
    pub fn clear(&mut self) {
        self.show = false;
        self.pending = None;
        self.conflict_key_name.clear();
        self.conflict_binding_desc.clear();
    }
}

impl Default for ConflictModalState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_modal_is_hidden() {
        let modal = ConflictModalState::new();
        assert!(!modal.is_visible());
        assert!(modal.pending.is_none());
    }

    #[test]
    fn show_makes_modal_visible() {
        let mut modal = ConflictModalState::new();
        let pending = PendingBinding {
            target_id: 0,
            binding: Binding::new('Q' as u32, 0),
            conflicting_id: Some(1),
        };

        modal.show(pending, "Move Left".to_string(), "Q".to_string());

        assert!(modal.is_visible());
        assert!(modal.pending.is_some());
        assert_eq!(modal.conflict_key_name(), "Move Left");
        assert_eq!(modal.conflict_binding_desc(), "Q");
    }

    #[test]
    fn hide_hides_modal() {
        let mut modal = ConflictModalState::new();
        let pending = PendingBinding {
            target_id: 0,
            binding: Binding::new('Q' as u32, 0),
            conflicting_id: Some(1),
        };

        modal.show(pending, "Move Left".to_string(), "Q".to_string());
        modal.hide();

        assert!(!modal.is_visible());
        // Pending binding should still exist after hide
        assert!(modal.pending.is_some());
    }

    #[test]
    fn take_pending_removes_binding() {
        let mut modal = ConflictModalState::new();
        let pending = PendingBinding {
            target_id: 0,
            binding: Binding::new('Q' as u32, 0),
            conflicting_id: Some(1),
        };

        modal.show(pending, "Move Left".to_string(), "Q".to_string());
        let taken = modal.take_pending();

        assert!(taken.is_some());
        assert_eq!(taken.unwrap().target_id, 0);
        assert!(modal.pending.is_none());
    }

    #[test]
    fn clear_resets_all_state() {
        let mut modal = ConflictModalState::new();
        let pending = PendingBinding {
            target_id: 0,
            binding: Binding::new('Q' as u32, 0),
            conflicting_id: Some(1),
        };

        modal.show(pending, "Move Left".to_string(), "Q".to_string());
        modal.clear();

        assert!(!modal.is_visible());
        assert!(modal.pending.is_none());
        assert_eq!(modal.conflict_key_name(), "");
        assert_eq!(modal.conflict_binding_desc(), "");
    }
}
