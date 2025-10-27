# Debug Console Architecture

## Overview

The debug console is an egui-based overlay controlled by the central `GameState` state machine. It is intended for developer diagnostics and quick runtime commands. The console is not modal by itself — input routing and priority are handled by the `InputDispatcher` (UI-first) and optionally by the `InputRouter` for layered input scenarios.

## Components

- `GameState` — source-of-truth for UI and simulation modes (e.g., `Playing`, `ConsoleOpen`, `Paused`).
- `Console` overlay — input buffer, history, and output rendering (in `moho_ui/src/overlays/console.rs`).
- `ConsoleAction` — returned by the console render step to indicate side effects (publish events, toggle states).
- `EguiAdapter` — receives `GameState` each frame and renders the console overlay when appropriate; it publishes `UiEvent`/`DebugEvent` via the global `EventBus`.
- `EventBus` — typed publish/subscribe system used to broadcast console commands to interested systems.

## Data Flow

1. Key press (backtick) -> `App::enter_console()` sets `GameState::ConsoleOpen`.
2. Each frame the adapter sees `GameState::ConsoleOpen` and calls `Console::render()`.
3. `Console::render()` returns `ConsoleAction` variants (e.g., `ToggleGodMode`, `ExitRequested`).
4. Adapter publishes corresponding events onto `EventBus` (for example `DebugEvent::ToggleGodMode`).
5. Subscribers receive events and apply changes (e.g., set god-mode, open menu, exit application).

## Extension Points

- Add new `ConsoleAction` variants and handle them in `moho_ui::adapter` to publish new events.
- Register input handlers with `InputRouter` (when fully integrated) to support layered input semantics.
- Add richer parsing or scripting layers for more advanced workflows.

## Safety & Threading

All console-driven events are published through the `EventBus` which is thread-safe (Arc + RwLock patterns). Console UI runs on the main thread (egui), and event handling can be consumed either on the main thread or worker threads depending on subscribers.

## Tests & Verification

- Unit tests for `Console` parsing and `ConsoleAction` generation live in `moho_ui` tests.
- Integration tests for event publishing and state transitions are tracked in `todo/CONSOLE_TODO.md` and planned as `tests/console_*`.
