# 0017 — The UI shell and debug values are engine seams

**Status:** Proposed (2026-10-16; extends [0012](0012-engine-crate-map-for-m2.md))

## Context

[ADR-0012](0012-engine-crate-map-for-m2.md) lists the engine's public seams and makes adding
one an ADR-level change. ENG-F18 adds two that games build on: the console
([#403](https://github.com/WrackedFella/moho/issues/403), #404) with screen text (#407), and a
game's debug values in prefs (#405). [ADR-0013](0013-ui-reaches-the-renderer-as-paint-data.md)
says no engine crate depends on egui; ADR-0012 puts egui integration in `moho_ui_shell`.

## Decision

- Seam 9, **the UI shell** (`moho_ui_shell`): console command registration, engine commands
  returned as values, the console binding, UI layers, screen text. Games register commands as
  data plus a pure parse function; names are unique across engine and game,
  case-insensitively, and a taken name is refused. Game commands reach the game as its own
  values and enter the simulation only through its tick command
  ([ADR-0011](0011-simulation-stays-network-ready.md)).
- Seam 10, **debug values** (`moho_core::prefs`): a game stores named string values for its
  hidden debug commands, which the engine saves and returns unread in that game's own prefs
  file. Console commands are hidden: never shown in a Settings screen.
- `console` is a reserved name in every game's `[bindings]` section.
- `moho_ui_shell` is the engine's UI crate in ADR-0013's sense; no other engine crate depends
  on egui.

## Consequences

- ENG-F5's "no engine API change" check includes seams 9 and 10.
- Strategy (#406) moves onto seam 9; its own console code is deleted there.
- The renderer still takes one UI source; several sources compose through `UiLayers`.
- An egui upgrade touches `moho_ui_shell` and `flagstone_ui`.
