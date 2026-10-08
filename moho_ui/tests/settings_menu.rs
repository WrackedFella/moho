//! Integration tests for SettingsMenu
//!
//! # Test Isolation
//!
//! These tests use `SettingsMenu::with_prefs(Prefs::default())` instead of
//! `SettingsMenu::new()` to ensure test isolation. This approach:
//!
//! 1. **Avoids shared state**: Each test starts with a clean default Prefs instance
//!    rather than loading from the potentially-modified config/prefs.ini file.
//!
//! 2. **Prevents test interference**: Tests that call `apply_staged_changes()`
//!    (which saves to disk) won't affect other tests because each test initializes
//!    with `Prefs::default()` regardless of disk state.
//!
//! 3. **Maintains realistic behavior**: Tests that need to verify save/revert
//!    functionality can still call `apply_staged_changes()` and `revert_staged_changes()`
//!    because the underlying Prefs instance behaves identically whether loaded from
//!    disk or created as default.
//!
//! This pattern ensures tests are deterministic, parallelizable, and won't fail
//! due to leftover state from previous test runs or other tests in the suite.

// See moho_ui/src/lib.rs's crate-level `#![allow(deprecated)]` for why
// `Context::run` (egui 0.34 deprecation) is still used here.
#![allow(deprecated)]

use moho_ui::UiComponent;
use moho_ui::prefs::{Binding, Prefs};
use moho_ui::screens::SettingsMenu;

/// Test 2: Escaping while listening cancels binding
#[test]
fn escape_cancels_binding_listen() {
    let mut menu = SettingsMenu::with_prefs(Prefs::default());

    menu.start_listening(0);
    assert!(menu.is_listening());

    // Press escape (code 0x200)
    let consumed = menu.apply_key_code_while_listening(0x200, 0);
    assert!(consumed, "escape should be consumed");
    assert!(!menu.is_listening(), "should stop listening after escape");
    assert!(
        !menu.conflict_modal().is_visible(),
        "should not show conflict modal"
    );
}

/// Test 3: Tab switching functionality
#[test]
fn tab_switching_works() {
    use moho_ui::screens::SettingsTab;

    let mut menu = SettingsMenu::with_prefs(Prefs::default());

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
    let mut menu = SettingsMenu::with_prefs(Prefs::default());

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
    let mut menu = SettingsMenu::with_prefs(Prefs::default());

    // First, explicitly set a known binding so we're not dependent on defaults
    menu.start_listening(0);
    menu.apply_key_code_while_listening(87, 0); // 'W'
    menu.apply_staged_changes(); // Save it

    // Get the saved binding
    let original_binding = menu.get_staged_binding(0);
    assert_eq!(
        original_binding.code, 87,
        "Expected original binding to be 'W'"
    );

    // Now make a change - use 'Q' which is different
    menu.start_listening(0);
    menu.apply_key_code_while_listening('Q' as u32, 0); // 'Q' key

    // If there was a conflict, confirm it
    if menu.conflict_modal().is_visible() {
        menu.confirm_pending_binding();
    }

    // Staged binding should be different
    let new_binding = menu.get_staged_binding(0);
    assert_ne!(original_binding, new_binding);
    assert_eq!(new_binding.code, 'Q' as u32);
    assert!(menu.has_unsaved_changes());

    // Revert changes
    menu.revert_staged_changes();

    // Should be back to original
    assert_eq!(menu.get_staged_binding(0), original_binding);
    assert!(!menu.has_unsaved_changes());
}

/// Confirming through the adapter's take-then-confirm order moves the key.
#[test]
fn confirming_conflict_through_modal_flow_replaces_binding() {
    use moho_ui::screens::Screen;

    let mut menu = SettingsMenu::with_prefs(Prefs::default());
    menu.start_listening(1);
    menu.apply_key_code_while_listening('W' as u32, 0);

    let modal = Screen::take_pending_modal(&mut menu);
    assert!(modal.is_some(), "conflict should produce a modal");
    assert!(
        Screen::take_pending_modal(&mut menu).is_none(),
        "taking the dialog must hide it"
    );
    assert!(!menu.conflict_modal().is_visible());
    Screen::on_modal_confirm(&mut menu);

    assert_eq!(menu.get_staged_binding(1), Binding::new('W' as u32, 0));
    assert_eq!(menu.get_staged_binding(0), Binding::new(0, 0));
}

/// Cancelling through the adapter's take-then-cancel order changes nothing.
#[test]
fn cancelling_conflict_through_modal_flow_keeps_bindings() {
    use moho_ui::screens::Screen;

    let mut menu = SettingsMenu::with_prefs(Prefs::default());
    let before: Vec<Binding> = (0..7).map(|i| menu.get_staged_binding(i)).collect();
    menu.start_listening(1);
    menu.apply_key_code_while_listening('W' as u32, 0);

    let modal = Screen::take_pending_modal(&mut menu);
    assert!(modal.is_some(), "conflict should produce a modal");
    Screen::on_modal_cancel(&mut menu);

    let after: Vec<Binding> = (0..7).map(|i| menu.get_staged_binding(i)).collect();
    assert_eq!(before, after);
}

/// Test 7: Canceling conflict preserves original binding
#[test]
fn canceling_conflict_preserves_original() {
    let mut menu = SettingsMenu::with_prefs(Prefs::default());

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
    assert!(menu.conflict_modal().is_visible());

    // Cancel the conflict
    menu.cancel_pending_binding();

    // Modal should be closed
    assert!(!menu.conflict_modal().is_visible());

    // Original bindings should be preserved
    assert_eq!(menu.get_staged_binding(0), key_w_binding);
    assert_eq!(menu.get_staged_binding(1), key_a_binding);
}

/// Test 8: Multiple bindings don't interfere with each other
#[test]
fn multiple_unique_bindings_work() {
    let mut menu = SettingsMenu::with_prefs(Prefs::default());

    // Bind all six keys to unique codes
    let bindings = [87, 65, 83, 68, 38, 40]; // W, A, S, D, Up, Down

    for (id, &code) in bindings.iter().enumerate() {
        menu.start_listening(id);
        menu.apply_key_code_while_listening(code, 0);
        assert!(
            !menu.conflict_modal().is_visible(),
            "unique bindings should not conflict"
        );
    }

    // Verify all bindings are set correctly
    for (id, &code) in bindings.iter().enumerate() {
        let binding = menu.get_staged_binding(id);
        assert_eq!(binding.code, code, "binding {id} should have correct code");
    }
}

/// Test 9: Render doesn't crash with default context
#[test]
fn render_doesnt_crash() {
    use moho_ui::screens::SettingsTab;
    let mut menu = SettingsMenu::with_prefs(Prefs::default());
    let ctx = egui::Context::default();

    // Render in both tabs
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        menu.render(ctx);
    });

    // Switch tab and render again
    menu.set_active_tab(SettingsTab::Audio);

    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        menu.render(ctx);
    });

    // Should not panic
}

/// Test 10: Modifier keys are handled correctly
#[test]
fn modifier_keys_handled() {
    let mut menu = SettingsMenu::with_prefs(Prefs::default());

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
