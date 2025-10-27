# Phase 3: Overlay Support Implementation

## Overview

Phase 3 adds support for **in-game overlays** - non-modal UI elements that appear during gameplay without blocking the game. This includes HUD elements, notifications, in-game menus, and other interactive UI that coexists with the game world.

**Status:** Not Started (Phase 1 & 2 Complete)  
**Estimated Effort:** 2-3 days  
**Prerequisites:** Phase 1 & 2 completed ✅

---

## Goals

1. **Define Overlay trait** similar to Screen but with different characteristics
2. **Implement overlay rendering** that coexists with game rendering
3. **Handle overlay input** with proper pass-through to game when needed
4. **Create example overlays** to validate the system
5. **Simplify action system** by renaming MenuItem → UiAction

---

## Task Breakdown

### Task 1: Define Overlay Trait (1-2 hours)

**File:** `moho_ui/src/overlays/overlay.rs` (new)

Create the base trait for overlay components:

```rust
/// Trait for non-modal UI elements that appear during gameplay
pub trait Overlay: UiComponent {
    /// Opacity level (0.0 = fully transparent, 1.0 = fully opaque)
    fn opacity(&self) -> f32 {
        1.0
    }
    
    /// Whether this overlay blocks input from reaching the game
    /// Return true for interactive menus, false for HUD/notifications
    fn blocks_input(&self) -> bool {
        false
    }
    
    /// Z-order for layering (higher values render on top)
    /// Standard ranges:
    /// - 0-99: Background overlays (rarely used)
    /// - 100-199: HUD elements
    /// - 200-299: Notifications
    /// - 300-399: In-game menus
    /// - 400+: Critical alerts
    fn z_order(&self) -> i32 {
        100
    }
    
    /// Whether this overlay should be visible
    fn is_visible(&self) -> bool {
        true
    }
    
    /// Position and size configuration
    fn layout(&self) -> OverlayLayout {
        OverlayLayout::default()
    }
    
    /// Called every frame to update overlay state (animations, timers, etc.)
    fn update(&mut self, delta_time: f32) {}
}

/// Layout configuration for overlays
#[derive(Clone, Debug)]
pub struct OverlayLayout {
    pub anchor: Align2,           // Which corner/edge to anchor to
    pub offset: Vec2,              // Offset from anchor point
    pub size: OverlaySize,         // How to size the overlay
}

#[derive(Clone, Debug)]
pub enum OverlaySize {
    Fixed { width: f32, height: f32 },
    Flexible,                      // Size based on content
    FullScreen,
}
```

**Implementation Notes:**
- Overlays inherit from `UiComponent` (base trait)
- Different from `Screen` which is always full-screen and modal
- Support transparency and layering
- Can optionally block input or allow pass-through

**Testing:**
- Create simple test overlay
- Verify trait methods with default implementations

---

### Task 2: Create Example Overlays (2-3 hours)

#### 2a. HUD Overlay (`moho_ui/src/overlays/hud.rs`)

Basic heads-up display overlay:

```rust
pub struct HudOverlay {
    health: u32,
    max_health: u32,
    score: u64,
    visible: bool,
}

impl HudOverlay {
    pub fn new() -> Self {
        Self {
            health: 100,
            max_health: 100,
            score: 0,
            visible: true,
        }
    }
    
    pub fn set_health(&mut self, health: u32) {
        self.health = health;
    }
    
    pub fn add_score(&mut self, points: u64) {
        self.score += points;
    }
}

impl UiComponent for HudOverlay {
    fn name(&self) -> &str { "hud" }
    
    fn render(&mut self, ctx: &egui::Context) -> Vec<MenuItem> {
        // Render health bar, score, etc.
        egui::Area::new("hud_area")
            .anchor(Align2::LEFT_TOP, egui::vec2(10.0, 10.0))
            .show(ctx, |ui| {
                ui.label(format!("Health: {}/{}", self.health, self.max_health));
                ui.label(format!("Score: {}", self.score));
            });
        
        Vec::new() // HUD doesn't produce actions
    }
    
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn as_any(&self) -> &dyn Any { self }
}

impl Overlay for HudOverlay {
    fn blocks_input(&self) -> bool { false } // HUD is non-interactive
    fn z_order(&self) -> i32 { 100 }
    fn is_visible(&self) -> bool { self.visible }
}
```

#### 2b. Notification Overlay (`moho_ui/src/overlays/notification.rs`)

Temporary message overlay with auto-dismiss:

```rust
pub struct NotificationOverlay {
    message: String,
    duration: f32,
    elapsed: f32,
    notification_type: NotificationType,
}

#[derive(Clone, Copy)]
pub enum NotificationType {
    Info,
    Success,
    Warning,
    Error,
}

impl NotificationOverlay {
    pub fn new(message: String, duration: f32, notification_type: NotificationType) -> Self {
        Self {
            message,
            duration,
            elapsed: 0.0,
            notification_type,
        }
    }
}

impl UiComponent for NotificationOverlay {
    fn name(&self) -> &str { "notification" }
    
    fn render(&mut self, ctx: &egui::Context) -> Vec<MenuItem> {
        let color = match self.notification_type {
            NotificationType::Info => egui::Color32::BLUE,
            NotificationType::Success => egui::Color32::GREEN,
            NotificationType::Warning => egui::Color32::YELLOW,
            NotificationType::Error => egui::Color32::RED,
        };
        
        egui::Area::new("notification_area")
            .anchor(Align2::CENTER_TOP, egui::vec2(0.0, 50.0))
            .show(ctx, |ui| {
                ui.colored_label(color, &self.message);
            });
        
        Vec::new()
    }
    
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn as_any(&self) -> &dyn Any { self }
}

impl Overlay for NotificationOverlay {
    fn blocks_input(&self) -> bool { false }
    fn z_order(&self) -> i32 { 200 }
    fn is_visible(&self) -> bool { self.elapsed < self.duration }
    
    fn update(&mut self, delta_time: f32) {
        self.elapsed += delta_time;
    }
}
```

#### 2c. Pause Menu Overlay (`moho_ui/src/overlays/pause_menu.rs`)

In-game menu that blocks input:

```rust
pub struct PauseMenuOverlay {
    visible: bool,
}

impl PauseMenuOverlay {
    pub fn new() -> Self {
        Self { visible: false }
    }
    
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }
}

impl UiComponent for PauseMenuOverlay {
    fn name(&self) -> &str { "pause_menu" }
    
    fn render(&mut self, ctx: &egui::Context) -> Vec<MenuItem> {
        let mut items = Vec::new();
        
        egui::Area::new("pause_menu_area")
            .anchor(Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.heading("PAUSED");
                    ui.add_space(20.0);
                    
                    if ui.button("Resume").clicked() {
                        items.push(MenuItem {
                            action: MenuAction::Close,
                            rect: None,
                            enabled: true,
                        });
                    }
                    
                    if ui.button("Settings").clicked() {
                        items.push(MenuItem {
                            action: MenuAction::ShowMenu("settings".to_string()),
                            rect: None,
                            enabled: true,
                        });
                    }
                    
                    if ui.button("Exit to Menu").clicked() {
                        items.push(MenuItem {
                            action: MenuAction::ShowMenu("start".to_string()),
                            rect: None,
                            enabled: true,
                        });
                    }
                });
            });
        
        items
    }
    
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    fn as_any(&self) -> &dyn Any { self }
}

impl Overlay for PauseMenuOverlay {
    fn blocks_input(&self) -> bool { true } // Pause menu blocks game input
    fn z_order(&self) -> i32 { 300 }
    fn opacity(&self) -> f32 { 0.95 }
    fn is_visible(&self) -> bool { self.visible }
}
```

---

### Task 3: Overlay Manager (2-3 hours)

**File:** `moho_ui/src/overlays/overlay_manager.rs` (new)

Create a manager to handle multiple overlays:

```rust
pub struct OverlayManager {
    overlays: Vec<Box<dyn Overlay>>,
}

impl OverlayManager {
    pub fn new() -> Self {
        Self {
            overlays: Vec::new(),
        }
    }
    
    /// Add an overlay to the stack
    pub fn add(&mut self, overlay: Box<dyn Overlay>) {
        self.overlays.push(overlay);
        self.sort_by_z_order();
    }
    
    /// Remove overlay by name
    pub fn remove(&mut self, name: &str) {
        self.overlays.retain(|o| o.name() != name);
    }
    
    /// Get mutable reference to overlay by name
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Box<dyn Overlay>> {
        self.overlays.iter_mut().find(|o| o.name() == name)
    }
    
    /// Update all overlays (for animations, timers, etc.)
    pub fn update(&mut self, delta_time: f32) {
        for overlay in &mut self.overlays {
            overlay.update(delta_time);
        }
        
        // Remove expired overlays (like notifications)
        self.overlays.retain(|o| o.is_visible());
    }
    
    /// Render all visible overlays in z-order
    pub fn render(&mut self, ctx: &egui::Context) -> Vec<MenuItem> {
        let mut all_items = Vec::new();
        
        for overlay in &mut self.overlays {
            if overlay.is_visible() {
                let items = overlay.render(ctx);
                all_items.extend(items);
            }
        }
        
        all_items
    }
    
    /// Check if any overlay blocks input
    pub fn blocks_input(&self) -> bool {
        self.overlays.iter()
            .any(|o| o.is_visible() && o.blocks_input())
    }
    
    fn sort_by_z_order(&mut self) {
        self.overlays.sort_by_key(|o| o.z_order());
    }
}
```

---

### Task 4: Update Adapter for Overlays (2-3 hours)

**File:** `moho_ui/src/adapter.rs`

Add overlay support to the adapter:

```rust
pub struct EguiAdapter {
    // Existing fields...
    ui_state: UiStateManager,
    progress: Option<ProgressState>,
    
    // NEW: Overlay manager
    overlay_manager: OverlayManager,
    
    // Existing egui fields...
    egui_ctx: egui::Context,
    egui_winit: egui_winit::State,
    renderer: egui_wgpu::Renderer,
}

impl EguiAdapter {
    pub fn new(window: Option<Arc<Window>>) -> (Self, UiReceiver) {
        // ... existing initialization ...
        
        Self {
            ui_state: UiStateManager::new(),
            overlay_manager: OverlayManager::new(), // NEW
            // ... rest of fields ...
        }
    }
    
    /// Add an overlay
    pub fn add_overlay(&mut self, overlay: Box<dyn Overlay>) {
        self.overlay_manager.add(overlay);
    }
    
    /// Remove an overlay
    pub fn remove_overlay(&mut self, name: &str) {
        self.overlay_manager.remove(name);
    }
    
    /// Get mutable reference to overlay
    pub fn get_overlay_mut(&mut self, name: &str) -> Option<&mut Box<dyn Overlay>> {
        self.overlay_manager.get_mut(name)
    }
    
    /// Check if input should be forwarded to game
    pub fn should_forward_input(&self) -> bool {
        // Block input if:
        // - Screen is visible (modal UI)
        // - Any overlay blocks input
        if self.ui_state.visible {
            return false;
        }
        
        if self.overlay_manager.blocks_input() {
            return false;
        }
        
        true
    }
}

impl FrameCallback for EguiAdapter {
    fn call(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, /* ... */) {
        // ... existing setup ...
        
        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            let mut menu_actions = Vec::new();
            
            // 1. Render active screen (if visible)
            if self.ui_state.visible {
                if let Some(screen) = self.ui_state.active_screen_mut() {
                    menu_actions.extend(screen.render(ctx));
                }
            }
            
            // 2. Render overlays (NEW - always rendered when visible)
            let overlay_actions = self.overlay_manager.render(ctx);
            menu_actions.extend(overlay_actions);
            
            // 3. Handle modals
            // ... existing modal code ...
            
            // 4. Handle progress overlay
            // ... existing progress code ...
            
            menu_actions
        });
        
        // ... rest of rendering ...
    }
}
```

**Key Changes:**
- Add `overlay_manager: OverlayManager` field
- Add `add_overlay()`, `remove_overlay()`, `get_overlay_mut()` methods
- Update `should_forward_input()` to check overlay blocking
- Render overlays after screens in frame callback
- Call `overlay_manager.update(delta_time)` in frame callback

---

### Task 5: Update Main Application (1 hour)

**File:** `src/main.rs`

Update the main application to use overlays:

```rust
// In MohoApp initialization
fn init_ui(window: &Window) -> Arc<Mutex<EguiAdapter>> {
    let (mut adapter, _receiver) = EguiAdapter::new(Some(window.clone()));
    
    // Initialize overlays
    adapter.add_overlay(Box::new(HudOverlay::new()));
    
    Arc::new(Mutex::new(adapter))
}

// In event loop - handle input forwarding
fn handle_input_event(&mut self, event: &WindowEvent) {
    if let Some(adapter) = &self.ui_adapter {
        let should_forward = adapter.lock()
            .map(|a| a.should_forward_input())
            .unwrap_or(false);
        
        if !should_forward {
            return; // UI is blocking, don't forward to game
        }
    }
    
    // Forward to game...
}

// Add pause menu toggle
fn handle_pause_key(&mut self) {
    if let Some(adapter) = &self.ui_adapter {
        if let Ok(mut adapter) = adapter.lock() {
            if let Some(pause_menu) = adapter.get_overlay_mut("pause_menu") {
                if let Some(menu) = pause_menu.as_any_mut().downcast_mut::<PauseMenuOverlay>() {
                    menu.toggle();
                }
            }
        }
    }
}
```

---

### Task 6: Simplify MenuItem → UiAction (2-3 hours)

**Rationale:** `MenuItem` is now used by both screens and overlays, making the name misleading. Rename to `UiAction` for clarity.

**Files to Update:**
- `moho_ui/src/screens/menu.rs` - Rename type
- All screen implementations
- All overlay implementations
- `moho_ui/src/adapter.rs`
- Test files

**Changes:**
```rust
// Before
pub struct MenuItem {
    pub action: MenuAction,
    pub rect: Option<egui::Rect>,
    pub enabled: bool,
}

// After
pub struct UiAction {
    pub action: MenuAction,
    pub rect: Option<egui::Rect>,
    pub enabled: bool,
}

// Update UiComponent trait
pub trait UiComponent {
    fn render(&mut self, ctx: &egui::Context) -> Vec<UiAction>;
    // ... rest unchanged
}
```

**Migration Steps:**
1. Rename `MenuItem` → `UiAction` in `screens/menu.rs`
2. Find/replace `MenuItem` → `UiAction` across codebase
3. Update all trait implementations
4. Update tests
5. Update public exports in `lib.rs`

---

## Module Structure After Phase 3

```
moho_ui/src/
├── screens/                    # Full-screen modal UI (Phase 1 & 2)
│   ├── mod.rs
│   ├── menu.rs                 # Screen trait, UiComponent, UiAction (renamed)
│   ├── start.rs
│   ├── settings.rs
│   ├── new_world.rs
│   └── form_controls.rs
├── overlays/                   # In-game overlays (Phase 3)
│   ├── mod.rs
│   ├── overlay.rs              # Overlay trait (NEW)
│   ├── overlay_manager.rs      # Overlay management (NEW)
│   ├── hud.rs                  # HUD overlay (NEW)
│   ├── notification.rs         # Notification overlay (NEW)
│   └── pause_menu.rs           # Pause menu overlay (NEW)
├── adapter.rs                  # Updated with overlay support
├── ui_state.rs                 # Screen state management
├── modal.rs
├── modals/
├── input_handling.rs
└── prefs.rs
```

---

## Testing Plan

### Unit Tests

**File:** `moho_ui/tests/overlay_system.rs` (new)

```rust
#[test]
fn overlay_z_order_sorting() {
    let mut manager = OverlayManager::new();
    manager.add(Box::new(HudOverlay::new()));           // z=100
    manager.add(Box::new(NotificationOverlay::new())); // z=200
    manager.add(Box::new(PauseMenuOverlay::new()));    // z=300
    
    // Verify overlays render in correct order
}

#[test]
fn overlay_input_blocking() {
    let hud = HudOverlay::new();
    assert!(!hud.blocks_input());
    
    let pause = PauseMenuOverlay::new();
    assert!(pause.blocks_input());
}

#[test]
fn notification_auto_dismiss() {
    let mut notif = NotificationOverlay::new(
        "Test".to_string(),
        1.0,
        NotificationType::Info
    );
    
    assert!(notif.is_visible());
    notif.update(0.5);
    assert!(notif.is_visible());
    notif.update(0.6);
    assert!(!notif.is_visible());
}
```

### Integration Tests

1. **Overlay Rendering**
   - Verify overlays render in z-order
   - Verify visibility toggling works
   - Verify opacity is applied

2. **Input Handling**
   - HUD doesn't block input
   - Pause menu blocks input
   - Input forwarding works correctly

3. **Overlay Management**
   - Add/remove overlays
   - Update overlays each frame
   - Auto-remove expired overlays

### Manual Testing

1. **HUD Display**
   - Run game with HUD visible
   - Verify health/score display
   - Verify HUD doesn't interfere with gameplay

2. **Notifications**
   - Show notification
   - Verify auto-dismiss after duration
   - Show multiple notifications

3. **Pause Menu**
   - Press ESC to pause
   - Verify game input is blocked
   - Verify menu is interactive
   - Resume game

---

## Migration Guide

### For Existing Code

**Before Phase 3:**
```rust
// Only screens were supported
adapter.show_menu("start");
adapter.hide_menus();
```

**After Phase 3:**
```rust
// Screens still work the same
adapter.show_menu("start");
adapter.hide_menus();

// NEW: Overlays
adapter.add_overlay(Box::new(HudOverlay::new()));
adapter.remove_overlay("notification");

// Update HUD data
if let Some(hud) = adapter.get_overlay_mut("hud") {
    if let Some(hud) = hud.as_any_mut().downcast_mut::<HudOverlay>() {
        hud.set_health(75);
    }
}
```

### Breaking Changes

- `MenuItem` renamed to `UiAction` (simple find/replace)
- No other breaking changes - backward compatible!

---

## Implementation Order

1. **Day 1 Morning:** Task 1 (Define Overlay trait)
2. **Day 1 Afternoon:** Task 2a (HUD overlay)
3. **Day 2 Morning:** Task 2b-2c (Notification & Pause menu overlays)
4. **Day 2 Afternoon:** Task 3 (Overlay Manager)
5. **Day 3 Morning:** Task 4 (Update Adapter)
6. **Day 3 Afternoon:** Task 5 & 6 (Update main app, rename MenuItem)

---

## Success Criteria

- ✅ Overlay trait defined with all required methods
- ✅ At least 3 working overlay examples (HUD, notification, pause menu)
- ✅ OverlayManager handles multiple overlays correctly
- ✅ Adapter renders overlays in correct z-order
- ✅ Input blocking works correctly
- ✅ MenuItem renamed to UiAction
- ✅ All tests pass
- ✅ Documentation updated

---

## Future Enhancements (Beyond Phase 3)

Once Phase 3 is complete, consider:

- **Animation system** for overlay transitions
- **Overlay templates** for common patterns (health bars, minimaps, etc.)
- **Overlay positioning helpers** (screen edges, corners, etc.)
- **Overlay events** (onMouseEnter, onMouseLeave, etc.)
- **Persistent overlays** that survive scene changes
- **Overlay theming** separate from screen themes

---

## Notes

- Overlays and screens can coexist (e.g., pause menu overlay over settings screen)
- Overlays should be lightweight and performant (rendered every frame)
- Consider using `egui::Area` for flexible positioning
- Modal blocking should cascade: screen blocks all, overlay blocks if configured
- Z-order is critical for proper layering

---

## Questions to Resolve

1. **Delta time source:** Where does `update(delta_time)` get its time from?
   - **Answer:** Main game loop should track frame delta and pass to adapter

2. **Overlay persistence:** Should overlays survive scene changes?
   - **Answer:** HUD yes, notifications/pause menu probably not

3. **Maximum overlays:** Should there be a limit?
   - **Answer:** Start unlimited, add limit if performance issues arise

4. **Input priority:** If multiple overlays block input, which handles it?
   - **Answer:** Highest z-order blocking overlay gets priority

---

**Ready to implement? Start with Task 1!** 🚀
