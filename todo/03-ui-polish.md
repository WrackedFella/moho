# Phase 3: UI & Settings

**Goal:** Polish independent systems using `moho_ui`.

## Tasks

### 1. Menu Improvements
- **Visuals:** Standardize button styles, fonts, and padding.
- **Audio:** Add background music support to the Main Menu (using `moho_audio`).
- **Feedback:** Add hover/click SFX.

### 2. Settings: Resolution Locking
- **Problem:** Window resizing causes UI scaling issues or non-standard aspect ratios.
- **Fix:** Implement a "Video Settings" tab.
    - Dropdown for standard resolutions (1080p, 1440p, 4k).
    - Toggle for Fullscreen / Windowed / Borderless.
    - Write to `prefs.ini`.

### 3. HUD Visual Framework
- **Compass:** UI Element showing cardinal directions relative to Camera Yaw.
- **Status Bars:** Visual containers for Health and Stamina (connect to dummy values in `PlayerState` for now).
- **Item Bar:** Hotbar visual slots (empty implementation).
