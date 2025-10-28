/// Reusable UI controls for creating consistent form elements across the application.
///
/// This module encapsulates common UI patterns for settings forms, including:
/// - Keybind controls with conflict detection and listening state
/// - Volume sliders with drag values and dirty state indicators
/// - Consistent styling and layout
///
/// # Example
/// ```ignore
/// let dirty = FormControls::volume_slider(ui, "Volume:", &mut value, original_value, 150.0);
/// if dirty {
///     dirty_fields.insert(Field::Volume);
/// }
/// ```
pub struct FormControls;

impl FormControls {
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

                let button =
                    ui.add(egui::Button::new(button_text).min_size(egui::vec2(120.0, 28.0)));
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
                        let slider = ui.add(egui::Slider::new(value, 1.0..=10.0).show_value(false));
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

    /// Standard three-panel layout for form screens.
    ///
    /// This helper reduces duplication by providing a consistent layout pattern:
    /// - Top panel: centered title with spacing
    /// - Bottom panel: right-aligned action buttons
    /// - Central panel: scrollable content area with horizontal margins
    ///
    /// # Arguments
    /// * `ctx` - The egui context
    /// * `title` - The screen title text
    /// * `panel_id_prefix` - Unique prefix for panel IDs (e.g., "settings", "new_world")
    /// * `render_buttons` - Closure to render bottom panel buttons, returns (primary_clicked, secondary_clicked)
    /// * `render_content` - Closure to render the central scrollable content
    ///
    /// # Returns
    /// Tuple of (primary_button_clicked, secondary_button_clicked)
    ///
    /// # Example
    /// ```ignore
    /// let (save_clicked, back_clicked) = FormControls::standard_screen_layout(
    ///     ctx,
    ///     "Settings",
    ///     "settings",
    ///     |ui| {
    ///         let save = ui.button("Save");
    ///         ui.add_space(12.0);
    ///         let back = ui.button("Back");
    ///         (save.clicked(), back.clicked())
    ///     },
    ///     |ui| {
    ///         ui.label("Content here");
    ///     }
    /// );
    /// ```
    pub fn standard_screen_layout<FB, FC>(
        ctx: &egui::Context,
        title: &str,
        panel_id_prefix: &str,
        mut render_buttons: FB,
        mut render_content: FC,
    ) -> (bool, bool)
    where
        FB: FnMut(&mut egui::Ui) -> (bool, bool),
        FC: FnMut(&mut egui::Ui),
    {
        let mut button_results = (false, false);

        // Top panel: title area
        egui::TopBottomPanel::top(format!("{}_top", panel_id_prefix)).show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(16.0);
                ui.heading(title);
                ui.add_space(12.0);
            });
        });

        // Bottom panel: action buttons
        egui::TopBottomPanel::bottom(format!("{}_bottom", panel_id_prefix)).show(ctx, |ui| {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    button_results = render_buttons(ui);
                });
            });
            ui.add_space(16.0);
        });

        // Central panel: scrollable content
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let gutter: f32 = 20.0;
                    let avail = ui.available_width();
                    ui.horizontal(|ui| {
                        ui.add_space(gutter);
                        ui.allocate_ui_with_layout(
                            egui::vec2((avail - 2.0 * gutter).max(0.0), 0.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                render_content(ui);
                            },
                        );
                        ui.add_space(gutter);
                    });
                });
        });

        button_results
    }

    /// Calculate responsive gutter size based on available width and percentage.
    ///
    /// This helper enables percentage-based layouts similar to WinForms or CSS,
    /// working around egui's lack of native percentage support.
    ///
    /// # Arguments
    /// * `available_width` - The total available width (from `ui.available_width()`)
    /// * `gutter_percent` - Percentage of width to use for each gutter (0.0 to 1.0)
    ///
    /// # Returns
    /// The calculated gutter size in pixels
    ///
    /// # Example
    /// ```ignore
    /// let gutter = FormControls::calculate_gutter(ui.available_width(), 0.30);
    /// // Creates 30% gutters on each side, 40% content width
    /// ```
    pub fn calculate_gutter(available_width: f32, gutter_percent: f32) -> f32 {
        (available_width * gutter_percent).max(20.0)
    }

    /// Calculate content width from available width and gutter percentage.
    ///
    /// # Arguments
    /// * `available_width` - The total available width
    /// * `gutter_percent` - Percentage for each side gutter (0.0 to 1.0)
    ///
    /// # Returns
    /// The calculated content width (total - 2*gutters)
    pub fn calculate_content_width(available_width: f32, gutter_percent: f32) -> f32 {
        let gutter = Self::calculate_gutter(available_width, gutter_percent);
        (available_width - 2.0 * gutter).max(100.0)
    }

    /// Renders a horizontal tab bar and returns the index of the selected tab.
    ///
    /// This creates a row of tab buttons styled to indicate active/inactive state.
    /// The active tab appears elevated with lighter background, while inactive tabs
    /// are darker and recessed.
    ///
    /// # Arguments
    /// * `ui` - The egui UI context
    /// * `tabs` - Slice of tab label strings
    /// * `active_index` - Index of the currently active tab (0-based)
    ///
    /// # Returns
    /// The index of the selected tab (may be unchanged if no tab was clicked)
    ///
    /// # Example
    /// ```ignore
    /// let tabs = ["Controls", "Audio", "Graphics"];
    /// let new_index = FormControls::tab_bar(ui, &tabs, self.active_tab_index);
    /// if new_index != self.active_tab_index {
    ///     self.active_tab_index = new_index;
    /// }
    /// ```
    pub fn tab_bar(ui: &mut egui::Ui, tabs: &[&str], active_index: usize) -> usize {
        let mut selected = active_index;

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;

            for (i, &label) in tabs.iter().enumerate() {
                let is_active = i == active_index;

                // Style the button based on active state
                let button = if is_active {
                    egui::Button::new(label)
                        .fill(egui::Color32::from_rgb(60, 60, 60))
                        .min_size(egui::vec2(100.0, 32.0))
                } else {
                    egui::Button::new(label)
                        .fill(egui::Color32::from_rgb(40, 40, 40))
                        .min_size(egui::vec2(100.0, 32.0))
                };

                let response = ui.add(button);

                // Update selection if clicked
                if response.clicked() {
                    selected = i;
                }

                // Add subtle visual feedback on hover for inactive tabs
                if !is_active && response.hovered() {
                    ui.painter().rect_filled(
                        response.rect,
                        4.0,
                        egui::Color32::from_rgba_premultiplied(50, 50, 50, 30),
                    );
                }
            }
        });

        selected
    }
}
