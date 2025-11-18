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
            let saved_value = menu.state.prefs().audio_sound_effect_volume;
            let dirty = FormControls::volume_slider(
                ui,
                "Sound Effects:",
                &mut menu.state.staged_mut().audio_sound_effect_volume,
                saved_value,
                label_width,
            );
            if dirty {
                menu.state.mark_dirty(SettingsField::AudioSoundEffect);
            }
        }

        // Music Volume
        {
            let saved_value = menu.state.prefs().audio_music_volume;
            let dirty = FormControls::volume_slider(
                ui,
                "Music:",
                &mut menu.state.staged_mut().audio_music_volume,
                saved_value,
                label_width,
            );
            if dirty {
                menu.state.mark_dirty(SettingsField::AudioMusic);
            }
        }

        // UI Volume
        {
            let saved_value = menu.state.prefs().audio_ui_volume;
            let dirty = FormControls::volume_slider(
                ui,
                "User Interface:",
                &mut menu.state.staged_mut().audio_ui_volume,
                saved_value,
                label_width,
            );
            if dirty {
                menu.state.mark_dirty(SettingsField::AudioUI);
            }
        }

        // Voice Volume
        {
            let saved_value = menu.state.prefs().audio_voice_volume;
            let dirty = FormControls::volume_slider(
                ui,
                "Voice:",
                &mut menu.state.staged_mut().audio_voice_volume,
                saved_value,
                label_width,
            );
            if dirty {
                menu.state.mark_dirty(SettingsField::AudioVoice);
            }
        }
    });
}
