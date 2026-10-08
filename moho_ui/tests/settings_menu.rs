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

use moho_input::bindings::Binding;
use moho_input::key::Key;
use moho_ui::UiComponent;
use moho_ui::prefs::Prefs;
use moho_ui::screens::SettingsMenu;

/// Test 2: Escaping while listening cancels binding
#[test]
fn escape_cancels_binding_listen() {
    let mut menu = SettingsMenu::with_prefs(Prefs::default());

    menu.start_listening(0);
    assert!(menu.is_listening());

    let consumed = menu.apply_key_while_listening(Key::Escape);
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
    menu.apply_key_while_listening(Key::W);

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
    menu.apply_key_while_listening(Key::W);
    menu.apply_staged_changes(); // Save it

    // Get the saved binding
    let original_binding = menu.get_staged_binding(0).to_vec();
    assert_eq!(
        original_binding,
        [Binding::Key(Key::W)],
        "Expected original binding to be 'W'"
    );

    // Now make a change - use 'Q' which is different
    menu.start_listening(0);
    menu.apply_key_while_listening(Key::Q);

    // If there was a conflict, confirm it
    if menu.conflict_modal().is_visible() {
        menu.confirm_pending_binding();
    }

    // Staged binding should be different
    let new_binding = menu.get_staged_binding(0).to_vec();
    assert_ne!(original_binding, new_binding);
    assert_eq!(new_binding, [Binding::Key(Key::Q)]);
    assert!(menu.has_unsaved_changes());

    // Revert changes
    menu.revert_staged_changes();

    // Should be back to original
    assert_eq!(menu.get_staged_binding(0), original_binding.as_slice());
    assert!(!menu.has_unsaved_changes());
}

/// Confirming through the adapter's take-then-confirm order moves the key.
#[test]
fn confirming_conflict_through_modal_flow_replaces_binding() {
    use moho_ui::screens::Screen;

    let mut menu = SettingsMenu::with_prefs(Prefs::default());
    menu.start_listening(1);
    menu.apply_key_while_listening(Key::W);

    let modal = Screen::take_pending_modal(&mut menu);
    assert!(modal.is_some(), "conflict should produce a modal");
    assert!(
        Screen::take_pending_modal(&mut menu).is_none(),
        "taking the dialog must hide it"
    );
    assert!(!menu.conflict_modal().is_visible());
    Screen::on_modal_confirm(&mut menu);

    assert_eq!(menu.get_staged_binding(1), [Binding::Key(Key::W)]);
    assert_eq!(menu.get_staged_binding(0), []);
}

/// Cancelling through the adapter's take-then-cancel order changes nothing.
#[test]
fn cancelling_conflict_through_modal_flow_keeps_bindings() {
    use moho_ui::screens::Screen;

    let mut menu = SettingsMenu::with_prefs(Prefs::default());
    let before: Vec<Vec<Binding>> = (0..7)
        .map(|i| menu.get_staged_binding(i).to_vec())
        .collect();
    menu.start_listening(1);
    menu.apply_key_while_listening(Key::W);

    let modal = Screen::take_pending_modal(&mut menu);
    assert!(modal.is_some(), "conflict should produce a modal");
    Screen::on_modal_cancel(&mut menu);

    let after: Vec<Vec<Binding>> = (0..7)
        .map(|i| menu.get_staged_binding(i).to_vec())
        .collect();
    assert_eq!(before, after);
}

/// Test 7: Canceling conflict preserves original binding
#[test]
fn canceling_conflict_preserves_original() {
    let mut menu = SettingsMenu::with_prefs(Prefs::default());

    // Bind key_w to 'W'
    menu.start_listening(0);
    menu.apply_key_while_listening(Key::W);
    let key_w_binding = menu.get_staged_binding(0).to_vec();

    // Bind key_a to 'A'
    menu.start_listening(1);
    menu.apply_key_while_listening(Key::A);
    let key_a_binding = menu.get_staged_binding(1).to_vec();

    // Try to bind key_a to 'W' again (will conflict)
    menu.start_listening(1);
    menu.apply_key_while_listening(Key::W);

    // Should show conflict modal
    assert!(menu.conflict_modal().is_visible());

    // Cancel the conflict
    menu.cancel_pending_binding();

    // Modal should be closed
    assert!(!menu.conflict_modal().is_visible());

    // Original bindings should be preserved
    assert_eq!(menu.get_staged_binding(0), key_w_binding.as_slice());
    assert_eq!(menu.get_staged_binding(1), key_a_binding.as_slice());
}

/// Test 8: Multiple bindings don't interfere with each other
#[test]
fn multiple_unique_bindings_work() {
    let mut menu = SettingsMenu::with_prefs(Prefs::default());

    let bindings = [Key::W, Key::A, Key::S, Key::D, Key::ArrowUp, Key::ArrowDown];

    for (id, &key) in bindings.iter().enumerate() {
        menu.start_listening(id);
        menu.apply_key_while_listening(key);
        assert!(
            !menu.conflict_modal().is_visible(),
            "unique bindings should not conflict"
        );
    }

    // Verify all bindings are set correctly
    for (id, &key) in bindings.iter().enumerate() {
        let binding = menu.get_staged_binding(id);
        assert_eq!(
            binding,
            [Binding::Key(key)],
            "binding {id} should have correct key"
        );
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

/// A lone modifier key can be captured as a binding
#[test]
fn modifier_key_can_be_bound() {
    let mut menu = SettingsMenu::with_prefs(Prefs::default());

    // Move Down holds Ctrl by default; free it first.
    menu.start_listening(5);
    menu.apply_key_while_listening(Key::Z);

    menu.start_listening(0);
    menu.apply_key_while_listening(Key::Ctrl);

    assert!(!menu.conflict_modal().is_visible());
    assert_eq!(menu.get_staged_binding(0), [Binding::Key(Key::Ctrl)]);
}

/// Confirming a conflict removes only the contested key from the other row.
#[test]
fn confirming_conflict_removes_only_the_contested_key_from_a_multi_key_row() {
    use moho_ui::screens::Screen;

    let (prefs, issues) = Prefs::parse("[bindings]\nmove_forward = W, ArrowUp\n");
    assert_eq!(issues, vec![]);
    let mut menu = SettingsMenu::with_prefs(prefs);
    menu.start_listening(1);
    menu.apply_key_while_listening(Key::W);

    assert!(Screen::take_pending_modal(&mut menu).is_some());
    Screen::on_modal_confirm(&mut menu);

    assert_eq!(menu.get_staged_binding(0), [Binding::Key(Key::ArrowUp)]);
    assert_eq!(menu.get_staged_binding(1), [Binding::Key(Key::W)]);
}
