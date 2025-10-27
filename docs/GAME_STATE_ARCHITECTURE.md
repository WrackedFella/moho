# Game State Management Architecture

## Overview

This document describes the architectural refactor to implement proper game state management and input routing for the Moho game engine. This replaces the previous atomic-bool based visibility tracking with a comprehensive state machine.

## Problem Statement

### Previous Architecture Issues

1. **Timing Problems**: Atomic visibility flags were checked across thread boundaries with event loop batching causing race conditions
2. **Scattered State**: Application mode tracked in multiple places (AppMode enum, console_visible atomic, ui_adapter.visible)
3. **Unclear Ownership**: Input routing logic mixed with event handling, making it hard to reason about who handles what
4. **No Pause Capability**: No clean way to pause game while keeping it visible
5. **Fragile Input Routing**: Manual checks scattered throughout event handlers

### Design Goals

1. **Single Source of Truth**: One enum that defines what the application is doing
2. **Explicit Transitions**: Clear, validated state transitions with transition handlers
3. **Predictable Input Routing**: Layer-based priority system that's easy to understand
4. **Immediate State Changes**: No cross-thread timing issues - state changes on main thread, render thread reads state
5. **Extensible**: Easy to add new states (multiplayer lobby, loading screen, etc.)

## Architecture Components

### 1. GameState Enum

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameState {
    /// Main menu is active, game world not running
    Menu,
    
    /// Actively playing, game world simulating, camera controlled
    Playing,
    
    /// Console overlay is open, game visible but paused, input goes to console
    ConsoleOpen,
    
    /// Game paused (via pause menu), world frozen, UI overlay showing
    Paused,
}
```

**Location**: `src/game_state.rs`

**Responsibilities**:
- Define all valid application states
- Provide transition validation (which states can transition to which)
- Define properties of each state (cursor mode, input routing, render mode)

**Benefits**:
- Explicit enumeration of all modes
- Compile-time checking of state handling
- Easy to add new states without breaking existing code

### 2. InputLayer Enum

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum InputLayer {
    /// Modal dialogs (highest priority)
    Modal = 100,
    
    /// Debug console
    Console = 50,
    
    /// Menu system
    Menu = 40,
    
    /// In-game UI/HUD
    GameUI = 30,
    
    /// Game world input (movement, looking, actions)
    Game = 10,
}
```

**Location**: `src/input_routing.rs`

**Responsibilities**:
- Define input handling layers with priorities
- Separate "what handles input" from "what state we're in"
- Enable multiple layers active at once (e.g., GameUI + Game)

**Benefits**:
- Clear priority ordering
- Composable input handling
- Easy to understand which layer should handle which event

### 3. InputRouter

```rust
pub struct InputRouter {
    /// Stack of active input layers (highest priority first)
    active_layers: Vec<InputLayer>,
    
    /// Handlers registered for each layer
    handlers: HashMap<InputLayer, Box<dyn Fn(&WindowEvent) -> bool>>,
}
```

**Location**: `src/input_routing.rs`

**Responsibilities**:
- Maintain stack of active input layers based on GameState
- Route WindowEvents to appropriate handlers in priority order
- Integrate with existing InputDispatcher for backwards compatibility

**Key Methods**:
```rust
// Update active layers based on new game state
fn update_for_state(&mut self, state: GameState)

// Dispatch event to layers in priority order
fn dispatch(&self, event: &WindowEvent) -> bool

// Register handler for a specific layer
fn register_handler(&mut self, layer: InputLayer, handler: impl Fn(&WindowEvent) -> bool)
```

**Benefits**:
- Centralized input routing logic
- No manual priority checks scattered in code
- Easy to debug (can log which layer handled which event)

### 4. State Transitions

**Implemented in**: `src/main.rs` (App impl)

**Transition Handlers**:
```rust
impl App {
    fn transition_to(&mut self, new_state: GameState) -> Result<(), String>
    fn enter_console(&mut self)
    fn exit_console(&mut self)
    fn enter_menu(&mut self)
    fn start_game(&mut self)
    fn toggle_pause(&mut self)
}
```

**Each Transition Handler**:
1. Validate transition is legal
2. Update `self.game_state`
3. Update input router layers
4. Update cursor grab/hide state
5. Emit transition events to EventBus
6. Update UI state if needed

**Example**:
```rust
fn enter_console(&mut self) {
    // Can only open console while playing
    if self.game_state != GameState::Playing {
        return;
    }
    
    // Transition state
    self.game_state = GameState::ConsoleOpen;
    
    // Update input routing (Console layer now active)
    self.input_router.update_for_state(GameState::ConsoleOpen);
    
    // Release cursor
    self.release_cursor();
    
    // Emit event
    self.event_bus.publish(GameEvent::ConsoleDpened);
    
    // Request redraw
    if let Some(wr) = &self.window_renderer {
        wr.window.request_redraw();
    }
}
```

### 5. State-Based Rendering

**Implemented in**: `moho_ui/src/adapter.rs` (FrameCallback::call)

```rust
fn call(&mut self, ...) {
    let game_state = /* read from shared state or passed parameter */;
    
    match game_state {
        GameState::Menu => {
            // Render active menu screen only
            self.ui_state.render_active_screen(ctx);
        }
        GameState::Playing => {
            // Game renders itself, UI adapter does nothing
            // (or renders HUD if we have game UI layer)
        }
        GameState::ConsoleOpen => {
            // Game already rendered by main loop
            // Render console overlay on top
            self.overlays.console.render(ctx);
        }
        GameState::Paused => {
            // Game already rendered (frozen frame)
            // Render pause menu overlay
            self.overlays.pause_menu.render(ctx);
        }
    }
}
```

**Benefits**:
- Clear separation of rendering paths
- No timing issues (state read directly, not across threads)
- Easy to add new rendering modes

## Data Flow

### State Change Flow

```
User Input (backtick press)
  ↓
window_event() receives KeyboardInput
  ↓
Match on keycode (Backquote)
  ↓
Call App::enter_console()
  ↓
Validate transition (Playing → ConsoleOpen)
  ↓
Update self.game_state = ConsoleOpen
  ↓
Update input_router.active_layers = [Console]
  ↓
Release cursor
  ↓
Emit GameEvent::ConsoleOpened to EventBus
  ↓
Request redraw
  ↓
RedrawRequested event arrives
  ↓
FrameCallback::call() reads self.game_state
  ↓
Match ConsoleOpen → render console overlay
  ↓
Console visible on screen ✓
```

### Input Routing Flow

```
WindowEvent arrives (KeyboardInput)
  ↓
App::window_event()
  ↓
InputRouter::dispatch()
  ↓
For each active layer (highest to lowest):
  ↓
  Call layer's handler
  ↓
  If handler returns true (consumed):
    Stop dispatching, return
  ↓
  If handler returns false (not consumed):
    Continue to next layer
  ↓
All layers passed through, event not consumed
  ↓
App handles event as fallback
```

## Implementation Plan

### Phase 1: Foundation (Tasks 1-3)
1. Create `GameState` enum with transition validation
2. Create `InputLayer` enum
3. Implement `InputRouter` with layer stack management

### Phase 2: Integration (Tasks 4-6)
4. Refactor `App` to use `GameState` instead of `AppMode`
5. Create `Console` overlay component
6. Integrate `Console` into `UiStateManager` as overlay

### Phase 3: Rendering & Input (Tasks 7-10)
7. Implement state-based rendering in `FrameCallback`
8. Add state transition handlers to `App`
9. Wire console commands to `GameEvent` publishing
10. Update input routing to use `InputRouter`

### Phase 4: Polish (Tasks 11-12)
11. Add comprehensive tests
12. Remove deprecated code and update documentation

## Migration Strategy

### Compatibility

- Keep `AppMode` initially, map it to `GameState` internally
- Gradually migrate code to use `GameState` directly
- Remove `AppMode` once all references updated

### Testing

Each phase should be testable independently:
- **Phase 1**: Unit tests for state transitions and input routing
- **Phase 2**: Integration tests for state management in `App`
- **Phase 3**: End-to-end tests for rendering and input
- **Phase 4**: Regression tests to ensure nothing broke

### Rollback Plan

Each commit should be atomic and revertable:
- Commit after each phase completion
- Tag stable points: `game-state-phase-1`, `game-state-phase-2`, etc.
- If issues arise, can revert to previous phase

## Future Extensions

This architecture makes it easy to add:

- **Loading State**: Show loading screen while generating world
- **Multiplayer Lobby**: Menu state with network UI
- **Inventory Screen**: Pause with inventory overlay
- **Dialog/NPC Interaction**: Game continues but input goes to dialog
- **Photo Mode**: Frozen game with free camera control
- **Replay Mode**: Playback with custom input handling

Each just requires:
1. Add variant to `GameState`
2. Define transition rules
3. Add rendering case
4. Add input layer if needed

## Benefits Summary

✅ **No More Timing Issues**: State changes and reads happen on same thread  
✅ **Clear Ownership**: Each state knows its cursor mode, input routing, render mode  
✅ **Easy to Test**: State machine can be tested independently  
✅ **Easy to Extend**: Adding states is straightforward  
✅ **Easy to Debug**: Can log all state transitions  
✅ **Type Safe**: Compiler ensures all states are handled  
✅ **Predictable**: Clear rules for transitions and input routing  

## Open Questions

1. **Should GameState be Copy or require methods to change it?**
   - **Recommendation**: Make it Copy for simplicity, use methods to validate transitions

2. **How to share GameState between threads?**
   - **Option A**: Arc<AtomicU8> with state encoded as integer (minimal)
   - **Option B**: Arc<RwLock<GameState>> (more Rusty, easier to extend)
   - **Recommendation**: Option B for now, optimize later if needed

3. **Should overlays be part of UiStateManager or separate?**
   - **Recommendation**: Part of UiStateManager but distinct from screens
   - Screens: Mutually exclusive (start menu XOR settings menu)
   - Overlays: Can layer (console + pause menu + modal)

4. **How to handle HUD elements that should show during gameplay?**
   - **Recommendation**: Add `GameUI` layer, separate from `Game` layer
   - HUD elements can capture input for buttons while game continues

## References

- Original issue: Console visibility timing problems
- Related: Input routing requirements
- Similar pattern: Bevy's `State<T>` system
- Inspiration: Classic game state machines (Quake, Doom, etc.)
