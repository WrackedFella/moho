```markdown
# TODO: Console Feature Remaining Tasks

## Task 10: InputRouter Full Integration (Optional Enhancement)

**Status**: Deferred - Current architecture works well

**Context**:
- The `InputRouter` was designed as a priority-based event routing system
- Currently, `InputRouter::update_for_state()` is called to maintain state consistency
- However, actual event routing uses the existing `InputDispatcher` system
- Both systems work but serve slightly different purposes:
  - `InputDispatcher`: UI-first subscription model (works well for egui integration)
  - `InputRouter`: State-based layer routing (useful for complex input scenarios)

**Potential Work** (if needed in future):
1. Decide whether to:
   - a) Keep both systems (current state - simple and working)
   - b) Migrate fully to InputRouter (more complex, state-driven)
   - c) Remove InputRouter and rely on InputDispatcher only

2. If choosing (b), would need to:
   - Register console input handler with `InputRouter::register_handler()`
   - Replace `dispatcher.dispatch()` with `input_router.dispatch()` in `window_event()`
   - Ensure egui still gets priority through Console layer
   - Test that all input scenarios work (menu, game, console, modals)

3. Benefits of full InputRouter integration:
   - Single source of truth for input routing
   - Clearer priority semantics (layer-based vs subscription order)
   - Better testability of input routing logic

4. Benefits of current hybrid approach:
   - Simpler - egui integration already works via InputDispatcher
   - Less code to maintain
   - InputRouter available for future complex scenarios

**Recommendation**: Keep current hybrid approach unless complex multi-layer input scenarios arise.

**Files Involved**:
- `src/input_routing.rs` - InputRouter implementation (tested, ready to use)
- `src/main.rs` - window_event() uses InputDispatcher
- `src/input_dispatcher.rs` - Current working solution

---

## Task 11: Integration Tests

**Status**: TODO

**Description**: Add integration tests for console functionality

**Required Tests**:

1. **Console Toggle Tests** (`tests/console_toggle.rs`):
   ```rust
   // Test backtick toggles console on/off
   // Test Escape closes console
   // Test console only opens from Playing state
   // Test state transitions are valid
   ```

2. **Console Command Tests** (`tests/console_commands.rs`):
   ```rust
   // Test quit command publishes ExitRequested event
   // Test god command publishes ToggleGodMode event
   // Test noclip command publishes ToggleCollision event
   // Test help command displays all commands
   // Test clear command empties output
   // Test unknown command shows error
   ```

3. **Event Publishing Tests** (`tests/console_events.rs`):
   ```rust
   // Test EventBus receives console command events
   // Test multiple subscribers can receive events
   // Test event data is correct
   ```

4. **State Transition Tests** (extend existing in `src/game_state.rs`):
   ```rust
   // Test Playing -> ConsoleOpen -> Playing cycle
   // Test ConsoleOpen -> Menu transition
   // Test invalid transitions are rejected
   // Test cursor state changes with transitions
   ```

5. **Rendering Tests** (`moho_ui/tests/console_rendering.rs`):
   ```rust
   // Test console renders in ConsoleOpen state
   // Test console doesn't render in Playing state
   // Test console overlay layers correctly
   // Test GameState propagates to adapter
   ```

**Implementation Priority**:
- High: Command event publishing tests (#2, #3)
- Medium: State transition integration tests (#4)
- Low: UI rendering tests (#5) - harder to test without full GPU context

**Files to Create**:
- `tests/console_toggle.rs`
- `tests/console_commands.rs`
- `tests/console_events.rs`
- `moho_ui/tests/console_rendering.rs`

---

## Task 12: Cleanup and Documentation

**Status**: TODO

**Description**: Clean up deprecated code and document the architecture

### Cleanup Items:

1. **Remove Dead Code** (if not needed for future):
   - ❓ `InputRouter::dispatch()` - Currently unused but tested and functional
   - ❓ `InputRouter::register_handler()` - Unused but may be useful later
   - ✅ Console command TODOs - Already removed (Phase 3)

2. **Verify No Atomic Visibility Patterns Remain**:
   - Check for any leftover `AtomicBool` patterns from old approach
   - Ensure all visibility is controlled via GameState
   - Files to check:
     - `src/main.rs` - Look for old console_visible patterns
     - `moho_ui/src/adapter.rs` - UI_OVERLAY_VISIBLE is still used but acceptable

3. **Code Comments Cleanup**:
   - Remove any TODO comments that reference Phase 3
   - Update doc comments to reflect current architecture
   - Add examples to public API methods

### Documentation Items:

1. **Architecture Documentation** (`docs/CONSOLE_ARCHITECTURE.md`):
   ```markdown
   # Debug Console Architecture
   
   ## Overview
   - GameState-driven visibility
   - Event-based command execution
   - Separation of concerns (UI vs logic)
   
   ## Components
   - GameState enum
   - Console overlay
   - ConsoleAction events
   - EventBus integration
   
   ## Data Flow
   [Diagram of backtick press -> state change -> render -> command -> event]
   
   ## Extension Points
   - Adding new commands
   - Handling new events
   - Custom console styling
   ```

2. **Update Main README.md**:
   - Add section on debug console
   - Document backtick key binding
   - List available commands
   - Explain event-driven architecture

3. **API Documentation**:
   - Add rustdoc examples to:
     - `GameState::can_transition_to()`
     - `Console::new()`
     - `ConsoleAction` variants
     - State transition methods

4. **Developer Guide** (`docs/ADDING_CONSOLE_COMMANDS.md`):
   ```markdown
   # How to Add Console Commands
   
   1. Add variant to ConsoleAction enum
   2. Add match arm in Console::handle_command()
   3. Add event publishing in adapter.rs
   4. Subscribe to event in main.rs
   5. Add help text
   6. Write tests
   ```

### Files to Create/Update:
- `docs/CONSOLE_ARCHITECTURE.md` (new)
- `docs/ADDING_CONSOLE_COMMANDS.md` (new)
- `README.md` (update)
- `src/game_state.rs` (add rustdoc examples)
- `moho_ui/src/overlays/console.rs` (add rustdoc examples)

---

## Summary

**Current Status**: Phase 3 complete, console fully functional

**Blocking Items**: None - console works and is production-ready

**Nice-to-Have Items**:
- Task 10: Architectural decision about InputRouter (deferred)
- Task 11: Integration tests (recommended before v1.0)
- Task 12: Documentation (recommended for team onboarding)

**Next Steps** (when returning to this feature):
1. Review Task 10 decision based on actual needs
2. Implement high-priority tests from Task 11
3. Write architecture documentation from Task 12
4. Consider adding more commands (teleport, spawn, etc.)

``` 
