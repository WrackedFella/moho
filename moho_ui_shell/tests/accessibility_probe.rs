//! Drives the shell through egui's accessibility tree, the way a UI test harness finds widgets:
//! by role and label, never by pixel position.

use std::collections::BTreeMap;

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use moho_ui_shell::{Console, MenuEvent, MenuItem, MenuScreen, UiShell, menu};

const SIZE: [f32; 2] = [1280.0, 720.0];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Choice {
    Play,
    Settings,
    Quit,
}

fn main_screen() -> MenuScreen<Choice> {
    let button = |label: &str, action| MenuItem::Button {
        label: label.to_owned(),
        action,
    };
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

fn shell() -> UiShell<(), Choice> {
    let (console, _) = Console::<()>::new(&BTreeMap::new());
    UiShell::new(console)
}

fn menu_shell() -> UiShell<(), Choice> {
    let mut shell = shell();
    shell.menu_mut().push(main_screen());
    shell
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
fn a_row_overlay_found_by_role_and_label_chooses_its_action() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(SIZE[0], SIZE[1]))
        .build_ui_state(
            |ui, shell: &mut UiShell<(), Choice>| {
                shell.paint(ui.ctx());
                let screen = shell.menu_mut().top().cloned().expect("a screen is open");
                for (index, (item, row)) in screen
                    .items
                    .iter()
                    .zip(menu::layout(&screen, SIZE))
                    .enumerate()
                {
                    let MenuItem::Button { label, .. } = item else {
                        continue;
                    };
                    let rect = egui::Rect::from_min_max(row.min.into(), row.max.into());
                    let response =
                        ui.interact(rect, egui::Id::new(("row", index)), egui::Sense::click());
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label)
                    });
                    if response.clicked() {
                        let centre = rect.center();
                        shell.menu_mut().click([centre.x, centre.y], SIZE);
                    }
                }
            },
            menu_shell(),
        );
    harness.run();
    let before = harness.state_mut().menu_mut().take_events();

    harness
        .get_by_role_and_label(Role::Button, "Settings")
        .click();
    harness.run();

    let after = harness.state_mut().menu_mut().take_events();
    assert_eq!(before, [], "nothing is chosen before the click");
    assert_eq!(after, [MenuEvent::Chosen(Choice::Settings)]);
}

/// The production menu paints its rows as plain text, which egui does not expose as widgets, so a
/// harness cannot find them by label. A follow-up gives the rows an overlay of real widgets and
/// turns this around.
#[test]
fn menu_rows_expose_no_accessible_node_today() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(SIZE[0], SIZE[1]))
        .build_ui_state(
            |ui, shell: &mut UiShell<(), Choice>| shell.paint(ui.ctx()),
            menu_shell(),
        );
    harness.run();

    let row = harness.query_by_label("Settings");

    assert!(row.is_none());
}
