# 0026 — Menus and settings controls are part of the UI shell seam

**Status:** Accepted on merge of #695; amends [0017](0017-ui-shell-and-debug-values-are-engine-seams.md): seam 9 also covers menu screens and settings controls, seam 10 also covers a game's player settings

## Context

Engine request [#444](https://github.com/WrackedFella/moho/issues/444) asks that a game show
menus and a settings screen from its own crates without naming egui or wgpu. Both games need
the engine's own settings (mouse feel, volumes, graphics quality, window); the placement rule
puts what both games need in the engine. [ADR-0017](0017-ui-shell-and-debug-values-are-engine-seams.md)'s
seam 9 names the console, UI layers and screen text only, and its seam 10 holds hidden debug
values only. Searchlight's menus today are game-side `ScreenText` rows with their own hit
tests (FPS-F3-01 #458, -02 #459).

## Decision

- Seam 9 gains **menu screens**. A game describes a screen as data: a title, a list of items
  and an optional back action, each button carrying a value of the game's own action type.
  The shell owns a stack of screens, the focus, keyboard and mouse handling, layout and
  drawing, and returns what the player chose as values the game takes each frame, like
  console commands. The game pushes and pops screens; the shell never changes the stack on
  its own.
- Focus and keys are the shell's: Up and Down move the focus in item order and wrap, Enter
  or a click chooses, Escape chooses the top screen's back action. Hover moves the focus.
  The console key still wins over an open menu. While a screen is open the shell takes every
  key and button, as an open console does.
- Item kinds are a closed engine enum: buttons, the engine's settings rows, Apply, and later
  a game's own rows and item lists. A game cannot add a widget kind; it asks the engine
  (an engine request), which keeps one look and one keyboard model across games.
- The engine's settings come as **ready-made rows** a game places on a screen: mouse
  sensitivity, smoothing and invert Y, the sound-effect, music and UI volumes, shadow and
  ambient-occlusion quality, window mode and resolution. The shell stages changes on the
  open screen; Apply hands the game the applied values and Back (popping the screen)
  discards them. The game saves them to its own prefs file and applies them, through an
  engine function for the renderer, audio and window; look settings belong to the game's
  input map.
- Staging covers only the engine's settings values, never the whole prefs, so an Apply
  never overwrites a value the console or another writer stored meanwhile.
- Seam 10 gains **game settings**: a game stores named values for its own player settings in
  a `[game]` section of its prefs file, which the engine saves and returns unread, with the
  same name and value rules as `[debug]`. Unlike debug values they may appear in Settings, as
  game rows with the engine's control kinds and staging; the console and Settings read and
  write the same entry. Built with the first game row (ENG-F18-17, #697).

## Consequences

- ENG-F5's "no engine API change" check covers the menu and settings types.
- Searchlight's game-side menu kit is deleted; its main, pause and Settings screens are
  shell screens.
- Strategy keeps its own egui settings screen until its ENG-M3 migration card moves its
  Video, Audio and mouse rows onto these controls; until then the engine's settings rows
  have two implementations.
- Keybind capture, a conflict modal and gamepad navigation are not part of this decision;
  they join seam 9 as further item kinds when a game card needs them.
- `moho_ui_shell` depends on `moho_core` for the prefs it stages.
