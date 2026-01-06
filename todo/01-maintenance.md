# Phase 1: Maintenance & Hygiene

**Goal:** Clean up the workspace and fix immediate annoyances to improve developer experience.

## Tasks

### 1. Reverse Console Scroll
- **Problem:** Console appends messages to the top (oldest on bottom), making it hard to read the latest logs.
- **Fix:** Update UI logic in `moho_ui` to append messages to the bottom (newest on bottom) and auto-scroll.

### 2. Clean Shutdown
- **Problem:** App may leave dangling threads or GPU resources on exit.
- **Fix:** Review `Drop` implementations and main loop exit condition to ensure clean termination.

### 3. Prune Outdated Tests
- **Problem:** Test suite has rot over time.
- **Action:** Run `cargo test`, identify failing tests, and either update them or remove them if effectively dead code.

### 4. Local READMEs
- **Action:** Add a simple `README.md` to each crate folder (`moho_core`, `moho_renderer`, etc.) explaining its purpose and responsibility.
