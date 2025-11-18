# Game State Management Architecture

## Status: Production Ready (85% Complete)

**Last Updated**: November 17, 2025

This document describes the game state management system for the Moho engine. The core architecture has been implemented and is actively used in production. This document focuses on **optional polish work** and **future extensions**.

## Current Implementation Summary

The game state system provides:
- ✅ **GameState enum** with four states (Menu, Playing, ConsoleOpen, Paused)
- ✅ **Validated state transitions** preventing invalid transitions
- ✅ **InputRouter** with priority-based layer system
- ✅ **StateTransitionCoordinator** eliminating transition boilerplate
- ✅ **State-based rendering** in UI adapter
- ✅ **Console integration** (backtick toggles Playing ↔ ConsoleOpen)
- ✅ **Comprehensive test coverage** (23 unit tests)

**Files**:
- `moho_types/src/app_state.rs` - GameState enum with transition logic
- `moho_types/src/state_coordinator.rs` - StateTransitionActions and coordinator
- `src/game_state.rs` - Re-exports GameState
- `src/input_routing.rs` - InputRouter and InputLayer system
- `src/main.rs` - App integration with enter_console(), exit_console(), apply_transition()
- `moho_ui/src/adapter/rendering.rs` - State-based rendering

## Architecture Overview

### Core Design Principles

1. **Single Source of Truth**: GameState enum defines application mode
2. **Explicit Transitions**: Validated state changes with clear side effects
3. **Predictable Input Routing**: Layer-based priority system
4. **Immediate State Changes**: All state changes on main thread
5. **Extensible**: Easy to add new states

5. **Extensible**: Easy to add new states

### Current State Machine

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum GameState {
    Menu,          // Main menu, game not running
    Playing,       // Active gameplay, simulation running
    ConsoleOpen,   // Debug console over frozen game
    Paused,        // Pause menu over frozen game
}
```

**Valid Transitions**:
- Menu → Playing (start game)
- Playing → ConsoleOpen, Paused, Menu
- ConsoleOpen → Playing, Menu
- Paused → Playing, Menu

### Input Layer System

```rust
pub enum InputLayer {
    Modal = 100,   // Highest priority, blocks all
    Console = 50,
    Menu = 40,
    GameUI = 30,
    Game = 10,     // Lowest priority
}
```

**Active Layers by State**:
- Menu: `[Menu]`
- Playing: `[GameUI, Game]`
- ConsoleOpen: `[Console]`
- Paused: `[Menu]`

### Hybrid Architecture Note

The current implementation uses a **pragmatic hybrid approach**:
- **InputRouter** tracks which layers should be active based on GameState
- **InputDispatcher** handles actual event dispatch to UI and game systems
- InputRouter's `dispatch()` method exists but is not used in production

This works well because:
- ✅ Clear layer tracking based on state
- ✅ Existing InputDispatcher is proven and stable
- ✅ No disruption to working input system
- ✅ Can integrate fully later if needed

---

## Outstanding Work

### Optional: Full InputRouter Integration

**Current State**: InputRouter tracks layers, InputDispatcher handles events  
**Goal**: Use InputRouter.dispatch() directly for all input routing  
**Effort**: 3-5 SP  
**Priority**: Low - current system works well

**Benefits**:
- Single input routing system (eliminate InputDispatcher)
- Cleaner architecture with one dispatch path
- Full use of layer-based priority system

**Approach**:
1. Register handlers in InputRouter for each layer:
   - Modal → Modal dialog input
   - Console → Console text input
   - Menu → Menu navigation
   - GameUI → HUD interactions
   - Game → Player controls
2. Replace InputDispatcher calls with `input_router.dispatch(event)`
3. Remove InputDispatcher code
4. Update tests

**Risks**:
- Regression in input handling
- Need to test all input paths thoroughly
- Console and menu input may need refactoring

**Files to Modify**:
- `src/main.rs` - Replace InputDispatcher with InputRouter
- `moho_ui/src/input_handling.rs` - May need layer-aware handling
- `src/input_dispatcher.rs` - Remove or deprecate

---

### Optional: Integration Tests

**Current State**: 23 unit tests, no integration tests  
**Goal**: End-to-end tests for state transitions  
**Effort**: 2-3 SP  
**Priority**: Low - core functionality well-tested

**Test Scenarios**:
1. Menu → Playing → ConsoleOpen → Playing (full cycle)
2. Console toggle doesn't interfere with menu
3. Pause menu shows correct UI
4. Invalid transitions are rejected
5. Cursor state changes correctly
6. Simulation pauses/resumes appropriately

**Approach**:
- Create `tests/state_transitions.rs`
- Use AppState for testable state representation
- Mock window/input events
- Verify state changes and side effects

---

### Optional: Documentation Cleanup

**Current State**: This document describes implementation plan  
**Goal**: Update to reflect actual implementation  
**Effort**: 1 SP  
**Priority**: Medium - helps future maintenance

**Tasks**:
- ✅ Rewrite to focus on outstanding work (this update)
- Document hybrid InputRouter/InputDispatcher approach
- Add architecture diagrams if helpful
- Update code examples to match current implementation
- Add troubleshooting section

---

### Optional: Code Cleanup

**Goal**: Remove deprecated code and warnings  
**Effort**: 1-2 SP  
**Priority**: Medium

**Items**:
1. Check for old `AppMode` references (likely removed already)
2. Remove atomic visibility flags if no longer used
3. Fix `#[allow(dead_code)]` warnings in input_routing.rs
4. Consider removing InputRouter.dispatch() if never to be used
5. Consolidate GameState definitions (currently in moho_types and moho_ui)

---

## Future Extensions

The architecture makes it easy to add new states:

### Loading State
```rust
GameState::Loading // World generation, level loading
```
- Show loading screen
- Optional progress bar
- Cancel button returns to menu

### Multiplayer Lobby
```rust
GameState::Lobby // Network game setup
```
- Player list UI
- Ready/not ready states
- Host controls (kick, start)
- Input goes to lobby UI only

### Inventory/Dialog States
```rust
GameState::Inventory  // Paused with inventory UI
GameState::Dialog     // Conversation with NPC
```
- Game visible but frozen
- Specialized input handling
- Return to Playing when closed

### Photo Mode
```rust
GameState::PhotoMode  // Free camera, no HUD
```
- Frozen game world
- Free camera controls
- Screenshot utilities
- Return to Playing

**Adding a New State**:
1. Add variant to GameState enum
2. Define valid transitions in `can_transition_to()`
3. Add rendering case in `moho_ui/src/adapter/rendering.rs`
4. Define active input layers in `InputRouter::update_for_state()`
5. Add transition helpers to StateTransitionCoordinator if needed
6. Write tests

---

## Known Limitations

1. **No State History**: Can't go "back" to previous state automatically
   - Could add: State stack for back navigation
   - Use case: Pause → Settings → Controls → (back) → Settings

2. **No Substates**: Each state is flat
   - Could add: Menu(MenuScreen), Playing(PlayMode), etc.
   - Use case: Different menu screens, different game modes

3. **Single Active State**: Can't be in multiple states
   - Current design: One state at a time
   - Alternative: Bitflags for state composition
   - May not be needed - layers handle composition

4. **InputRouter Not Fully Utilized**: Hybrid approach leaves dispatch unused
   - See "Full InputRouter Integration" section above

---

## Reference: Key Implementation Details

### State Transition Pattern

All transitions use this pattern:

```rust
// In src/main.rs
fn enter_console(&mut self) {
    match StateTransitionCoordinator::enter_console(self.game_state) {
        Ok(actions) => self.apply_transition(actions),
        Err(e) => log::warn!("{}", e),
    }
}

fn apply_transition(&mut self, actions: StateTransitionActions) {
    // Update state
    self.game_state = actions.new_state;
    self.input_router.update_for_state(actions.new_state);
    
    // Update UI
    self.ui_adapter.set_visible(actions.ui_visible);
    self.ui_adapter.set_game_state(actions.new_state);
    
    // Update cursor
    if actions.cursor_grabbed {
        self.grab_cursor();
    } else {
        self.release_cursor();
    }
    
    // Show menu if requested
    if let Some(menu) = actions.show_menu {
        self.ui_adapter.show_menu(menu);
    }
}
```

### State-Based Rendering

```rust
// In moho_ui/src/adapter/rendering.rs
pub fn render_game_state(ctx: &egui::Context, state: GameState, ...) {
    match state {
        GameState::Menu => render_menu(ctx, ui_state),
        GameState::ConsoleOpen => render_console_overlay(ctx, ui_state),
        GameState::Paused => render_pause_menu(ctx, ui_state),
        GameState::Playing => {
            // Minimal or no UI rendering
            // Game world renders separately
        }
    }
}
```

### Testing Pattern

```rust
#[test]
fn test_transition() {
    let actions = StateTransitionCoordinator::enter_console(
        GameState::Playing
    ).unwrap();
    
    assert_eq!(actions.new_state, GameState::ConsoleOpen);
    assert_eq!(actions.ui_visible, true);
    assert_eq!(actions.cursor_grabbed, false);
}
```

---

## Maintenance Notes

**When Adding Features**:
- If adding UI that needs input → Consider new InputLayer
- If adding mode that changes behavior → Consider new GameState
- If adding transition → Update StateTransitionCoordinator

**Common Pitfalls**:
- Don't bypass transition validation - always use StateTransitionCoordinator
- Don't forget to update input_router when state changes
- Don't mix state checks with direct visibility flags

**Performance**:
- GameState is Copy - cheap to pass around
- State transitions are rare (user-initiated)
- Input routing is per-event but optimized (sorted layers)

---

## Related Documentation

- [Console Architecture](CONSOLE_ARCHITECTURE.md) - Console overlay implementation
- [Quick Reference](QUICK_REFERENCE.md) - Common patterns and examples
- See `src/input_routing.rs` for detailed InputRouter documentation
- See `moho_types/src/app_state.rs` for GameState implementation
- See `moho_types/src/state_coordinator.rs` for transition logic

---

**Status**: Production ready with optional polish work available  
**Maintainer**: Moho Development Team  
**Last Review**: November 17, 2025
