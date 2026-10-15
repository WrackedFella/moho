/// Renders the Audio tab content.
///
/// This includes volume sliders for sound effects, music, UI, and voice.
use super::{FormControls, SettingsField, SettingsMenu};

pub fn render(menu: &mut SettingsMenu, ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        // Audio Section Header
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Audio")
                    .size(18.0)
                    .color(egui::Color32::from_rgb(200, 200, 200)),
            );
        });
        ui.add_space(4.0);
        ui.separator();
        ui.add_space(8.0);

        let label_width = 150.0;

        // Sound Effect Volume
        {
            let saved_value = menu.state.prefs().sound_effect_volume();
            let dirty = FormControls::volume_slider(
                ui,
                "Sound Effects:",
                menu.state.staged_mut().sound_effect_volume_mut(),
                saved_value,
                label_width,
            );
            if dirty {
                menu.state.mark_dirty(SettingsField::AudioSoundEffect);
            }
        }

        // Music Volume
        {
            let saved_value = menu.state.prefs().music_volume();
            let dirty = FormControls::volume_slider(
                ui,
                "Music:",
                menu.state.staged_mut().music_volume_mut(),
                saved_value,
                label_width,
            );
            if dirty {
                menu.state.mark_dirty(SettingsField::AudioMusic);
            }
        }

        // UI Volume
        {
            let saved_value = menu.state.prefs().ui_volume();
            let dirty = FormControls::volume_slider(
                ui,
                "User Interface:",
                menu.state.staged_mut().ui_volume_mut(),
                saved_value,
                label_width,
            );
            if dirty {
                menu.state.mark_dirty(SettingsField::AudioUI);
            }
        }

        // Voice Volume
        {
            let saved_value = menu.state.prefs().voice_volume();
            let dirty = FormControls::volume_slider(
                ui,
                "Voice:",
                menu.state.staged_mut().voice_volume_mut(),
                saved_value,
                label_width,
            );
            if dirty {
                menu.state.mark_dirty(SettingsField::AudioVoice);
            }
        }
    });
}
