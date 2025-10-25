/// A reusable form builder for creating consistent UI controls across the application.
/// 
/// This builder encapsulates common UI patterns for settings forms, including:
/// - Keybind controls with conflict detection and listening state
/// - Volume sliders with drag values and dirty state indicators
/// - Consistent styling and layout
/// 
/// # Example
/// ```ignore
/// let dirty = FormBuilder::volume_slider(ui, "Volume:", &mut value, original_value, 150.0);
/// if dirty {
///     dirty_fields.insert(Field::Volume);
/// }
/// ```
pub struct FormBuilder;

impl FormBuilder {
    /// Renders a keybind control with a label, current binding display, and listen button.
    ///
    /// # Arguments
    /// * `ui` - The egui UI context
    /// * `label` - The label text for this keybind
    /// * `binding_text` - The text to display on the button (e.g., "W", "Spacebar")
    /// * `is_dirty` - Whether this control has unsaved changes
    /// * `is_listening` - Whether this control is currently listening for input
    /// * `label_width` - The width allocated for the label area
    ///
    /// # Returns
    /// `true` if the listen button was clicked this frame
    pub fn keybind_control(
        ui: &mut egui::Ui,
        label: &str,
        binding_text: &str,
        is_dirty: bool,
        is_listening: bool,
        label_width: f32,
    ) -> bool {
        let mut was_clicked = false;

        ui.horizontal(|ui| {
            // Label area
            ui.allocate_ui_with_layout(
                egui::vec2(label_width, 28.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.label(label);
                },
            );

            // Control area (right-aligned)
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let button_text = if is_listening {
                    "Press any key..."
                } else {
                    binding_text
                };

                let button = ui.add(egui::Button::new(button_text).min_size(egui::vec2(120.0, 28.0)));
                if button.clicked() {
                    was_clicked = true;
                }

                // Paint dirty indicator if binding has changed
                if is_dirty {
                    Self::paint_dirty_decor(ui, &button);
                }
            });
        });

        was_clicked
    }

    /// Renders a volume slider control with both a slider and drag value input.
    ///
    /// # Arguments
    /// * `ui` - The egui UI context
    /// * `label` - The label text for this volume control
    /// * `value` - Mutable reference to the volume value (will be modified by UI)
    /// * `original_value` - The original (saved) volume value for dirty checking
    /// * `label_width` - The width allocated for the label area
    ///
    /// # Returns
    /// `true` if the value differs from the original (is dirty), `false` otherwise
    pub fn volume_slider(
        ui: &mut egui::Ui,
        label: &str,
        value: &mut f32,
        original_value: f32,
        label_width: f32,
    ) -> bool {
        let is_dirty = (*value - original_value).abs() > f32::EPSILON;

        ui.horizontal(|ui| {
            // Label area
            ui.allocate_ui_with_layout(
                egui::vec2(label_width, 28.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.label(label);
                },
            );

            // Control area (right-aligned)
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Slider
                ui.allocate_ui_with_layout(
                    egui::vec2(120.0, 20.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.spacing_mut().slider_width = 120.0;
                        let slider = ui.add(
                            egui::Slider::new(value, 1.0..=10.0).show_value(false),
                        );
                        if is_dirty {
                            Self::paint_dirty_decor(ui, &slider);
                        }
                    },
                );

                ui.add_space(8.0);

                // Drag value
                let drag = ui.add(
                    egui::DragValue::new(value)
                        .range(1.0..=10.0)
                        .speed(0.1)
                        .min_decimals(1)
                        .max_decimals(1),
                );
                if is_dirty {
                    Self::paint_dirty_decor(ui, &drag);
                }
            });
        });

        is_dirty
    }

    /// Paints a colored border around a widget to indicate it has unsaved changes.
    ///
    /// # Arguments
    /// * `ui` - The egui UI context
    /// * `response` - The widget response to decorate
    fn paint_dirty_decor(ui: &egui::Ui, response: &egui::Response) {
        let rect = response.rect;
        let color = egui::Color32::from_rgba_premultiplied(150, 150, 150, 60);
        ui.painter().rect_filled(rect, 4.0, color);
    }
}
