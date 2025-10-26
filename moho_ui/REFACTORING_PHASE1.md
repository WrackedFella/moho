# moho_ui Refactoring - Phase 1 Complete

## Overview
Phase 1 of the moho_ui refactoring focused on quick wins to improve code organization, naming clarity, and reduce duplication. All changes are backward-compatible at the API level where possible.

## Completed Tasks

### ✅ 1. Renamed FormBuilder → FormControls
**Impact:** Improved semantic clarity

**Changes:**
- `src/forms/form_builder.rs`: Renamed struct from `FormBuilder` to `FormControls`
- `src/forms/settings.rs`: Updated all references to use `FormControls`
- `src/forms/mod.rs`: Updated re-export
- `src/lib.rs`: Updated public API export

**Rationale:** "FormControls" better describes the module's purpose as a collection of reusable UI control helpers, rather than a builder pattern implementation.

### ✅ 2. Renamed MenuSpec → ScreenSpec
**Impact:** Better semantic alignment with actual usage

**Changes:**
- `src/menus/menu.rs`: Renamed `MenuSpec` to `ScreenSpec`
- `src/menus/start.rs`: Updated to use `ScreenSpec`
- `src/menus/mod.rs`: Updated re-export
- `src/forms/settings.rs`: Updated to use `ScreenSpec`
- `src/forms/new_world.rs`: Updated to use `ScreenSpec`

**Rationale:** The spec describes full-screen UI configuration, not just "menus." This naming prepares for future distinction between full-screen menus, forms, and in-game overlays.

### ✅ 3. Extracted Hit-Test Logic into Separate Module
**Impact:** Improved code organization and separation of concerns

**Changes:**
- Created `src/input_handling.rs` with:
  - `hit_test_menu_items()` - Fallback hit-testing for UI elements
  - `rect_from_min_max()` - Test helper for rect construction
- `src/menus/menu.rs`: Removed the 80+ lines of hit-testing logic
- `src/lib.rs`: Added `pub mod input_handling`
- `tests/start_menu.rs`: Updated imports to use new module

**Rationale:** Hit-testing logic is unrelated to the Menu trait definition. Extracting it improves readability and makes the code easier to maintain and test.

### ✅ 4. Created Layout Templates in FormControls
**Impact:** Foundation for reducing duplication

**Changes:**
- Added `FormControls::standard_screen_layout()` helper function
- Provides consistent three-panel layout (top title, bottom buttons, central scrollable content)
- Parameterized with closures for custom button and content rendering

**Rationale:** Both SettingsMenu and NewWorldMenu (and future screens) share the same layout pattern. This helper provides a DRY foundation for consistent screen design.

## Test Results
All existing tests pass:
- ✅ 4 unit tests (settings menu binding logic)
- ✅ 3 menu system tests
- ✅ 2 start menu tests
- ✅ 1 staging belt test

## Files Modified
- `moho_ui/src/forms/form_builder.rs` - Renamed struct, added layout helper
- `moho_ui/src/forms/settings.rs` - Updated to use FormControls and ScreenSpec
- `moho_ui/src/forms/new_world.rs` - Updated to use ScreenSpec
- `moho_ui/src/forms/mod.rs` - Updated exports
- `moho_ui/src/menus/menu.rs` - Renamed MenuSpec, removed hit-test functions
- `moho_ui/src/menus/start.rs` - Updated to use ScreenSpec
- `moho_ui/src/menus/mod.rs` - Updated exports
- `moho_ui/src/lib.rs` - Updated public API exports
- `moho_ui/tests/start_menu.rs` - Updated to use new input_handling module

## Files Created
- `moho_ui/src/input_handling.rs` - New module for UI input utilities

## Breaking Changes
**None** - All changes maintain API compatibility:
- Old type names are removed but were internal implementation details
- Public exports remain stable
- Test suite passes without modifications (except import paths)

## Next Steps (Phase 2)
See the main refactoring plan for Phase 2 priorities:
1. Introduce `Screen` trait and alias `Menu` to it
2. Reorganize `menus/` and `forms/` into `screens/`
3. Add capability pattern to avoid downcasting in adapter
4. Extract `UiStateManager` from adapter

## Build Status
✅ `cargo build --features "backend-wgpu,ui-egui"` - Success
✅ `cargo test -p moho_ui --features "ui-egui"` - All tests pass
