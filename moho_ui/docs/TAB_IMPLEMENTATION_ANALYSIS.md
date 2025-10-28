# Tab Implementation Analysis

## Context

We want to add tabbed navigation to UI screens (like Settings) to organize controls into logical groups without creating separate screens for each category.

## Requirements

- **Organization**: Group related settings (Controls, Audio, Graphics, etc.)
- **Reusability**: Avoid duplicating layout/button logic across multiple screens
- **Modularity**: Keep tab content organized in separate files
- **State Management**: Handle dirty state, validation across tabs
- **User Experience**: Clean, responsive tab UI that fits our design language

## Approach Comparison

### Option 1: "Fake Tabs" (Button-Based Screen Switching)

**Implementation**: Style buttons as tabs, create separate `Screen` for each tab.

```rust
// Multiple screens
ControlsSettingsMenu  // Inherits from Screen
AudioSettingsMenu     // Inherits from Screen  
GraphicsSettingsMenu  // Inherits from Screen
```

**Pros:**
- ✅ Simple - uses existing Screen infrastructure
- ✅ Clean separation - each tab is independent
- ✅ No new abstractions needed

**Cons:**
- ❌ Duplicates top/bottom panels across screens
- ❌ Duplicates Save/Cancel/Back button logic
- ❌ State synchronization nightmare (dirty flags, staged values, etc.)
- ❌ Can't share validation logic easily
- ❌ Multiple Screen instances to manage
- ❌ Navigation history gets complex

**Verdict:** ❌ **Not Recommended** - Too much duplication and state management overhead.

---

### Option 2: "Real Tabs" (Component-Based Tab Groups)

**Implementation**: Single screen with tab state, render different content based on active tab.

```rust
pub struct SettingsMenu {
    active_tab: SettingsTab,
    // Existing state remains unified
    prefs: Prefs,
    staged: Prefs,
    dirty_fields: HashSet<SettingsField>,
    // ...
}

#[derive(Clone, Copy, PartialEq)]
enum SettingsTab {
    Controls,
    Audio,
    Graphics,
}
```

**Pros:**
- ✅ Single source of truth for state
- ✅ Shared Save/Cancel/Back logic
- ✅ Can modularize tab content into functions/files
- ✅ Easy to implement tab-specific validation
- ✅ Maintains navigation simplicity
- ✅ Professional UX pattern

**Cons:**
- ⚠️ Requires new tab UI component
- ⚠️ Tab content render functions can get large (solvable with modules)

**Verdict:** ✅ **Recommended** - Best balance of maintainability and UX.

---

### Option 3: "Hybrid Tabs" (Tab Bar + Sub-Components)

**Implementation**: Tab rendering system with trait-based tab content providers.

```rust
trait TabContent {
    fn render(&mut self, ui: &mut egui::Ui, prefs: &mut Prefs);
    fn is_dirty(&self) -> bool;
}

struct ControlsTab { /* ... */ }
impl TabContent for ControlsTab { /* ... */ }

struct AudioTab { /* ... */ }
impl TabContent for AudioTab { /* ... */ }
```

**Pros:**
- ✅ Highly modular - each tab is independent trait impl
- ✅ Easy to test tab content in isolation
- ✅ Can add/remove tabs without touching main screen logic
- ✅ Shared state still unified in parent screen

**Cons:**
- ⚠️ More complex architecture
- ⚠️ Requires careful trait design for state access
- ⚠️ May be overkill for simple settings screens

**Verdict:** ⚠️ **Consider for Future** - Great for complex apps with many tabs, but adds complexity upfront.

---

## Recommended Implementation: Option 2 (Real Tabs)

### Phase 1: Add Tab Infrastructure to FormControls

Create reusable tab bar component:

```rust
impl FormControls {
    /// Renders a horizontal tab bar and returns the index of the selected tab.
    ///
    /// # Arguments
    /// * `ui` - The egui UI context
    /// * `tabs` - Array of tab labels
    /// * `active_index` - Currently active tab index
    ///
    /// # Returns
    /// The index of the newly selected tab (may be same as active_index)
    pub fn tab_bar(
        ui: &mut egui::Ui, 
        tabs: &[&str], 
        active_index: usize
    ) -> usize {
        let mut selected = active_index;
        
        ui.horizontal(|ui| {
            for (i, &label) in tabs.iter().enumerate() {
                let is_active = i == active_index;
                
                // Style as tab button
                let button = if is_active {
                    egui::Button::new(label)
                        .fill(egui::Color32::from_rgb(60, 60, 60))
                } else {
                    egui::Button::new(label)
                        .fill(egui::Color32::from_rgb(40, 40, 40))
                };
                
                if ui.add(button).clicked() {
                    selected = i;
                }
            }
        });
        
        selected
    }
}
```

### Phase 2: Refactor Settings Screen Structure

**File Organization:**
```
moho_ui/src/screens/
├── settings/
│   ├── mod.rs           # Main SettingsMenu struct + render coordination
│   ├── controls_tab.rs  # render_controls_tab() function
│   ├── audio_tab.rs     # render_audio_tab() function
│   └── types.rs         # SettingsTab enum, shared types
└── settings.rs          # (move to settings/mod.rs)
```

**Main Structure:**
```rust
// settings/mod.rs
mod controls_tab;
mod audio_tab;
mod types;

use types::SettingsTab;

pub struct SettingsMenu {
    active_tab: SettingsTab,
    // ... existing fields remain the same
}

impl UiComponent for SettingsMenu {
    fn render(&mut self, ctx: &egui::Context) -> Vec<MenuItem> {
        // Top panel: title + tab bar
        egui::TopBottomPanel::top("settings_top").show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(16.0);
                ui.heading("Game Settings");
                ui.add_space(8.0);
                
                // Tab bar
                let tabs = ["Controls", "Audio", "Graphics"];
                let new_tab = FormControls::tab_bar(ui, &tabs, self.active_tab as usize);
                self.active_tab = SettingsTab::from_index(new_tab);
                
                ui.add_space(12.0);
            });
        });
        
        // Bottom panel: shared Save/Cancel/Back buttons (unchanged)
        // ...
        
        // Central panel: render active tab content
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.active_tab {
                SettingsTab::Controls => controls_tab::render(self, ui),
                SettingsTab::Audio => audio_tab::render(self, ui),
                // ... more tabs
            }
        });
    }
}
```

**Tab Content Module:**
```rust
// settings/controls_tab.rs
use super::SettingsMenu;

pub fn render(menu: &mut SettingsMenu, ui: &mut egui::Ui) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        // All the keybind controls, mouse sensitivity, etc.
        // Direct access to menu.staged, menu.dirty_fields, etc.
    });
}
```

### Benefits of This Approach

1. **Single State**: All tabs share the same `prefs`, `staged`, `dirty_fields`
2. **Shared Logic**: One set of Save/Cancel/Back buttons
3. **Modular Content**: Tab rendering split into focused files
4. **Easy to Extend**: Add new tab = add enum variant + render function
5. **Clean UX**: Professional tabbed interface
6. **Testable**: Each tab render function can be unit tested

### Migration Path

1. ✅ Add `tab_bar()` to FormControls
2. ✅ Add `SettingsTab` enum to settings module
3. ✅ Add `active_tab` field to SettingsMenu
4. ✅ Move existing controls to `controls_tab::render()`
5. ✅ Move existing audio to `audio_tab::render()`
6. ✅ Update main render() to call tab functions
7. ✅ (Optional) Refactor into settings/ subdirectory if needed

---

## Alternative: egui Built-in Solutions

**Does egui have tabs?** Not directly, but it provides:
- `egui::TopBottomPanel` - what we're already using
- `egui::CollapsingHeader` - accordion-style (not ideal for settings)
- Custom buttons styled as tabs - what we'd implement

**Recommendation**: Build our own tab system using Option 2. It gives us:
- Full control over styling
- Better integration with our FormControls pattern
- Reusable across other screens (New World, Graphics Settings, etc.)

---

## Implementation Estimate

**Story Points: 3** (Small-Medium)

**Tasks:**
1. Add `tab_bar()` helper to FormControls (~30 min)
2. Add SettingsTab enum and active_tab field (~15 min)
3. Refactor existing settings content into tab functions (~1 hour)
4. Test tab switching, ensure state persists (~30 min)
5. Visual polish on tab styling (~30 min)

**Total Estimated Effort:** ~2.5-3 hours

**Risk:** Low - purely additive, doesn't break existing functionality

---

## Future Enhancements

Once tab infrastructure is in place:
- Graphics tab (when renderer settings are exposed)
- Gameplay tab (difficulty, accessibility options)
- Network/Multiplayer tab
- Tab-specific dirty indicators (show * on tab with changes)
- Keyboard navigation between tabs (Ctrl+Tab)
- Remember last active tab in preferences
