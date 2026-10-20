# moho_input

Engine input: the keys, mouse buttons and gamepad inputs the engine names, bindings a game declares as data,
and the action map that hands the game one frame of actions per tick.

## Pieces

- `Key`: platform-free key enum. `Key::from_winit` maps a winit `PhysicalKey`;
  `name` is the persisted spelling, `parse` reads it back (case-insensitive, with
  aliases such as `Up`, `Esc`, `Return`, `Space`, `Control` and punctuation glyphs;
  prefs files need the word names, since `;`, `#`, `[` and `,` are special there),
  `label` is the player-facing text.
- `MouseButton`: `Left`, `Right`, `Middle`; persisted as `Mouse Left` etc.
- `PadButton`, `Stick`, `StickDir`, `PadInput`: gamepad inputs, persisted as
  `Pad South` or `Pad LeftStick Up`. A stick direction is held past
  `STICK_DIRECTION_THRESHOLD`.
- `Binding`: one physical input that triggers an action (`Binding::Key`, `Binding::Mouse`
  or `Binding::Pad`).
- `Action`: trait a game implements on its own action enum: `ALL`, a stable lowercase `snake_case`
  `name` used as the persisted id, and `default_bindings`.
- `ActionBindings<A>`: every action's current bindings. `load` reads a raw
  `[bindings]` section (action name → comma-separated key names, `Unbound` or
  empty for none) and returns warnings for unusable lines, which keep their
  defaults; `to_section` writes every action back.
- `ActionMap<A>`: owns the bindings, which bindings are down, this tick's edges and
  the mouse-look accumulator, `LookSettings` (turn per count, smoothing on/off; `set_look`) and the smoothing state. Feed it with `key`, `mouse_button`,
  `mouse_motion`, `pad_button`, `pad_axis`, `pad_disconnected` and `release_all`, or from winit through `action_map::handle_window_event`
  / `handle_device_event`. `end_tick` is the only read: it returns an `ActionFrame`
  (`held`, `pressed`, `released` masks and `look`; plain `Copy` data, at most 64 actions).
  `sample_frame(frame_dt)` turns the mouse counts since the last sample, plus the right stick at `PAD_LOOK_PER_SECOND` times its deflection per second, into a pending look (`pending_look`); `end_tick` returns it and zeroes it.
- `Gamepads`: the only `gilrs` user. `Gamepads::new` returns `None` with one warning when
  the backend can't start; `poll` drains pad events into an `ActionMap` (all pads feed one map).

```rust
use moho_input::Action;
use moho_input::bindings::{ActionBindings, Binding};
use moho_input::key::Key;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
enum MyAction { Jump }

impl Action for MyAction {
    const ALL: &'static [Self] = &[MyAction::Jump];
    fn name(self) -> &'static str { "jump" }
    fn default_bindings(self) -> &'static [Binding] { &[Binding::Key(Key::Space)] }
}

let section = [("jump".to_string(), "F".to_string())].into();
let (bindings, warnings) = ActionBindings::<MyAction>::load(&section);
assert!(warnings.is_empty());
assert_eq!(bindings.get(MyAction::Jump), [Binding::Key(Key::F)]);
```

`ActionBindings` takes the raw section rather than `Prefs`, so this crate does not
depend on `moho_core`. The strategy game's actions live in `flagstone_ui::actions`.
