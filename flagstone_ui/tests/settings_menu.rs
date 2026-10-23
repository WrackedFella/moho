//! Integration tests for SettingsMenu
//!
//! # Test Isolation
//!
//! Menus are built from `Prefs::default()` so no test depends on the contents of
//! `config/prefs.ini`.
//!
//! Saving writes to the menu's save path. `SettingsMenu::with_prefs_at` points it at a
//! temporary directory, and the one test that saves uses it. `SettingsMenu::with_prefs`
//! saves to `Prefs::config_path()`, the real prefs file, so no test here calls
//! `apply_staged_changes` on a menu built that way.

use flagstone_ui::UiComponent;
use flagstone_ui::actions::StrategyAction;
use flagstone_ui::prefs::Prefs;
use flagstone_ui::screens::SettingsMenu;
use moho_input::Modifiers;
use moho_input::bindings::Binding;
use moho_input::key::Key;
use moho_input::pad::{PadButton, PadInput, Stick, StickDir};

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
    Binding::pad(PadInput::Stick(Stick::LeftStick, dir))
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

/// Saving clears the dirty state and persists the staged binding with its pad binding.
#[test]
fn staged_changes_tracked_and_cleared_on_save() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("prefs.ini");
    let mut menu = SettingsMenu::with_prefs_at(Prefs::default(), path.clone());
    assert!(!menu.has_unsaved_changes());

    menu.start_listening(0);
    menu.apply_key_while_listening(Key::Q);

    assert!(menu.has_unsaved_changes());

    menu.apply_staged_changes().unwrap();

    assert!(!menu.has_unsaved_changes());
    menu.revert_staged_changes();
    assert_eq!(
        menu.get_staged_binding(0),
        [Binding::key(Key::Q), pad_stick(StickDir::Up)],
        "save updates the saved snapshot, so revert keeps it"
    );
    let (saved, issues) = Prefs::load_from(&path);
    assert_eq!(issues, vec![]);
    let reloaded = SettingsMenu::with_prefs(saved);
    assert_eq!(
        reloaded.get_staged_binding(0),
        [Binding::key(Key::Q), pad_stick(StickDir::Up)]
    );
}

/// A failed write is reported to the caller, not swallowed.
#[test]
fn save_error_propagates() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("a_file");
    std::fs::write(&blocker, "not a directory").unwrap();
    let mut menu = SettingsMenu::with_prefs_at(Prefs::default(), blocker.join("prefs.ini"));
    menu.start_listening(0);
    menu.apply_key_while_listening(Key::Q);

    let result = menu.apply_staged_changes();

    assert!(result.is_err());
}

/// Reverting discards a staged binding and returns the row to its saved keys.
#[test]
fn revert_changes_works() {
    let mut menu = SettingsMenu::with_prefs(Prefs::default());
    assert_eq!(
        menu.get_staged_binding(0),
        [Binding::key(Key::W), pad_stick(StickDir::Up)]
    );

    menu.start_listening(0);
    menu.apply_key_while_listening(Key::Q);

    assert!(
        !menu.conflict_modal().is_visible(),
        "Q is unbound by default"
    );
    assert_eq!(
        menu.get_staged_binding(0),
        [Binding::key(Key::Q), pad_stick(StickDir::Up)]
    );
    assert!(menu.has_unsaved_changes());

    menu.revert_staged_changes();

    assert_eq!(
        menu.get_staged_binding(0),
        [Binding::key(Key::W), pad_stick(StickDir::Up)]
    );
    assert!(!menu.has_unsaved_changes());
}

/// Confirming through the adapter's take-then-confirm order moves the key.
#[test]
fn confirming_conflict_through_modal_flow_replaces_binding() {
    use flagstone_ui::screens::Screen;

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
        [Binding::key(Key::W), pad_stick(StickDir::Left)]
    );
    assert_eq!(menu.get_staged_binding(0), [pad_stick(StickDir::Up)]);
}

/// Cancelling through the adapter's take-then-cancel order changes nothing.
#[test]
fn cancelling_conflict_through_modal_flow_keeps_bindings() {
    use flagstone_ui::screens::Screen;

    let mut menu = SettingsMenu::with_prefs(Prefs::default());
    let before: Vec<Vec<Binding>> = (0..7)
        .map(|i| menu.get_staged_binding(i).to_vec())
        .collect();
    menu.start_listening(1);
    menu.apply_key_while_listening(Key::W);

    let modal = Screen::take_pending_modal(&mut menu);
    assert!(modal.is_some(), "conflict should produce a modal");
    Screen::on_modal_cancel(&mut menu);

    assert!(!menu.conflict_modal().is_visible());
    let after: Vec<Vec<Binding>> = (0..7)
        .map(|i| menu.get_staged_binding(i).to_vec())
        .collect();
    assert_eq!(before, after);
}

/// Test 8: Multiple bindings don't interfere with each other
#[test]
fn multiple_unique_bindings_work() {
    let mut menu = SettingsMenu::with_prefs(Prefs::default());

    // None of these is any row's default key, and none repeats, so no step conflicts.
    let bindings = [Key::Q, Key::E, Key::R, Key::T, Key::Y, Key::U, Key::I];

    for (id, &key) in bindings.iter().enumerate() {
        menu.start_listening(id);
        menu.apply_key_while_listening(key);
        assert!(
            !menu.conflict_modal().is_visible(),
            "unique bindings should not conflict"
        );
    }

    for (id, &key) in bindings.iter().enumerate() {
        let binding = menu.get_staged_binding(id);
        let pads: Vec<Binding> = moho_input::Action::default_bindings(ROW_ACTIONS[id])
            .iter()
            .copied()
            .filter(|b| b.is_pad())
            .collect();
        let expected: Vec<Binding> = std::iter::once(Binding::key(key)).chain(pads).collect();
        assert_eq!(binding, expected, "binding {id} should have correct key");
    }
}

/// Test 9: Render doesn't crash with default context
#[test]
fn render_doesnt_crash() {
    use flagstone_ui::screens::SettingsTab;
    let mut menu = SettingsMenu::with_prefs(Prefs::default());
    let ctx = egui::Context::default();

    for tab in [
        SettingsTab::Controls,
        SettingsTab::Audio,
        SettingsTab::Video,
    ] {
        menu.set_active_tab(tab);

        let _ = run_ui(&ctx, egui::RawInput::default(), |ui| {
            menu.render(ui);
        });
    }
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
        [Binding::key(Key::Ctrl), pad_stick(StickDir::Up)]
    );
}

/// Confirming a conflict removes only the contested key from the other row.
#[test]
fn confirming_conflict_removes_only_the_contested_key_from_a_multi_key_row() {
    use flagstone_ui::screens::Screen;

    let (prefs, issues) = Prefs::parse("[bindings]\nmove_forward = W, ArrowUp\n");
    assert_eq!(issues, vec![]);
    let mut menu = SettingsMenu::with_prefs(prefs);
    menu.start_listening(1);
    menu.apply_key_while_listening(Key::W);

    assert!(Screen::take_pending_modal(&mut menu).is_some());
    Screen::on_modal_confirm(&mut menu);

    assert_eq!(menu.get_staged_binding(0), [Binding::key(Key::ArrowUp)]);
    assert_eq!(
        menu.get_staged_binding(1),
        [Binding::key(Key::W), pad_stick(StickDir::Left)]
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
    run_ui(&ctx, input, |ui| {
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
        [Binding::key(Key::Q), pad_stick(StickDir::Left)]
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
        [Binding::key(Key::A), pad_stick(StickDir::Left)]
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
        [Binding::key(Key::A), pad_stick(StickDir::Left)]
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
        [Binding::key(Key::A), pad_stick(StickDir::Left)]
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
        [Binding::key(Key::A), pad_stick(StickDir::Left)]
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
        "Mouse smoothing:",
    ] {
        assert!(
            text.contains(expected),
            "{expected:?} missing from:\n{text}"
        );
    }
}

/// The Video tab draws its window mode and resolution rows.
#[test]
fn video_tab_draws_window_mode_and_resolution_rows() {
    use flagstone_ui::screens::SettingsTab;
    let mut menu = SettingsMenu::with_prefs(Prefs::default());
    menu.set_active_tab(SettingsTab::Video);

    let text = rendered_text(&render_frame(&mut menu, vec![]));

    for expected in ["Video", "Window Mode:", "Resolution:"] {
        assert!(
            text.contains(expected),
            "{expected:?} missing from:\n{text}"
        );
    }
}

/// Rendering the Video tab with no input leaves a clean form clean.
#[test]
fn idle_video_tab_frames_leave_the_form_clean() {
    use flagstone_ui::screens::SettingsTab;
    let mut menu = SettingsMenu::with_prefs(Prefs::default());
    menu.set_active_tab(SettingsTab::Video);

    render_frame(&mut menu, vec![]);
    render_frame(&mut menu, vec![]);

    assert!(!menu.has_unsaved_changes());
}

/// Non-keyboard window events are not consumed by the keybind capture, listening or not.
#[test]
fn non_keyboard_window_events_are_not_consumed() {
    use flagstone_ui::screens::Screen;
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
    use flagstone_ui::screens::Screen;

    let mut menu = SettingsMenu::with_prefs(Prefs::default());
    assert!(!Screen::captures_raw_input(&menu));

    menu.start_listening(0);

    assert!(Screen::captures_raw_input(&menu));
}

/// Cancelling discards the queued binding, so a later confirm applies nothing.
#[test]
fn cancel_then_confirm_applies_nothing() {
    use flagstone_ui::screens::Screen;

    let mut menu = SettingsMenu::with_prefs(Prefs::default());
    menu.start_listening(1);
    menu.apply_key_while_listening(Key::W);
    assert!(Screen::take_pending_modal(&mut menu).is_some());

    Screen::on_modal_cancel(&mut menu);
    Screen::on_modal_confirm(&mut menu);

    assert_eq!(
        menu.get_staged_binding(0),
        [Binding::key(Key::W), pad_stick(StickDir::Up)]
    );
    assert_eq!(
        menu.get_staged_binding(1),
        [Binding::key(Key::A), pad_stick(StickDir::Left)]
    );
}

#[test]
fn rebinding_ascend_to_f_keeps_its_pad_binding() {
    let mut menu = SettingsMenu::with_prefs(Prefs::default());
    menu.start_listening(4);

    menu.apply_key_while_listening(Key::F);

    assert_eq!(
        menu.get_staged_binding(4),
        [
            Binding::key(Key::F),
            Binding::pad(PadInput::Button(PadButton::RightTrigger))
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
            Binding::key(Key::Ctrl),
            Binding::pad(PadInput::Button(PadButton::LeftThumb))
        ]
    );
    assert_eq!(
        menu.get_staged_binding(5),
        [Binding::pad(PadInput::Button(PadButton::LeftTrigger))]
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

/// A modifier pressed alone during a frame binds the listening row.
#[test]
fn egui_lone_modifier_while_listening_binds_the_row() {
    let mut menu = SettingsMenu::with_prefs(Prefs::default());
    // Move Down holds Ctrl by default; free it first.
    menu.start_listening(5);
    menu.apply_key_while_listening(Key::Z);
    menu.start_listening(0);

    render_frame(
        &mut menu,
        vec![egui::Event::ModifiersChanged(egui::Modifiers::CTRL)],
    );

    assert_eq!(
        menu.get_staged_binding(0),
        [Binding::key(Key::Ctrl), pad_stick(StickDir::Up)]
    );
    assert!(!menu.is_listening());
}

/// Rebinding a row replaces its plain key and keeps its hold bindings, in the settings text too.
#[test]
fn rebinding_sprint_keeps_its_hold_bindings() {
    let (prefs, issues) = Prefs::parse("[bindings]\nsprint = Shift, Hold Q, Hold Pad West\n");
    assert_eq!(issues, vec![]);
    let mut menu = SettingsMenu::with_prefs(prefs);
    menu.start_listening(6);

    menu.apply_key_while_listening(Key::E);

    assert_eq!(
        menu.get_staged_binding(6),
        [
            Binding::key(Key::E),
            Binding::key(Key::Q).held(),
            Binding::pad(PadInput::Button(PadButton::West)).held()
        ]
    );
    let text = rendered_text(&render_frame(&mut menu, vec![]));
    assert!(
        text.lines().any(|l| l == "E, Hold Q"),
        "sprint row missing in:\n{text}"
    );
}

/// Rebinding a row replaces its plain key and keeps its chord bindings, in the settings text too.
#[test]
fn rebinding_keeps_chord_bindings() {
    let (prefs, issues) = Prefs::parse("[bindings]\nmove_forward = W, Ctrl+Q\n");
    assert_eq!(issues, vec![]);
    let mut menu = SettingsMenu::with_prefs(prefs);
    menu.start_listening(0);

    menu.apply_key_while_listening(Key::E);

    assert_eq!(
        menu.get_staged_binding(0),
        [
            Binding::key(Key::E),
            Binding::key(Key::Q).with(Modifiers::CTRL)
        ]
    );
    let text = rendered_text(&render_frame(&mut menu, vec![]));
    assert!(
        text.lines().any(|l| l == "E, Ctrl+Q"),
        "move forward row missing in:\n{text}"
    );
}

fn toggle_labels(output: &egui::FullOutput) -> usize {
    rendered_text(output)
        .lines()
        .filter(|line| *line == "Toggle")
        .count()
}

fn find_text_pos(shape: &egui::epaint::Shape, wanted: &str) -> Option<egui::Pos2> {
    match shape {
        egui::epaint::Shape::Text(text) if text.galley.text() == wanted => Some(text.pos),
        egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|s| find_text_pos(s, wanted)),
        _ => None,
    }
}

/// Clicks the text `wanted` as drawn by `menu`, over three frames of one context.
fn click_text(menu: &mut SettingsMenu, wanted: &str) {
    let ctx = egui::Context::default();
    let layout = run_ui(&ctx, egui::RawInput::default(), |ui| {
        menu.render(ui);
    });
    let pos = layout
        .shapes
        .iter()
        .find_map(|clipped| find_text_pos(&clipped.shape, wanted))
        .unwrap_or_else(|| panic!("{wanted:?} is not drawn:\n{}", rendered_text(&layout)))
        + egui::vec2(4.0, 4.0);
    let button = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    for events in [
        vec![egui::Event::PointerMoved(pos)],
        vec![button(true)],
        vec![button(false)],
    ] {
        let input = egui::RawInput {
            events,
            ..Default::default()
        };
        run_ui(&ctx, input, |ui| {
            menu.render(ui);
        });
    }
}

/// Only an action the game marks toggleable shows the hold/toggle choice.
#[test]
fn only_toggleable_rows_show_a_toggle() {
    let mut menu = SettingsMenu::with_prefs(Prefs::default());

    let output = render_frame(&mut menu, vec![]);

    assert_eq!(
        toggle_labels(&output),
        1,
        "one Toggle, on the Sprint row, in:\n{}",
        rendered_text(&output)
    );
    let find = |wanted: &str| {
        output
            .shapes
            .iter()
            .find_map(|clipped| find_text_pos(&clipped.shape, wanted))
            .unwrap_or_else(|| panic!("{wanted:?} is not drawn:\n{}", rendered_text(&output)))
    };
    let (sprint, toggle) = (find("Sprint:"), find("Toggle"));
    assert!(
        (sprint.y - toggle.y).abs() < 8.0,
        "Toggle at {toggle:?} is not on the Sprint row at {sprint:?}"
    );
}

/// Ticking a row's Toggle and saving writes the mode to `[action_modes]`.
#[test]
fn saving_a_toggle_writes_action_modes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("prefs.ini");
    let mut menu = SettingsMenu::with_prefs_at(Prefs::default(), path.clone());
    assert!(!menu.has_unsaved_changes());

    click_text(&mut menu, "Toggle");

    assert!(menu.has_unsaved_changes(), "the click staged a change");
    menu.apply_staged_changes().unwrap();
    let (saved, issues) = Prefs::load_from(&path);
    assert_eq!(issues, vec![]);
    assert_eq!(
        saved.action_modes().get("sprint").map(String::as_str),
        Some("toggle")
    );
    assert_eq!(saved.action_modes().get("move_forward"), None);
}

/// Runs one frame and clears its texture delta; egui asserts in debug builds
/// that a non-empty delta is not dropped, and these tests never paint it.
fn run_ui(
    ctx: &egui::Context,
    input: egui::RawInput,
    add_contents: impl FnMut(&mut egui::Ui),
) -> egui::FullOutput {
    let mut output = ctx.run_ui(input, add_contents);
    output.textures_delta.clear();
    output
}
