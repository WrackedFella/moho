//! The bindings being edited on the Controls page, before Apply makes them live.

use moho_input::Action;
use moho_input::bindings::{ActionBindings, Binding, Input};

use crate::input::SearchlightAction;

/// One action's bindings: two positional slots, then whatever it holds beyond them.
#[derive(Clone, Debug)]
struct Row {
    action: SearchlightAction,
    slots: [Option<Binding>; 2],
    rest: Vec<Binding>,
}

impl Row {
    fn holds(&self, binding: Binding) -> bool {
        self.slots.contains(&Some(binding)) || self.rest.contains(&binding)
    }
}

/// The bindings the Controls page edits: every action's two slots, and any bindings beyond them.
#[derive(Clone, Debug)]
pub struct ControlsDraft {
    /// What the draft was built from, so the toggle modes survive `bindings`.
    base: ActionBindings<SearchlightAction>,
    /// One row per action, in `SearchlightAction::ALL` order.
    rows: Vec<Row>,
}

/// What happened to a captured input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capture {
    /// The action's primary slot now holds the plain press of the input.
    Set,
    /// Another action already holds that binding; nothing changed.
    InUse { by: SearchlightAction },
    /// The input is one menus keep for themselves; nothing changed.
    Reserved,
}

impl ControlsDraft {
    pub fn new(bindings: &ActionBindings<SearchlightAction>) -> Self {
        let rows = SearchlightAction::ALL
            .iter()
            .map(|&action| {
                let held = bindings.get(action);
                Row {
                    action,
                    slots: [held.first().copied(), held.get(1).copied()],
                    rest: held.iter().skip(2).copied().collect(),
                }
            })
            .collect();
        Self {
            base: bindings.clone(),
            rows,
        }
    }

    /// The bindings as drafted: each action's filled slots in order, then its bindings beyond two.
    pub fn bindings(&self) -> ActionBindings<SearchlightAction> {
        let mut bindings = self.base.clone();
        for row in &self.rows {
            let held = row.slots.iter().flatten().chain(&row.rest).copied();
            bindings.set(row.action, held.collect());
        }
        bindings
    }

    /// `action`'s two slots.
    pub fn slots(&self, action: SearchlightAction) -> [Option<Binding>; 2] {
        self.rows
            .iter()
            .find(|row| row.action == action)
            .map_or([None, None], |row| row.slots)
    }

    /// Puts the plain press of `input` in `action`'s primary slot unless it is reserved or another
    /// action holds it.
    pub fn capture(&mut self, action: SearchlightAction, input: Input) -> Capture {
        let binding = Binding::from(input);
        if binding.is_protected() {
            return Capture::Reserved;
        }
        if let Some(other) = self
            .rows
            .iter()
            .find(|row| row.action != action && row.holds(binding))
        {
            return Capture::InUse { by: other.action };
        }
        if let Some(row) = self.rows.iter_mut().find(|row| row.action == action) {
            row.slots[0] = Some(binding);
        }
        Capture::Set
    }
}

/// The player-facing name of `action`.
pub fn label(action: SearchlightAction) -> &'static str {
    match action {
        SearchlightAction::MoveForward => "Move forward",
        SearchlightAction::MoveBack => "Move back",
        SearchlightAction::MoveLeft => "Move left",
        SearchlightAction::MoveRight => "Move right",
        SearchlightAction::Jump => "Jump",
        SearchlightAction::Fire => "Fire",
        SearchlightAction::Reload => "Reload",
        SearchlightAction::CheckMagazine => "Check magazine",
        SearchlightAction::Restart => "Restart",
        SearchlightAction::LeanLeft => "Lean left",
        SearchlightAction::LeanRight => "Lean right",
        SearchlightAction::Use => "Use",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use moho_input::bindings::{Action, Binding};
    use moho_input::key::{Key, MouseButton};

    use super::*;

    fn defaults() -> ActionBindings<SearchlightAction> {
        let (bindings, warnings) = ActionBindings::load(&BTreeMap::new());
        assert!(warnings.is_empty());
        bindings
    }

    fn draft() -> ControlsDraft {
        ControlsDraft::new(&defaults())
    }

    #[test]
    fn an_input_another_action_holds_is_refused_naming_it() {
        let mut draft = draft();

        let to_lean = draft.capture(SearchlightAction::Reload, Input::Key(Key::Q));
        let to_check = draft.capture(SearchlightAction::Fire, Input::Key(Key::C));
        let to_reload = draft.capture(SearchlightAction::CheckMagazine, Input::Key(Key::R));

        assert_eq!(
            to_lean,
            Capture::InUse {
                by: SearchlightAction::LeanLeft
            }
        );
        assert_eq!(
            to_check,
            Capture::InUse {
                by: SearchlightAction::CheckMagazine
            }
        );
        assert_eq!(
            to_reload,
            Capture::InUse {
                by: SearchlightAction::Reload
            }
        );
        assert_eq!(draft.bindings(), defaults(), "a refusal changes nothing");
    }

    #[test]
    fn a_protected_input_is_refused_as_reserved() {
        let mut draft = draft();
        let protected = [
            Key::ArrowUp,
            Key::ArrowDown,
            Key::ArrowLeft,
            Key::ArrowRight,
            Key::Enter,
            Key::Escape,
        ];

        for key in protected {
            let outcome = draft.capture(SearchlightAction::Reload, Input::Key(key));

            assert_eq!(outcome, Capture::Reserved, "{key:?}");
        }
        assert_eq!(draft.bindings(), defaults(), "a refusal changes nothing");
    }

    #[test]
    fn a_free_input_becomes_the_primary_binding() {
        let mut draft = draft();

        let key = draft.capture(SearchlightAction::Reload, Input::Key(Key::T));
        let button = draft.capture(SearchlightAction::Fire, Input::Mouse(MouseButton::Right));

        let bindings = draft.bindings();
        assert_eq!((key, button), (Capture::Set, Capture::Set));
        assert_eq!(
            bindings.get(SearchlightAction::Reload),
            [Binding::key(Key::T)]
        );
        assert_eq!(
            bindings.get(SearchlightAction::Fire),
            [Binding::mouse(MouseButton::Right)]
        );
        assert_eq!(
            bindings.get(SearchlightAction::Jump),
            SearchlightAction::Jump.default_bindings(),
            "another action is untouched"
        );
    }

    #[test]
    fn capture_keeps_check_magazines_second_binding() {
        let mut draft = draft();

        let outcome = draft.capture(SearchlightAction::CheckMagazine, Input::Key(Key::Z));

        assert_eq!(outcome, Capture::Set);
        assert_eq!(
            draft.bindings().get(SearchlightAction::CheckMagazine),
            [Binding::key(Key::Z), Binding::key(Key::R).held()]
        );
    }

    #[test]
    fn recapturing_the_same_input_changes_nothing() {
        let mut draft = draft();

        let first = draft.capture(SearchlightAction::Reload, Input::Key(Key::R));
        let second = draft.capture(SearchlightAction::Reload, Input::Key(Key::R));

        assert_eq!((first, second), (Capture::Set, Capture::Set));
        assert_eq!(draft.bindings(), defaults());
    }

    #[test]
    fn bindings_beyond_two_survive_a_round_trip() {
        let mut bindings = defaults();
        let three = vec![
            Binding::key(Key::Space),
            Binding::key(Key::X),
            Binding::key(Key::Y),
        ];
        bindings.set(SearchlightAction::Jump, three);
        let mut draft = ControlsDraft::new(&bindings);

        let untouched = draft.bindings();
        let outcome = draft.capture(SearchlightAction::Jump, Input::Key(Key::Z));

        assert_eq!(untouched, bindings);
        assert_eq!(outcome, Capture::Set);
        assert_eq!(
            draft.bindings().get(SearchlightAction::Jump),
            [
                Binding::key(Key::Z),
                Binding::key(Key::X),
                Binding::key(Key::Y)
            ]
        );
    }

    #[test]
    fn every_action_has_a_distinct_label() {
        let labels: Vec<&str> = SearchlightAction::ALL.iter().map(|&a| label(a)).collect();

        let mut distinct = labels.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(distinct.len(), SearchlightAction::ALL.len(), "{labels:?}");
        assert!(labels.iter().all(|label| !label.trim().is_empty()));
        assert_eq!(label(SearchlightAction::MoveForward), "Move forward");
        assert_eq!(label(SearchlightAction::CheckMagazine), "Check magazine");
        assert_eq!(label(SearchlightAction::Restart), "Restart");
        assert_eq!(label(SearchlightAction::LeanLeft), "Lean left");
    }
}
