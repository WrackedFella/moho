# Moho Documentation

Start with the [project README](../README.md). Planning and work items live in
[`_todo/`](../_todo/README.md).

## Architecture

- [Architecture Decision Records](adr/README.md): crate boundaries and other
  decisions that constrain future work.
- Crate overviews: [`moho_core`](../moho_core/README.md),
  [`moho_renderer`](../moho_renderer/README.md),
  [`moho_audio`](../moho_audio/README.md), [`moho_ui`](../moho_ui/README.md),
  [`moho_input`](../moho_input/README.md), [`moho_sim`](../moho_sim/README.md),
  [`moho_types`](../moho_types/README.md).
- API docs: `cargo doc --open --no-deps`.

## Reference

- [Event bus best practices](engine_core/EVENT_BUS_BEST_PRACTICES.md) and
  [performance](engine_core/EVENT_BUS_PERFORMANCE.md)
- [GPU ABI](gpu_abi.md): shader/CPU data layout rules
- [Prefs format](prefs_format.md): `config/prefs.ini`
- [Console architecture](CONSOLE_ARCHITECTURE.md) and
  [adding console commands](ADDING_CONSOLE_COMMANDS.md)

## Known constraints

- Publishing an event from inside an event-bus handler deadlocks; hand off
  through a channel instead.
- `AudioSystem` is not `Send`/`Sync` and stays on the main thread.
- Event-bus history costs ~4.9× per publish when enabled; it is off by default.
- The UI takes input precedence: `InputDispatcher` routes window events by
  priority and stops at the first consumer. Mouse-wheel events reach the game
  only when no UI overlay is visible, and are dropped (not forwarded) if the UI
  lock can't be taken.
