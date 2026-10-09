# moho_input

Engine input: the keys and mouse buttons the engine names, bindings a game declares as data,
and the action map that hands the game one frame of actions per tick.

## Pieces

- `Key`: platform-free key enum. `Key::from_winit` maps a winit `PhysicalKey`;
  `name` is the persisted spelling, `parse` reads it back (case-insensitive, with
  aliases such as `Up`, `Esc`, `Return`, `Space`, `Control` and punctuation glyphs;
  prefs files need the word names, since `;`, `#`, `[` and `,` are special there),
  `label` is the player-facing text.
- `MouseButton`: `Left`, `Right`, `Middle`; persisted as `Mouse Left` etc.
- `Binding`: one physical input that triggers an action (`Binding::Key` or `Binding::Mouse`).
- `Action`: trait a game implements on its own action enum: `ALL`, a stable lowercase `snake_case`
  `name` used as the persisted id, and `default_bindings`.
- `ActionBindings<A>`: every action's current bindings. `load` reads a raw
  `[bindings]` section (action name → comma-separated key names, `Unbound` or
  empty for none) and returns warnings for unusable lines, which keep their
  defaults; `to_section` writes every action back.
- `ActionMap<A>`: owns the bindings, which bindings are down, this tick's edges and
  the mouse-look accumulator and filter. Feed it with `key`, `mouse_button`,
  `mouse_motion` and `release_all`, or from winit through `action_map::handle_window_event`
  / `handle_device_event`. `end_tick` is the only read: it returns an `ActionFrame`
  (`held`, `pressed`, `released` masks and `look`; plain `Copy` data, at most 64 actions).

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
depend on `moho_core`. The strategy game's actions live in `moho_ui::actions`.
