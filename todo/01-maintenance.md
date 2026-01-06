# Phase 1: Maintenance & Hygiene

**Goal:** Clean up the workspace and fix immediate annoyances to improve developer experience.

## Tasks

### 1. Reverse Console Scroll (Completed)
- **Problem:** Console appends messages to the top (oldest on bottom), making it hard to read the latest logs.
- **Fix:** Update UI logic in `moho_ui` to append messages to the bottom (newest on bottom) and auto-scroll.

### 2. Code Quality Check (Completed)
- **Action:** Run `cargo fmt` and `cargo clippy`. Fixed multiple warnings (unused variables, type complexity, len_zero, etc.).

### 3. Clean Shutdown (Completed)
- **Problem:** App may leave dangling threads or GPU resources on exit.
- **Fix:** Implemented `Drop` for `App` struct to signal limits and join background generation threads.

### 4. Prune Outdated Tests (Completed)
- **Problem:** Test suite has rot over time.
- **Action:** Run `cargo test`. Pruned failing tests in `moho_core` (`extraction.rs`) that relied on outdated mesh assumptions. Fixed clippy warnings in tests.

### 5. Local READMEs (Completed)
- **Action:**
    - Created `moho_types/README.md`.
    - Fixed corrupted/duplicated content in `moho_ui/README.md`.
    - Verified existence of other crate READMEs.
