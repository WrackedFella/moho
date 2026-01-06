# moho_ui



egui-based UI system for the Moho engine. Provides menus, settings screens, overlays, and console integration with event-driven communication to the main engine.



## Features



- **Start Menu** - Main menu with New World, Continue, Settings, Exit

- **Settings Screen** - Tabbed interface for Controls and Audio settings

- **New World Menu** - World generation configuration (size, seed, terrain type)

- **Debug Console** - Command execution and output display

- **HUD Overlay** - In-game information display

- **Modal Dialogs** - Confirmation prompts and notifications

- **Event-Driven** - Publishes `UiEvent` to engine event bus

- **Keybind Capture** - Click-to-rebind input controls

- **Persistent Settings** - Saves preferences to `config/prefs.ini`



## Architecture



### Core Components



- **`EguiAdapter`** - Integrates egui with wgpu/winit, handles rendering lifecycle

- **`UiStateManager`** - Manages which screen/overlay is currently active

- **`ScreenSpec`** - Trait defining full-screen UIs (menus, settings, etc.)

- **`Overlay`** - Trait for in-game overlays (console, HUD, etc.)

- **`FormControls`** - Reusable UI control helpers (buttons, sliders, text fields, tabs)



### Screen Flow



```

StartMenu â†’ NewWorldMenu â†’ Playing (with HUD)

         â†“                      â†“

    SettingsMenu â†â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ Console (backtick key)

         â†“

    StartMenu

```



## Quick Start



### Building with UI



```bash

# Run with egui UI

cargo run --features "backend-wgpu ui-egui"



# Run with debug output

cargo run --features "backend-wgpu ui-egui ui-egui-debug"

```



### Basic Integration



```rust

use moho_ui::{EguiAdapter, UiStateManager};

use moho_core::events::EventBus;

use std::sync::Arc;



// Create UI adapter

let event_bus = Arc::new(EventBus::new());

let adapter = EguiAdapter::new(

    Some(window.clone()),

    event_bus.clone(),

);



// Create UI state manager

let mut ui_manager = UiStateManager::new();



// Each frame

adapter.handle_window_event(&event, &window);

adapter.render_frame(&mut ui_manager, &game_state)?;

```



### Handling UI Events



UI events are published to the engine's `EventBus`:



```rust

use moho_core::events::{EventBus, UiEvent};



bus.subscribe(|event: &UiEvent| {

    match event {

        UiEvent::NewWorldRequested { size_xz, seed, terrain_type } => {

            // Generate new world

        }

        UiEvent::ButtonClicked { name } => {

            println!("Button clicked: {}", name);

        }

        UiEvent::MenuShown { name } => {

            println!("Menu shown: {}", name);

        }

        _ => {}

    }

});

```



## Creating Custom Screens

- Short diagnosis: The non-egui fallback used a different geometry/coordinate

Implement the `ScreenSpec` trait for full-screen UIs:  space than the painted egui widgets â€” at one point the fallback used hard

  coded rectangles while the painted UI moved/laid out buttons based on

```rust  egui's layout (and egui rectangles were in logical points). This mismatch

use moho_ui::ScreenSpec;  caused the fallback to classify a click using a different box than the one

  the user saw.

pub struct MyCustomScreen {

    // Screen state- Fixes applied:

}  1. Always record the egui `Response.rect` for buttons each frame and use

     those stored rects in the fallback hit-tests. This removes duplicated

impl ScreenSpec for MyCustomScreen {     geometry definitions and makes the fallback match exactly what egui

    fn title(&self) -> &str {     painted.

        "My Custom Screen"  2. Ensure cursor positions are converted to logical points with

    }     `window.scale_factor()` where we capture them. This keeps both sides of

     the containment test in the same coordinate space.

    fn render(&mut self, ui: &mut egui::Ui, event_bus: &EventBus) {  3. Centralize OS cursor/grab side-effects in `main` by having the adapter

        ui.heading("Custom Content");     send `UiEvent::OverlayToggled(bool)` instead of directly setting

             cursor/grab in multiple places. This prevents races and inconsistent

        if ui.button("Do Something").clicked() {     cursor state.

            event_bus.publish(UiEvent::ButtonClicked {  4. Add diagnostic logging (and debug rectangles) to help spot remaining

                name: "my_button".to_string()     mismatches quickly.

            });

        }Known remaining quirks and recommendations

    }

- The fallback path exists to be robust while egui is being integrated. If

    fn should_capture_mouse(&self) -> bool {  you fully trust egui_winit in your platform/versions you can remove the

        true // Screen captures mouse input  fallback code â€” but keep in mind press+release between frames can still

    }  happen and may be surprising without the fallback.

}

```- Keep `recall_staging_belt()` usage in your submit path. It's easy to

  forget and will silently leak staging memory.

## Creating Overlays

- If you port or update egui/egui-winit/egui-wgpu versions, re-check the

Implement the `Overlay` trait for in-game overlays:  `egui_winit::State` constructor and any `egui_wgpu::Renderer` API â€” egui

  and friends have historically changed their initialization signatures

```rust  between minor versions.

use moho_ui::Overlay;

- When adding new top-left widgets, remember the fallback relies on the

pub struct MyOverlay {  `Response.rect`s being stored before any immediate hit-tests happen. Don't

    visible: bool,  move rect storage to a conditional branch that runs only when the widget

}  is clicked.



impl Overlay for MyOverlay {8) egui Slider width control quirk

    fn is_visible(&self) -> bool {- **Problem**: `egui::Slider` does not respect `ui.add_sized(...)` for its width

        self.visible  the way `TextEdit` and other widgets do. Using `add_sized(egui::vec2(width, height), Slider::new(...))`

    }  will NOT make the slider's draggable bar expand to fill the specified width.

 

    fn toggle(&mut self) {- **Root cause**: egui's `Slider` widget has internal sizing logic that uses

        self.visible = !self.visible;  `ui.spacing().slider_width` to determine the width of the draggable bar portion.

    }  The slider ignores the outer `add_sized` constraint and instead consults this

  spacing property.

    fn render(&mut self, ctx: &egui::Context, event_bus: &EventBus) {

        egui::Window::new("My Overlay")- **Solution**: To make a slider wider than the default, you must set

            .show(ctx, |ui| {  `ui.spacing_mut().slider_width` to the desired width (minus space for the

                ui.label("Overlay content");  value display and padding, typically ~60px) before adding the slider:

            });  

    }  ```rust

}  ui.horizontal(|ui| {

```      ui.allocate_ui_with_layout(egui::vec2(label_width, 0.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {

          ui.label("World Size");

## FormControls - Reusable UI Components      });

      ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {

The `FormControls` struct provides standardized UI components:          let slider_width = field_width * 1.5; // e.g., 540px if field_width is 360px

          ui.spacing_mut().slider_width = slider_width - 60.0; // Account for value display box

### Buttons          ui.add(egui::Slider::new(&mut self.size_xz, 64..=256).min_decimals(0));

      });

```rust  });

use moho_ui::FormControls;  ```



let controls = FormControls::new(360.0, 180.0); // field_width, label_width- **Example**: In `NewWorldMenu`, we wanted the World Size slider to be 50% wider

  than the text fields (360px â†’ 540px). Using `add_sized` alone did not work.

// Standard button  Setting `ui.spacing_mut().slider_width = 480.0` (540px - 60px) before adding

if controls.button(ui, "Click Me", 200.0) {  the slider successfully made the draggable bar expand to the desired width.

    // Button clicked

}- **Tip**: The subtracted padding (60px in our case) accounts for egui's internal

  spacing, the value display box on the right, and margins. You may need to adjust

// Button row (left-aligned labels, right-aligned buttons)  this value if your slider uses custom formatters or different styling.

controls.button_row(ui, "Action", "Execute", 200.0, || {

    // Button clicked callbackHow to run with debug output (PowerShell)

});

``````powershell

# run with egui + debug helper logs

### Text Fieldscargo run -p moho --features "backend-wgpu ui-egui ui-egui-debug"

```

```rust

// Standard text fieldDevelopment notes / future improvements

controls.text_field(ui, "Name", &mut name_string);

- Replace the manual fallback hit-test with a dedicated input-glue layer that

// Number input  consistently converts winit events to egui `RawInput` and guarantees

controls.number_field(ui, "Seed", &mut seed_value);  event ordering. This will reduce the need for ad-hoc press-tracking.

```- Add a small integration test harness (headless or offscreen) that simulates

  press+release sequences to validate fallback behavior.

### Sliders- Consider exposing a small utility that returns the last-stored button

  rects for unit tests (currently they are stored on the adapter struct).

```rust

// Slider with labelIf you want, I can:

controls.slider(ui, "Volume", &mut volume, 0.0..=1.0);- Add a short unit/integration test that exercises the press+release fallback

  logic.

// Slider with custom format- Add the README content to the top-level docs or `docs/` folder instead.

ui.add(egui::Slider::new(&mut value, 0..=100).suffix("%"));

```Recent input & UI notes



### Tabs- InputDispatcher: the app now uses a small prioritized dispatcher to route `WindowEvent`s. The settings keybind capture registers at a higher priority so it can intercept events while the settings menu is listening.

- Shared mapping: the physical key â†’ binding mapping lives in the workspace crate `moho_input` to avoid duplication between the binary and `moho_ui`.

```rust- Wheel policy: mouse-wheel events are forwarded to the game only when the UI overlay is hidden. The forwarder uses a conservative `try_lock()` behavior and will *not* forward if the UI lock cannot be obtained.

enum MyTabs {

    Tab1,These items are small, self-contained, and were added to make keybind capture and UI/game input composition predictable.

    Tab2,

}---

Small note: this adapter intentionally contains a few pragmatic

let tab_labels = ["Tab 1", "Tab 2"];engineering choices (warmup pass, synthetic pointer injection, fallback

let selected = controls.tab_bar(ui, &tab_labels, current_tab);hit-tests) to make the UI feel responsive in a minimal embedding. When you

fully wire a richer UI and input system, a simpler integration (pure

match MyTabs::from_index(selected) {`egui_winit` + no fallback) may be preferable.

    MyTabs::Tab1 => render_tab1(ui),
    MyTabs::Tab2 => render_tab2(ui),
}
```

### Standard Layouts

```rust
// Three-panel layout (title, content, buttons)
controls.standard_screen_layout(
    ui,
    "Screen Title",
    |ui| {
        // Render bottom buttons
        if controls.button(ui, "OK", 200.0) {
            // Handle OK
        }
    },
    |ui| {
        // Render scrollable central content
        ui.label("Content here");
    }
);
```

## Settings Management

Settings are automatically loaded from and saved to `config/prefs.ini`:

```rust
use moho_ui::Prefs;

// Load settings
let prefs = Prefs::load()?;

// Access keybindings
let forward_key = prefs.forward;
let back_key = prefs.back;

// Access audio settings
let master_volume = prefs.master_volume;
let music_volume = prefs.music_volume;

// Save settings
prefs.save()?;
```

### Keybind Capture

The settings screen provides click-to-rebind functionality:

1. User clicks a keybind control
2. Control enters "capturing" state
3. User presses desired key
4. New binding is saved to prefs.ini

## Console Integration

The debug console overlay supports command execution:

```rust
use moho_ui::Console;

let mut console = Console::new();

// Render console
console.render(ctx, event_bus);

// Console publishes ConsoleAction events:
// - SetTimeOfDay(f32)
// - SaveRequested
// - LoadRequested
// - ExitRequested
```

Available commands:
- `time <0-24>` - Set time of day
- `save` - Save game state
- `load` - Load game state
- `exit` / `quit` - Exit application
- `clear` - Clear console output

## HUD Overlay

In-game information display:

```rust
use moho_ui::Hud;

let mut hud = Hud::new();

// Render HUD
hud.render(ctx, clock, fps);

// Displays:
// - Current time of day (HH:MM)
// - FPS counter
```

## Important Integration Notes

### Staging Belt Lifecycle

The egui adapter uses `wgpu::util::StagingBelt` for GPU uploads. **You must call `recall_staging_belt()` after `queue.submit()`**:

```rust
// Render frame
adapter.render_frame(&mut ui_manager, &game_state)?;

// Submit GPU work
queue.submit(commands);

// IMPORTANT: Recall staging belt to prevent memory leak
adapter.recall_staging_belt();
```

Failing to call `recall_staging_belt()` causes GPU memory to accumulate.

### Coordinate Spaces

egui uses logical points, not physical pixels. When handling window events:

```rust
// Convert physical pixels to logical points
let scale_factor = window.scale_factor();
let logical_x = physical_x / scale_factor;
let logical_y = physical_y / scale_factor;
```

The adapter handles this automatically for winit events.

### egui Version Compatibility

This crate uses:
- `egui = "0.29"`
- `egui-wgpu = "0.29"`
- `egui-winit = "0.29"`

If updating egui versions, verify initialization APIs haven't changed. egui historically changes constructor signatures between minor versions.

## Testing

```bash
# Run all tests
cargo test --package moho_ui

# Run specific test suites
cargo test --test start_menu
cargo test --test menu_system
cargo test --test console_rendering
```

**Test Coverage:**
- Settings menu unit tests (keybind capture logic)
- Menu system integration tests
- Start menu tests
- Console rendering tests

## Thread Safety

- **`EguiAdapter`**: Not `Send` (egui/winit require main thread)
- **`UiStateManager`**: Not `Send` (manages egui state)
- **`Prefs`**: `Send + Sync` (can load/save on worker threads)

Always create and use UI components on the main thread with the winit event loop.

## Performance Characteristics

- **Frame Overhead**: <1ms for typical UI rendering
- **Settings Load/Save**: <5ms for prefs.ini I/O
- **Memory**: ~1-2MB for egui context and textures
- **GPU Memory**: ~10-20MB for fonts and UI textures

## Known Limitations

### egui Slider Width

`egui::Slider` does not respect `ui.add_sized()` for width. To control slider width:

```rust
ui.spacing_mut().slider_width = desired_width - 60.0; // Account for value display
ui.add(egui::Slider::new(&mut value, range));
```

The 60px subtraction accounts for egui's value display box and padding.

### Fast Clicks

egui may miss very fast clicks (press+release between frames). The adapter includes fallback hit-testing using stored `Response.rect` positions to catch these cases.

## Future Plans

- Settings import/export
- Gamepad UI navigation
- Localization support
- Theme customization
- More overlay types (inventory, crafting, etc.)
- Accessibility improvements (screen reader, high contrast)
