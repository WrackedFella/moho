//! Data-driven registry for managing key bindings and conflict detection.
//!
//! This module replaces the manual if-else chains used for binding conflict
//! detection with a clean, testable registry pattern.

use super::types::BindingId;
use crate::prefs::{Binding, Prefs};

/// Registry that manages all key bindings and provides conflict detection.
///
/// Instead of manually checking each binding field against every other field,
/// the registry provides a unified API for:
/// - Finding conflicts between bindings
/// - Updating bindings by ID
/// - Iterating over all bindings
#[derive(Clone, Debug)]
#[allow(dead_code)] // Methods will be used in subsequent refactoring increments
pub struct BindingRegistry {
    bindings: [(BindingId, Binding); 6],
}

#[allow(dead_code)] // Methods will be used in subsequent refactoring increments
impl BindingRegistry {
    /// Create a new binding registry from Prefs.
    ///
    /// # Arguments
    /// * `prefs` - The preferences containing current binding values
    ///
    /// # Returns
    /// A new BindingRegistry with all bindings loaded
    pub fn from_prefs(prefs: &Prefs) -> Self {
        Self {
            bindings: [
                (BindingId::KeyW, prefs.key_w),
                (BindingId::KeyA, prefs.key_a),
                (BindingId::KeyS, prefs.key_s),
                (BindingId::KeyD, prefs.key_d),
                (BindingId::KeyUp, prefs.key_up),
                (BindingId::KeyDown, prefs.key_down),
            ],
        }
    }

    /// Write the current bindings back to Prefs.
    ///
    /// # Arguments
    /// * `prefs` - The preferences to update with current binding values
    pub fn write_to_prefs(&self, prefs: &mut Prefs) {
        for (id, binding) in &self.bindings {
            match id {
                BindingId::KeyW => prefs.key_w = *binding,
                BindingId::KeyA => prefs.key_a = *binding,
                BindingId::KeyS => prefs.key_s = *binding,
                BindingId::KeyD => prefs.key_d = *binding,
                BindingId::KeyUp => prefs.key_up = *binding,
                BindingId::KeyDown => prefs.key_down = *binding,
            }
        }
    }

    /// Find a binding conflict.
    ///
    /// Checks if `new_binding` is already assigned to a different binding ID.
    ///
    /// # Arguments
    /// * `new_binding` - The binding to check for conflicts
    /// * `exclude_id` - The binding ID being edited (ignore conflicts with self)
    ///
    /// # Returns
    /// Some(BindingId) if there's a conflict, None if the binding is available
    ///
    /// # Example
    /// ```ignore
    /// let registry = BindingRegistry::from_prefs(&prefs);
    /// let new_binding = Binding::new('W' as u32, 0);
    ///
    /// // Check if 'W' is already bound (excluding KeyA itself)
    /// if let Some(conflicting_id) = registry.find_conflict(&new_binding, BindingId::KeyA) {
    ///     println!("'W' is already bound to {:?}", conflicting_id);
    /// }
    /// ```
    pub fn find_conflict(&self, new_binding: &Binding, exclude_id: BindingId) -> Option<BindingId> {
        // Ignore unbound keys (code 0)
        if new_binding.code == 0 {
            return None;
        }

        self.bindings
            .iter()
            .find(|(id, binding)| *id != exclude_id && binding == new_binding)
            .map(|(id, _)| *id)
    }

    /// Update a specific binding by ID.
    ///
    /// # Arguments
    /// * `id` - The binding ID to update
    /// * `new_binding` - The new binding value
    pub fn update_binding(&mut self, id: BindingId, new_binding: Binding) {
        if let Some((_, binding)) = self.bindings.iter_mut().find(|(bid, _)| *bid == id) {
            *binding = new_binding;
        }
    }

    /// Get the current binding for a specific ID.
    ///
    /// # Arguments
    /// * `id` - The binding ID to query
    ///
    /// # Returns
    /// The current binding, or an unbound binding if ID is invalid
    pub fn get_binding(&self, id: BindingId) -> Binding {
        self.bindings
            .iter()
            .find(|(bid, _)| *bid == id)
            .map(|(_, binding)| *binding)
            .unwrap_or_else(|| Binding::new(0, 0))
    }

    /// Iterate over all bindings in order.
    ///
    /// # Returns
    /// Iterator yielding (BindingId, Binding) tuples
    pub fn iter(&self) -> impl Iterator<Item = &(BindingId, Binding)> {
        self.bindings.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_prefs_loads_all_bindings() {
        let prefs = Prefs::default();
        let registry = BindingRegistry::from_prefs(&prefs);

        assert_eq!(registry.get_binding(BindingId::KeyW), prefs.key_w);
        assert_eq!(registry.get_binding(BindingId::KeyA), prefs.key_a);
        assert_eq!(registry.get_binding(BindingId::KeyS), prefs.key_s);
        assert_eq!(registry.get_binding(BindingId::KeyD), prefs.key_d);
        assert_eq!(registry.get_binding(BindingId::KeyUp), prefs.key_up);
        assert_eq!(registry.get_binding(BindingId::KeyDown), prefs.key_down);
    }

    #[test]
    fn write_to_prefs_updates_all_bindings() {
        let mut prefs = Prefs::default();
        let mut registry = BindingRegistry::from_prefs(&prefs);

        // Modify some bindings
        registry.update_binding(BindingId::KeyW, Binding::new('Q' as u32, 0));
        registry.update_binding(BindingId::KeyA, Binding::new('E' as u32, 0));

        // Write back to prefs
        registry.write_to_prefs(&mut prefs);

        assert_eq!(prefs.key_w, Binding::new('Q' as u32, 0));
        assert_eq!(prefs.key_a, Binding::new('E' as u32, 0));
    }

    #[test]
    fn find_conflict_detects_duplicate_bindings() {
        let mut prefs = Prefs::default();
        prefs.key_w = Binding::new('W' as u32, 0);
        prefs.key_a = Binding::new('A' as u32, 0);

        let registry = BindingRegistry::from_prefs(&prefs);

        // Try to bind KeyS to 'W' (already bound to KeyW)
        let conflict = registry.find_conflict(&Binding::new('W' as u32, 0), BindingId::KeyS);
        assert_eq!(conflict, Some(BindingId::KeyW));

        // Try to bind KeyS to 'A' (already bound to KeyA)
        let conflict = registry.find_conflict(&Binding::new('A' as u32, 0), BindingId::KeyS);
        assert_eq!(conflict, Some(BindingId::KeyA));
    }

    #[test]
    fn find_conflict_ignores_exclude_id() {
        let mut prefs = Prefs::default();
        prefs.key_w = Binding::new('W' as u32, 0);

        let registry = BindingRegistry::from_prefs(&prefs);

        // Binding KeyW to 'W' again should not conflict with itself
        let conflict = registry.find_conflict(&Binding::new('W' as u32, 0), BindingId::KeyW);
        assert_eq!(conflict, None);
    }

    #[test]
    fn find_conflict_returns_none_for_unique_binding() {
        let prefs = Prefs::default();
        let registry = BindingRegistry::from_prefs(&prefs);

        // 'Q' is not bound to anything by default
        let conflict = registry.find_conflict(&Binding::new('Q' as u32, 0), BindingId::KeyW);
        assert_eq!(conflict, None);
    }

    #[test]
    fn find_conflict_ignores_unbound_keys() {
        let prefs = Prefs::default();
        let registry = BindingRegistry::from_prefs(&prefs);

        // Code 0 means unbound - should never conflict
        let conflict = registry.find_conflict(&Binding::new(0, 0), BindingId::KeyW);
        assert_eq!(conflict, None);
    }

    #[test]
    fn update_binding_modifies_registry() {
        let prefs = Prefs::default();
        let mut registry = BindingRegistry::from_prefs(&prefs);

        let new_binding = Binding::new('Q' as u32, 1); // Ctrl+Q
        registry.update_binding(BindingId::KeyW, new_binding);

        assert_eq!(registry.get_binding(BindingId::KeyW), new_binding);
    }

    #[test]
    fn iter_returns_all_bindings_in_order() {
        let prefs = Prefs::default();
        let registry = BindingRegistry::from_prefs(&prefs);

        let bindings: Vec<_> = registry.iter().collect();
        assert_eq!(bindings.len(), 6);

        assert_eq!(bindings[0].0, BindingId::KeyW);
        assert_eq!(bindings[1].0, BindingId::KeyA);
        assert_eq!(bindings[2].0, BindingId::KeyS);
        assert_eq!(bindings[3].0, BindingId::KeyD);
        assert_eq!(bindings[4].0, BindingId::KeyUp);
        assert_eq!(bindings[5].0, BindingId::KeyDown);
    }
}
