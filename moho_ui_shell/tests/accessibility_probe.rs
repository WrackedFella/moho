//! Drives the shell through egui's accessibility tree, the way a UI test harness finds widgets:
//! by role and label, never by pixel position.

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

use egui::accesskit::{Role, Toggled};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use moho_core::prefs::Prefs;
use moho_input::bindings::Input;
use moho_input::key::MouseButton;
use moho_ui_shell::{
    Console, EngineSetting, EngineSettings, MenuEvent, MenuItem, MenuScreen, UiShell,
};

const SIZE: [f32; 2] = [1280.0, 720.0];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Choice {
    Play,
    Settings,
    Quit,
    Back,
}

fn button(label: &str, action: Choice) -> MenuItem<Choice> {
    MenuItem::Button {
        label: label.to_owned(),
        action,
    }
}

fn main_screen() -> MenuScreen<Choice> {
    MenuScreen {
        title: None,
        items: vec![
            button("Play", Choice::Play),
            button("Settings", Choice::Settings),
            button("Quit", Choice::Quit),
        ],
        back: None,
        settings: None,
    }
}

fn base_settings() -> EngineSettings {
    EngineSettings::from_prefs(&Prefs::default())
}

/// Every engine row, then Apply: thirteen rows, of which ten fit at 720 points.
fn settings_screen() -> MenuScreen<Choice> {
    let mut items: Vec<MenuItem<Choice>> = EngineSetting::ALL
        .into_iter()
        .map(MenuItem::Setting)
        .collect();
    items.push(MenuItem::Apply);
    MenuScreen {
        title: None,
        items,
        back: Some(Choice::Back),
        settings: Some(base_settings()),
    }
}

fn shell() -> UiShell<(), Choice> {
    let (console, _) = Console::<()>::new(&BTreeMap::new());
    UiShell::new(console)
}

fn shell_showing(screen: MenuScreen<Choice>) -> UiShell<(), Choice> {
    let mut shell = shell();
    shell.menu_mut().push(screen);
    shell
}

fn menu_shell() -> UiShell<(), Choice> {
    shell_showing(main_screen())
}

fn harness_of(shell: UiShell<(), Choice>) -> Harness<'static, UiShell<(), Choice>> {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(SIZE[0], SIZE[1]))
        .build_ui_state(
            |ui, shell: &mut UiShell<(), Choice>| shell.paint(ui.ctx()),
            shell,
        );
    harness.run();
    harness
}

/// A left click at the centre of `rect` through the shell's own input path, as the game routes it.
fn click_through_route(shell: &mut UiShell<(), Choice>, rect: egui::Rect) {
    let centre = rect.center();
    shell.cursor_moved([f64::from(centre.x), f64::from(centre.y)], 1.0);
    shell.route(Input::Mouse(MouseButton::Left), true, false);
    shell.route(Input::Mouse(MouseButton::Left), false, false);
}

/// Finds the control by role and label, clicks it the way a player does and lets the frame settle.
fn click_found(harness: &mut Harness<'static, UiShell<(), Choice>>, role: Role, label: &str) {
    let rect = harness.get_by_role_and_label(role, label).rect();
    click_through_route(harness.state_mut(), rect);
    harness.run();
}

fn toggled_of(
    harness: &Harness<'static, UiShell<(), Choice>>,
    role: Role,
    label: &str,
) -> Option<Toggled> {
    harness
        .get_by_role_and_label(role, label)
        .accesskit_node()
        .toggled()
}

type Seen = (String, String, Option<String>, Option<String>);

/// The role, label, toggled state and value of every node the shell exposes.
fn nodes_seen(harness: &Harness<'static, UiShell<(), Choice>>) -> Vec<Seen> {
    harness
        .root()
        .children_recursive()
        .map(|node| {
            let inner = node.accesskit_node();
            (
                format!("{:?}", inner.role()),
                inner.label().unwrap_or_default(),
                inner.toggled().map(|toggled| format!("{toggled:?}")),
                inner
                    .value()
                    .or_else(|| inner.numeric_value().map(|value| value.to_string())),
            )
        })
        .collect()
}

/// The toggle flow: Invert mouse Y found by label and clicked, then Apply after scrolling to it.
/// Returns what the player read before and after the click, every node seen, and the events.
fn toggle_flow() -> (
    Option<Toggled>,
    Option<Toggled>,
    Vec<Seen>,
    Vec<MenuEvent<Choice>>,
) {
    let mut harness = harness_of(shell_showing(settings_screen()));
    let before = toggled_of(&harness, Role::CheckBox, "Invert mouse Y");

    click_found(&mut harness, Role::CheckBox, "Invert mouse Y");
    let after = toggled_of(&harness, Role::CheckBox, "Invert mouse Y");
    let seen = nodes_seen(&harness);
    harness.state_mut().menu_mut().scroll(1000.0);
    harness.run();
    click_found(&mut harness, Role::Button, "Apply");

    let events = harness.state_mut().menu_mut().take_events();
    (before, after, seen, events)
}

#[test]
fn the_console_line_found_by_role_runs_help_on_enter() {
    let mut shell = shell();
    shell.console_mut().open();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(SIZE[0], SIZE[1]))
        .build_ui_state(
            |ui, shell: &mut UiShell<(), Choice>| shell.paint(ui.ctx()),
            shell,
        );
    harness.run();

    harness.get_by_role(Role::TextInput).type_text("help");
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();

    let console = harness.state_mut().console_mut();
    let lines: Vec<&str> = console
        .output()
        .lines()
        .iter()
        .map(String::as_str)
        .collect();
    assert!(lines.contains(&"> help"), "the echo is missing: {lines:?}");
    assert!(
        lines.contains(&"  help - list the commands"),
        "the first listing line is missing: {lines:?}"
    );
    assert!(
        lines
            .contains(&"  r_debug_view <0-8> - set the debug view (0=None, 3=Shadows, 8=Cascades)"),
        "an engine command line is missing: {lines:?}"
    );
    assert!(
        lines.contains(
            &"  audio_cue <path> <x> <y> <z> [volume] - play a sound at a world position (metres)"
        ),
        "the last listing line is missing: {lines:?}"
    );
    assert_eq!(console.input(), "", "Enter submits the line and clears it");
}

#[test]
fn a_settings_toggle_found_by_its_label_reads_checked_and_applies_on() {
    let (before, after, _, events) = toggle_flow();

    assert_eq!(before, Some(Toggled::False));
    assert_eq!(after, Some(Toggled::True));
    assert_eq!(
        events,
        [MenuEvent::SettingsApplied(EngineSettings {
            invert_mouse_y: true,
            ..base_settings()
        })]
    );
}

#[test]
fn a_missing_button_fails_naming_its_role_and_label() {
    let harness = harness_of(menu_shell());

    let failure = catch_unwind(AssertUnwindSafe(|| {
        harness.get_by_role_and_label(Role::Button, "Resume");
    }))
    .expect_err("a screen without Resume has no such button");

    let text = failure
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| {
            failure
                .downcast_ref::<&str>()
                .map(|text| (*text).to_owned())
        })
        .expect("the panic carries a message");
    assert!(text.contains("label: \"Resume\""), "label missing: {text}");
    assert!(text.contains("role: Button"), "role missing: {text}");
}

#[test]
fn the_settings_check_sees_the_same_controls_every_run() {
    let (_, _, first, _) = toggle_flow();
    let (_, _, second, _) = toggle_flow();

    assert!(
        first
            .iter()
            .any(|(role, label, toggled, _)| role == "CheckBox"
                && label == "Invert mouse Y"
                && toggled.as_deref() == Some("True")),
        "the toggle is among the nodes read: {first:?}"
    );
    assert_eq!(first, second);
}

#[test]
fn an_egui_click_on_a_row_node_chooses_nothing() {
    let mut harness = harness_of(menu_shell());

    harness
        .get_by_role_and_label(Role::Button, "Settings")
        .click();
    harness.run();

    let events = harness.state_mut().menu_mut().take_events();
    assert_eq!(events, [], "input stays on route, not on egui");
}

#[test]
fn a_row_click_through_route_chooses_its_action() {
    let mut harness = harness_of(menu_shell());

    click_found(&mut harness, Role::Button, "Settings");

    let events = harness.state_mut().menu_mut().take_events();
    assert_eq!(events, [MenuEvent::Chosen(Choice::Settings)]);
}

#[test]
fn row_nodes_paint_nothing() {
    let harness = harness_of(menu_shell());

    harness.get_by_role_and_label(Role::Button, "Settings");

    let layer = egui::LayerId::new(egui::Order::Background, egui::Id::new("moho_menu_rows"));
    // egui drops a layer that only hovered once the pass is drained, so absent counts as empty.
    let shapes = harness.ctx.graphics(|graphics| {
        graphics
            .get(layer)
            .map_or(0, |list| list.all_entries().count())
    });
    assert_eq!(shapes, 0, "the layer holds no shapes");
}

#[test]
fn a_slider_and_a_choice_row_are_found_by_role_and_label() {
    let harness = harness_of(shell_showing(settings_screen()));

    let slider = harness.get_by_role_and_label(Role::Slider, "Mouse sensitivity");
    let choice = harness.get_by_role_and_label(Role::ComboBox, "Shadow quality");

    assert_eq!(
        slider.accesskit_node().numeric_value(),
        Some(f64::from(base_settings().mouse_sensitivity))
    );
    assert_eq!(
        choice.accesskit_node().value().as_deref(),
        Some(
            base_settings()
                .value_text(EngineSetting::ShadowQuality)
                .as_str()
        )
    );
}
