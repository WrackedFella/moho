//! Reads the settings screen the way a player does, by role and label, with no display or GPU.
//!
//! Saving writes to a temporary directory through `SettingsMenu::with_prefs_at`, never to the real
//! prefs file.

use std::path::Path;

use egui::accesskit::{Role, Toggled};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use flagstone_ui::UiComponent;
use flagstone_ui::prefs::Prefs;
use flagstone_ui::screens::{SettingsMenu, SettingsTab};

const SIZE: egui::Vec2 = egui::vec2(1280.0, 720.0);

fn harness_of(menu: SettingsMenu) -> Harness<'static, SettingsMenu> {
    let mut harness = Harness::builder().with_size(SIZE).build_ui_state(
        |ui, menu: &mut SettingsMenu| {
            menu.render(ui);
        },
        menu,
    );
    harness.run();
    harness
}

fn toggled_of(harness: &Harness<'static, SettingsMenu>, label: &str) -> Option<Toggled> {
    harness
        .get_by_role_and_label(Role::CheckBox, label)
        .accesskit_node()
        .toggled()
}

fn text_of(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
    match shape {
        egui::epaint::Shape::Text(text) => out.push(text.galley.text().to_owned()),
        egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|shape| text_of(shape, out)),
        _ => {}
    }
}

/// Every text the menu draws on its current tab in one frame.
fn drawn_texts(menu: &mut SettingsMenu) -> Vec<String> {
    let ctx = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
        ..egui::RawInput::default()
    };
    let mut first = ctx.run_ui(input.clone(), |ui| {
        menu.render(ui);
    });
    first.textures_delta.clear();
    let mut output = ctx.run_ui(input, |ui| {
        menu.render(ui);
    });
    output.textures_delta.clear();
    let mut texts = Vec::new();
    for clipped in &output.shapes {
        text_of(&clipped.shape, &mut texts);
    }
    texts
}

#[test]
fn invert_mouse_y_found_by_its_label_reads_checked_and_is_saved() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("prefs.ini");
    let mut harness = harness_of(SettingsMenu::with_prefs_at(Prefs::default(), path.clone()));
    let before = toggled_of(&harness, "Invert mouse Y");

    harness
        .get_by_role_and_label(Role::CheckBox, "Invert mouse Y")
        .click();
    harness.run();
    let after = toggled_of(&harness, "Invert mouse Y");
    let saved_early = Path::new(&path).exists();
    harness
        .get_by_role_and_label(Role::Button, "Save Changes")
        .click();
    harness.run();

    assert_eq!(before, Some(Toggled::False));
    assert_eq!(after, Some(Toggled::True));
    assert!(!saved_early, "nothing is saved before Save Changes");
    let (saved, warnings) = Prefs::load_from(&path);
    assert_eq!(warnings, vec![]);
    assert!(saved.invert_mouse_y());
}

#[test]
fn every_toggle_on_the_controls_tab_is_found_by_its_row_label() {
    let harness = harness_of(SettingsMenu::with_prefs(Prefs::default()));

    let found = ["Mouse acceleration", "Invert mouse Y", "Mouse smoothing"].map(|label| {
        harness
            .query_by_role_and_label(Role::CheckBox, label)
            .is_some()
    });
    let unlabelled = harness
        .query_all_by_role_and_label(Role::CheckBox, "Enable")
        .count();

    assert_eq!(found, [true, true, true]);
    assert_eq!(unlabelled, 0, "no checkbox keeps the generic Enable label");
}

#[test]
fn the_vsync_toggle_on_the_video_tab_is_found_by_its_row_label() {
    let mut menu = SettingsMenu::with_prefs(Prefs::default());
    menu.set_active_tab(SettingsTab::Video);
    let harness = harness_of(menu);

    let vsync = harness.query_by_role_and_label(Role::CheckBox, "Vsync");
    let unlabelled = harness
        .query_all_by_role_and_label(Role::CheckBox, "Enable")
        .count();

    assert!(vsync.is_some(), "Vsync is a labelled checkbox");
    assert_eq!(unlabelled, 0);
}

#[test]
fn the_enable_text_is_still_drawn_beside_each_toggle() {
    let mut menu = SettingsMenu::with_prefs(Prefs::default());

    let texts = drawn_texts(&mut menu);

    let enables = texts
        .iter()
        .filter(|text| text.as_str() == "Enable")
        .count();
    assert_eq!(enables, 3, "one visible Enable per toggle row: {texts:?}");
    assert!(
        texts.iter().any(|text| text == "Invert mouse Y:"),
        "the row label keeps its colon: {texts:?}"
    );
}
