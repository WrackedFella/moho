# How to Add Console Commands

This page describes the minimal steps to add a new console command to the debug console overlay.

1) Add a new variant to `ConsoleAction` (or the console command enum used by `Console::handle_command`).

2) Implement parsing/handling in `moho_ui/src/overlays/console.rs` inside `Console::handle_command()` so the command maps to the new `ConsoleAction`.

3) Update `moho_ui::adapter` (adapter.rs) to handle the new `ConsoleAction` by publishing an appropriate `EventBus` event (e.g., `DebugEvent::YourCommand`).

4) Subscribe to the published event where the behavior should be implemented (for example, in `src/main.rs` or the relevant system crate).

5) Add unit tests for parsing and integration tests for event publishing (see `todo/CONSOLE_TODO.md` task list).

6) Add help text to the console `help` output so users see the new command.

Example (pseudo-change):

```rust
// moho_ui/src/overlays/console.rs
match input_trimmed {
    "teleport" => ConsoleAction::Teleport{ x: 0.0, y: 0.0, z: 0.0 },
    _ => ConsoleAction::Unknown(input_trimmed.to_string()),
}
```

```rust
// moho_ui/src/adapter.rs
match action {
    ConsoleAction::Teleport{x,y,z} => event_bus.publish(DebugEvent::Teleport{x,y,z}),
    _ => {}
}
```
