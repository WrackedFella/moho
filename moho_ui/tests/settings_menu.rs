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

use moho_input::bindings::Binding;
use moho_input::key::Key;
use moho_input::pad::{PadButton, PadInput, Stick, StickDir};
use moho_ui::UiComponent;
use moho_ui::actions::StrategyAction;
use moho_ui::prefs::Prefs;
use moho_ui::screens::SettingsMenu;

const ROW_ACTIONS: [StrategyAction; 7] = [
    StrategyAction::MoveForward,
    StrategyAction::MoveLeft,
    StrategyAction::MoveBack,
    StrategyAction::MoveRight,
    StrategyAction::Ascend,
    StrategyAction::Descend,
    StrategyAction::Sprint,
];

fn pad_stick(dir: StickDir) -> Binding {
    Binding::Pad(PadInput::Stick(Stick::LeftStick, dir))
}

/// Test 2: Escaping while listening cancels binding
#[test]
fn escape_cancels_binding_listen() {
    let mut menu = SettingsMenu::with_prefs(Prefs::default());
    let before = menu.get_staged_binding(0).to_vec();

    menu.start_listening(0);
    assert!(menu.is_listening());

    let consumed = menu.apply_key_while_listening(Key::Escape);
    assert!(consumed, "escape should be consumed");
    assert!(!menu.is_listening(), "should stop listening after escape");
    assert!(
        !menu.conflict_modal().is_visible(),
        "should not show conflict modal"
    );
    assert_eq!(menu.get_staged_binding(0), before.as_slice());
    assert!(!menu.has_unsaved_changes());
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
        [Binding::Key(Key::W), pad_stick(StickDir::Up)],
        "Expected original binding to be 'W' plus its pad binding"
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
    assert_eq!(new_binding, [Binding::Key(Key::Q), pad_stick(StickDir::Up)]);
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

    assert_eq!(
        menu.get_staged_binding(1),
        [Binding::Key(Key::W), pad_stick(StickDir::Left)]
    );
    assert_eq!(menu.get_staged_binding(0), [pad_stick(StickDir::Up)]);
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
        let pads: Vec<Binding> = moho_input::Action::default_bindings(ROW_ACTIONS[id])
            .iter()
            .copied()
            .filter(|b| matches!(b, Binding::Pad(_)))
            .collect();
        let expected: Vec<Binding> = std::iter::once(Binding::Key(key)).chain(pads).collect();
        assert_eq!(binding, expected, "binding {id} should have correct key");
    }
}

/// Test 9: Render doesn't crash with default context
#[test]
fn render_doesnt_crash() {
    use moho_ui::screens::SettingsTab;
    let mut menu = SettingsMenu::with_prefs(Prefs::default());
    let ctx = egui::Context::default();

    // Render in both tabs
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
        menu.render(ui);
    });

    // Switch tab and render again
    menu.set_active_tab(SettingsTab::Audio);

    let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
        menu.render(ui);
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
    assert_eq!(
        menu.get_staged_binding(0),
        [Binding::Key(Key::Ctrl), pad_stick(StickDir::Up)]
    );
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
    assert_eq!(
        menu.get_staged_binding(1),
        [Binding::Key(Key::W), pad_stick(StickDir::Left)]
    );
}

fn key_event(key: egui::Key, pressed: bool) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

fn render_frame(menu: &mut SettingsMenu, events: Vec<egui::Event>) -> egui::FullOutput {
    let ctx = egui::Context::default();
    let input = egui::RawInput {
        events,
        ..Default::default()
    };
    ctx.run_ui(input, |ui| {
        menu.render(ui);
    })
}

fn collect_text(shape: &egui::epaint::Shape, out: &mut String) {
    match shape {
        egui::epaint::Shape::Text(text) => {
            out.push_str(text.galley.text());
            out.push('\n');
        }
        egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect_text(s, out)),
        _ => {}
    }
}

fn rendered_text(output: &egui::FullOutput) -> String {
    let mut out = String::new();
    for clipped in &output.shapes {
        collect_text(&clipped.shape, &mut out);
    }
    out
}

/// A key pressed during a frame binds the row being listened for.
#[test]
fn egui_key_press_while_listening_binds_the_row() {
    let mut menu = SettingsMenu::with_prefs(Prefs::default());
    menu.start_listening(1);

    render_frame(&mut menu, vec![key_event(egui::Key::Q, true)]);

    assert_eq!(
        menu.get_staged_binding(1),
        [Binding::Key(Key::Q), pad_stick(StickDir::Left)]
    );
    assert!(!menu.is_listening());
}

/// A key release is not a capture.
#[test]
fn egui_key_release_while_listening_is_ignored() {
    let mut menu = SettingsMenu::with_prefs(Prefs::default());
    menu.start_listening(1);

    render_frame(&mut menu, vec![key_event(egui::Key::Q, false)]);

    assert_eq!(
        menu.get_staged_binding(1),
        [Binding::Key(Key::A), pad_stick(StickDir::Left)]
    );
    assert!(menu.is_listening());
}

/// Escape during a frame cancels listening and binds nothing.
#[test]
fn egui_escape_while_listening_cancels_without_binding() {
    let mut menu = SettingsMenu::with_prefs(Prefs::default());
    menu.start_listening(1);

    render_frame(&mut menu, vec![key_event(egui::Key::Escape, true)]);

    assert_eq!(
        menu.get_staged_binding(1),
        [Binding::Key(Key::A), pad_stick(StickDir::Left)]
    );
    assert!(!menu.is_listening());
    assert!(!menu.has_unsaved_changes());
}

/// An egui key the engine does not name keeps listening.
#[test]
fn egui_unnamed_key_while_listening_keeps_listening() {
    let mut menu = SettingsMenu::with_prefs(Prefs::default());
    menu.start_listening(1);

    render_frame(&mut menu, vec![key_event(egui::Key::F5, true)]);

    assert_eq!(
        menu.get_staged_binding(1),
        [Binding::Key(Key::A), pad_stick(StickDir::Left)]
    );
    assert!(menu.is_listening());
}

/// Key events are ignored when no row is listening.
#[test]
fn egui_key_press_when_not_listening_changes_nothing() {
    let mut menu = SettingsMenu::with_prefs(Prefs::default());

    render_frame(&mut menu, vec![key_event(egui::Key::Q, true)]);

    assert_eq!(
        menu.get_staged_binding(1),
        [Binding::Key(Key::A), pad_stick(StickDir::Left)]
    );
    assert!(!menu.has_unsaved_changes());
}

/// The Controls tab draws a labelled row per binding plus the input options.
#[test]
fn controls_tab_draws_binding_rows_and_input_options() {
    let mut menu = SettingsMenu::with_prefs(Prefs::default());

    let text = rendered_text(&render_frame(&mut menu, vec![]));

    for expected in [
        "Controls",
        "Move Forward:",
        "Move Left:",
        "Sprint:",
        "Mouse Sensitivity:",
        "Input Filtering:",
    ] {
        assert!(
            text.contains(expected),
            "{expected:?} missing from:\n{text}"
        );
    }
}

/// Non-keyboard window events are not consumed by the keybind capture, listening or not.
#[test]
fn non_keyboard_window_events_are_not_consumed() {
    use moho_ui::screens::Screen;
    use winit::event::WindowEvent;

    let mut menu = SettingsMenu::with_prefs(Prefs::default());
    assert!(!Screen::handle_raw_input(
        &mut menu,
        &WindowEvent::Focused(true)
    ));
    menu.start_listening(1);

    let consumed = Screen::handle_raw_input(&mut menu, &WindowEvent::Focused(true));

    assert!(!consumed);
    assert!(menu.is_listening());
}

/// Exactly the listening row swaps its key label for the prompt.
#[test]
fn only_the_listening_row_shows_the_prompt() {
    let mut idle = SettingsMenu::with_prefs(Prefs::default());
    let mut listening = SettingsMenu::with_prefs(Prefs::default());
    listening.start_listening(3);

    let idle_text = rendered_text(&render_frame(&mut idle, vec![]));
    let listening_text = rendered_text(&render_frame(&mut listening, vec![]));

    assert_eq!(idle_text.matches("Press any key...").count(), 0);
    assert_eq!(listening_text.matches("Press any key...").count(), 1);
    let lines: Vec<_> = listening_text.lines().collect();
    assert!(lines.contains(&"W") && lines.contains(&"S"));
    assert!(lines.contains(&"A"), "Move Left's key stays visible");
    assert!(!lines.contains(&"D"), "Move Right is the listening row");
}

#[test]
fn raw_input_is_captured_only_while_listening() {
    use moho_ui::screens::Screen;

    let mut menu = SettingsMenu::with_prefs(Prefs::default());
    assert!(!Screen::captures_raw_input(&menu));

    menu.start_listening(0);

    assert!(Screen::captures_raw_input(&menu));
}

/// Cancelling discards the queued binding, so a later confirm applies nothing.
#[test]
fn cancel_then_confirm_applies_nothing() {
    use moho_ui::screens::Screen;

    let mut menu = SettingsMenu::with_prefs(Prefs::default());
    menu.start_listening(1);
    menu.apply_key_while_listening(Key::W);
    assert!(Screen::take_pending_modal(&mut menu).is_some());

    Screen::on_modal_cancel(&mut menu);
    Screen::on_modal_confirm(&mut menu);

    assert_eq!(
        menu.get_staged_binding(0),
        [Binding::Key(Key::W), pad_stick(StickDir::Up)]
    );
    assert_eq!(
        menu.get_staged_binding(1),
        [Binding::Key(Key::A), pad_stick(StickDir::Left)]
    );
}

#[test]
fn confirm_pending_binding_moves_the_key() {
    let mut menu = SettingsMenu::with_prefs(Prefs::default());
    menu.start_listening(1);
    menu.apply_key_while_listening(Key::W);
    assert!(menu.conflict_modal().is_visible());

    menu.confirm_pending_binding();

    assert_eq!(
        menu.get_staged_binding(1),
        [Binding::Key(Key::W), pad_stick(StickDir::Left)]
    );
    assert_eq!(menu.get_staged_binding(0), [pad_stick(StickDir::Up)]);
}

#[test]
fn rebinding_ascend_to_f_keeps_its_pad_binding() {
    let mut menu = SettingsMenu::with_prefs(Prefs::default());
    menu.start_listening(4);

    menu.apply_key_while_listening(Key::F);

    assert_eq!(
        menu.get_staged_binding(4),
        [
            Binding::Key(Key::F),
            Binding::Pad(PadInput::Button(PadButton::RightTrigger))
        ]
    );
}

#[test]
fn conflict_confirm_keeps_pad_bindings_on_both_actions() {
    let mut menu = SettingsMenu::with_prefs(Prefs::default());
    menu.start_listening(6); // Sprint
    menu.apply_key_while_listening(Key::Ctrl); // held by Descend

    menu.confirm_pending_binding();

    assert_eq!(
        menu.get_staged_binding(6),
        [
            Binding::Key(Key::Ctrl),
            Binding::Pad(PadInput::Button(PadButton::LeftThumb))
        ]
    );
    assert_eq!(
        menu.get_staged_binding(5),
        [Binding::Pad(PadInput::Button(PadButton::LeftTrigger))]
    );
}

#[test]
fn rows_never_show_pad_labels() {
    let mut menu = SettingsMenu::with_prefs(Prefs::default());

    let text = rendered_text(&render_frame(&mut menu, vec![]));

    assert!(!text.contains("Pad"), "pad label shown in:\n{text}");
    assert!(text.lines().any(|l| l == "W"), "W row missing in:\n{text}");
}

#[test]
fn pad_only_action_row_shows_unbound() {
    let (prefs, issues) = Prefs::parse("[bindings]\nmove_forward = Pad LeftStick Up\n");
    assert_eq!(issues, vec![]);
    let mut menu = SettingsMenu::with_prefs(prefs);

    let text = rendered_text(&render_frame(&mut menu, vec![]));

    assert_eq!(text.lines().filter(|l| *l == "Unbound").count(), 1);
    assert!(!text.contains("Pad"), "pad label shown in:\n{text}");
}
