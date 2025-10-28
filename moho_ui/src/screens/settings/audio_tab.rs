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
            let dirty = FormControls::volume_slider(
                ui,
                "Sound Effects:",
                &mut menu.staged.audio_sound_effect_volume,
                menu.prefs.audio_sound_effect_volume,
                label_width,
            );
            if dirty {
                menu.dirty_fields
                    .insert(SettingsField::AudioSoundEffect);
            } else {
                menu.dirty_fields
                    .remove(&SettingsField::AudioSoundEffect);
            }
        }

        // Music Volume
        {
            let dirty = FormControls::volume_slider(
                ui,
                "Music:",
                &mut menu.staged.audio_music_volume,
                menu.prefs.audio_music_volume,
                label_width,
            );
            if dirty {
                menu.dirty_fields.insert(SettingsField::AudioMusic);
            } else {
                menu.dirty_fields.remove(&SettingsField::AudioMusic);
            }
        }

        // UI Volume
        {
            let dirty = FormControls::volume_slider(
                ui,
                "User Interface:",
                &mut menu.staged.audio_ui_volume,
                menu.prefs.audio_ui_volume,
                label_width,
            );
            if dirty {
                menu.dirty_fields.insert(SettingsField::AudioUI);
            } else {
                menu.dirty_fields.remove(&SettingsField::AudioUI);
            }
        }

        // Voice Volume
        {
            let dirty = FormControls::volume_slider(
                ui,
                "Voice:",
                &mut menu.staged.audio_voice_volume,
                menu.prefs.audio_voice_volume,
                label_width,
            );
            if dirty {
                menu.dirty_fields.insert(SettingsField::AudioVoice);
            } else {
                menu.dirty_fields.remove(&SettingsField::AudioVoice);
            }
        }
    });
}
