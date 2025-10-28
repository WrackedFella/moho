# Responsive Layouts in moho_ui

## Overview

While egui doesn't natively support percentage-based layouts (like CSS or WinForms), we can achieve similar responsive behavior using calculated pixel values based on available space.

## Pattern: Percentage-Based Gutters

### The Problem
Fixed pixel gutters (e.g., 20px) don't scale well across different screen sizes. On large monitors, content appears cramped; on small screens, there's too little content space.

### The Solution
Calculate gutter sizes as a percentage of available width:

```rust
let available_width = ui.available_width();
let gutter = FormControls::calculate_gutter(available_width, 0.30);
// Creates 30% gutters on each side, leaving 40% for content
```

### Helper Functions

The `FormControls` module provides two helpers:

#### `calculate_gutter(available_width, gutter_percent)`
Returns the pixel size for one gutter (left or right).
- Minimum of 20px to prevent unreasonably small gutters
- `gutter_percent`: 0.0 to 1.0 (e.g., 0.30 = 30%)

#### `calculate_content_width(available_width, gutter_percent)`
Returns the content width after accounting for both gutters.
- Minimum of 100px to ensure usable content area

## Usage Example

```rust
egui::CentralPanel::default().show(ctx, |ui| {
    egui::ScrollArea::vertical().show(ui, |ui| {
        let avail = ui.available_width();
        let gutter = FormControls::calculate_gutter(avail, 0.30);
        
        ui.horizontal(|ui| {
            ui.add_space(gutter);
            ui.allocate_ui_with_layout(
                egui::vec2((avail - 2.0 * gutter).max(0.0), 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    // Your form content here
                },
            );
            ui.add_space(gutter);
        });
    });
});
```

## Philosophy: WinForms-Style Layouts

Our approach follows patterns from Microsoft WinForms and similar UI frameworks:

1. **Measure first, layout second**: Query available space, then calculate sizes
2. **Responsive by calculation**: Use percentages calculated to pixels at runtime
3. **Minimum constraints**: Always enforce sensible minimums (20px gutters, 100px content)
4. **Reusable helpers**: Encapsulate layout logic in `FormControls` for consistency

## Best Practices

### Standard Percentages
- **30% gutters** (0.30): Good for settings/form screens, brings labels and controls closer
- **15% gutters** (0.15): More content-focused layouts
- **40% gutters** (0.40): Very focused, centered forms (login screens, etc.)

### Spacing Guidelines
- **Top panel**: 16px top, 12px bottom (title areas)
- **Bottom panel**: 12px top, 16px bottom (action buttons)
- **Section spacing**: 24px between major sections
- **Control spacing**: 8-12px between individual controls

### When to Use Fixed vs Percentage
- **Use percentage gutters**: For main content areas that should scale with screen size
- **Use fixed pixels**: For control heights, button sizes, and small spacing adjustments

## Future Enhancements

Potential additions to the responsive layout system:

1. **Breakpoint helpers**: Different gutter percentages for different screen widths
2. **Grid layouts**: Calculate column widths based on percentages
3. **Adaptive control sizes**: Scale button/input heights on very large/small screens
4. **Layout presets**: Named configurations (e.g., "centered-form", "wide-content")

## See Also
- `moho_ui/src/screens/form_controls.rs` - Implementation of layout helpers
- `moho_ui/src/screens/settings.rs` - Example usage of percentage-based gutters
