#![cfg(feature = "ui-egui")]

use moho_ui::screens::SettingsMenu;
use moho_ui::prefs::Binding;
use moho_ui::UiComponent;

/// Test 1: Binding conflict detection
/// 
/// This test verifies that the settings menu correctly detects when a user
/// attempts to bind a key that's already assigned to another action.
#[test]
fn binding_conflict_detection_works() {
    let mut menu = SettingsMenu::new();
    
    // Start listening for key_w (id = 0)
    menu.start_listening(0);
    assert!(menu.is_listening());
    
    // Apply a binding (e.g., code 87 = 'W' key, no modifiers)
    let consumed = menu.apply_key_code_while_listening(87, 0);
    assert!(consumed, "binding should have been consumed");
    assert!(!menu.is_listening(), "should stop listening after binding");
    
    // Now try to bind the same key to key_a (id = 1)
    menu.start_listening(1);
    let consumed = menu.apply_key_code_while_listening(87, 0);
    assert!(consumed, "conflicting binding should be consumed");
    
    // Should show conflict modal
    assert!(menu.show_conflict_modal, "conflict modal should be shown");
    assert!(!menu.is_listening(), "should stop listening when conflict detected");
    
    // Verify conflict information
    assert_eq!(menu.conflict_key_name, "Move Forward", "should identify conflicting key");
    assert!(!menu.conflict_binding_desc.is_empty(), "should have binding description");
}

/// Test 2: Escaping while listening cancels binding
#[test]
fn escape_cancels_binding_listen() {
    let mut menu = SettingsMenu::new();
    
    menu.start_listening(0);
    assert!(menu.is_listening());
    
    // Press escape (code 0x200)
    let consumed = menu.apply_key_code_while_listening(0x200, 0);
    assert!(consumed, "escape should be consumed");
    assert!(!menu.is_listening(), "should stop listening after escape");
    assert!(!menu.show_conflict_modal, "should not show conflict modal");
}

/// Test 3: Tab switching functionality
#[test]
fn tab_switching_works() {
    use moho_ui::screens::SettingsTab;
    
    let mut menu = SettingsMenu::new();
    
    // Default tab should be Controls
    assert_eq!(menu.active_tab(), SettingsTab::Controls);
    
    // Switch to Audio tab
    menu.set_active_tab(SettingsTab::Audio);
    assert_eq!(menu.active_tab(), SettingsTab::Audio);
    
    // Switch back to Controls
    menu.set_active_tab(SettingsTab::Controls);
    assert_eq!(menu.active_tab(), SettingsTab::Controls);
}

/// Test 4: Staged changes and dirty tracking
#[test]
fn staged_changes_tracked_correctly() {
    let mut menu = SettingsMenu::new();
    
    // Initially no dirty fields
    assert!(!menu.has_unsaved_changes());
    
    // Make a change by binding a key
    menu.start_listening(0);
    menu.apply_key_code_while_listening(87, 0); // 'W' key
    
    // Should now have dirty fields
    assert!(menu.has_unsaved_changes());
    
    // Apply changes (save)
    menu.apply_staged_changes();
    
    // Should no longer have dirty fields
    assert!(!menu.has_unsaved_changes());
}

/// Test 5: Reverting staged changes
#[test]
fn revert_changes_works() {
    let mut menu = SettingsMenu::new();
    
    // Get original binding for key_w
    let original_binding = menu.get_staged_binding(0);
    
    // Make a change - use 'Q' which isn't bound by default
    menu.start_listening(0);
    menu.apply_key_code_while_listening('Q' as u32, 0); // 'Q' key
    
    // Staged binding should be different
    let new_binding = menu.get_staged_binding(0);
    assert_ne!(original_binding, new_binding);
    assert!(menu.has_unsaved_changes());
    
    // Revert changes
    menu.revert_staged_changes();
    
    // Should be back to original
    assert_eq!(menu.get_staged_binding(0), original_binding);
    assert!(!menu.has_unsaved_changes());
}

/// Test 6: Confirming conflict replaces binding
#[test]
fn confirming_conflict_replaces_binding() {
    let mut menu = SettingsMenu::new();
    
    // Bind key_w to 'W'
    menu.start_listening(0);
    menu.apply_key_code_while_listening(87, 0);
    let key_w_binding = menu.get_staged_binding(0);
    
    // Try to bind key_a to the same key (will conflict)
    menu.start_listening(1);
    menu.apply_key_code_while_listening(87, 0);
    
    // Should show conflict modal
    assert!(menu.show_conflict_modal);
    
    // Confirm the conflict (replace existing binding)
    menu.confirm_pending_binding();
    
    // Modal should be closed
    assert!(!menu.show_conflict_modal);
    
    // key_a should now have the binding
    assert_eq!(menu.get_staged_binding(1), key_w_binding);
    
    // key_w should have an empty binding (conflict was replaced)
    assert_eq!(menu.get_staged_binding(0), Binding::new(0, 0));
}

/// Test 7: Canceling conflict preserves original binding
#[test]
fn canceling_conflict_preserves_original() {
    let mut menu = SettingsMenu::new();
    
    // Bind key_w to 'W'
    menu.start_listening(0);
    menu.apply_key_code_while_listening(87, 0);
    let key_w_binding = menu.get_staged_binding(0);
    
    // Bind key_a to 'A'
    menu.start_listening(1);
    menu.apply_key_code_while_listening(65, 0);
    let key_a_binding = menu.get_staged_binding(1);
    
    // Try to bind key_a to 'W' again (will conflict)
    menu.start_listening(1);
    menu.apply_key_code_while_listening(87, 0);
    
    // Should show conflict modal
    assert!(menu.show_conflict_modal);
    
    // Cancel the conflict
    menu.cancel_pending_binding();
    
    // Modal should be closed
    assert!(!menu.show_conflict_modal);
    
    // Original bindings should be preserved
    assert_eq!(menu.get_staged_binding(0), key_w_binding);
    assert_eq!(menu.get_staged_binding(1), key_a_binding);
}

/// Test 8: Multiple bindings don't interfere with each other
#[test]
fn multiple_unique_bindings_work() {
    let mut menu = SettingsMenu::new();
    
    // Bind all six keys to unique codes
    let bindings = [87, 65, 83, 68, 38, 40]; // W, A, S, D, Up, Down
    
    for (id, &code) in bindings.iter().enumerate() {
        menu.start_listening(id);
        menu.apply_key_code_while_listening(code, 0);
        assert!(!menu.show_conflict_modal, "unique bindings should not conflict");
    }
    
    // Verify all bindings are set correctly
    for (id, &code) in bindings.iter().enumerate() {
        let binding = menu.get_staged_binding(id);
        assert_eq!(binding.code, code, "binding {} should have correct code", id);
    }
}

/// Test 9: Render doesn't crash with default context
#[test]
fn render_doesnt_crash() {
    let mut menu = SettingsMenu::new();
    let ctx = egui::Context::default();
    
    // Render in both tabs
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        menu.render(ctx);
    });
    
    // Switch tab and render again
    use moho_ui::screens::SettingsTab;
    menu.set_active_tab(SettingsTab::Audio);
    
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        menu.render(ctx);
    });
    
    // Should not panic
}

/// Test 10: Modifier keys are handled correctly
#[test]
fn modifier_keys_handled() {
    let mut menu = SettingsMenu::new();
    
    // Bind with Ctrl modifier (mods_bits = 1)
    menu.start_listening(0);
    menu.apply_key_code_while_listening(87, 1); // Ctrl+W
    
    let binding = menu.get_staged_binding(0);
    assert_eq!(binding.code, 87);
    assert_eq!(binding.mods, 1, "should have Ctrl modifier");
    
    // Bind with multiple modifiers (Ctrl+Shift = 3)
    menu.start_listening(1);
    menu.apply_key_code_while_listening(65, 3); // Ctrl+Shift+A
    
    let binding = menu.get_staged_binding(1);
    assert_eq!(binding.code, 65);
    assert_eq!(binding.mods, 3, "should have Ctrl+Shift modifiers");
}
