//! The bindings being edited on the Controls page, before Apply makes them live.

use moho_input::Action;
use moho_input::bindings::{ActionBindings, Binding, Input};
use moho_ui_shell::Slot;

use crate::input::SearchlightAction;

/// One action's bindings: two positional slots, then whatever it holds beyond them.
#[derive(Clone, Debug)]
struct Row {
    action: SearchlightAction,
    slots: [Option<Binding>; 2],
    rest: Vec<Binding>,
}

impl Row {
    fn from_bindings(action: SearchlightAction, held: &[Binding]) -> Self {
        Self {
            action,
            slots: [held.first().copied(), held.get(1).copied()],
            rest: held.iter().skip(2).copied().collect(),
        }
    }

    fn holds(&self, binding: Binding) -> bool {
        self.slots.contains(&Some(binding)) || self.rest.contains(&binding)
    }

    /// Empties every position holding `binding`.
    fn remove(&mut self, binding: Binding) {
        for slot in &mut self.slots {
            if *slot == Some(binding) {
                *slot = None;
            }
        }
        self.rest.retain(|held| *held != binding);
    }
}

/// The bindings the Controls page edits: every action's two slots, and any bindings beyond them.
#[derive(Clone, Debug)]
pub struct ControlsDraft {
    /// What the draft was built from, so the toggle modes survive `bindings`.
    base: ActionBindings<SearchlightAction>,
    /// One row per action, in `SearchlightAction::ALL` order.
    rows: Vec<Row>,
    /// The capture waiting on a conflict answer.
    pending: Option<Pending>,
}

/// A captured input another action holds: `target` would take `binding` from `by` into `slot`.
#[derive(Clone, Copy, Debug)]
struct Pending {
    target: SearchlightAction,
    slot: Slot,
    binding: Binding,
    by: SearchlightAction,
}

/// What happened to a captured input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capture {
    /// The requested slot now holds the plain press of the input.
    Set,
    /// The action already holds the input in a slot or beyond them; nothing changed.
    Unchanged,
    /// Another action holds that binding in `slot` (`None` when beyond its two slots); nothing
    /// changed and the capture is pending until `replace` or `cancel`.
    Conflict {
        by: SearchlightAction,
        slot: Option<Slot>,
    },
    /// The input is one menus keep for themselves; nothing changed.
    Reserved,
}

impl ControlsDraft {
    pub fn new(bindings: &ActionBindings<SearchlightAction>) -> Self {
        let rows = SearchlightAction::ALL
            .iter()
            .map(|&action| Row::from_bindings(action, bindings.get(action)))
            .collect();
        Self {
            base: bindings.clone(),
            rows,
            pending: None,
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

    /// Whether `action` holds nothing in either slot or beyond them.
    pub fn is_unbound(&self, action: SearchlightAction) -> bool {
        self.rows
            .iter()
            .find(|row| row.action == action)
            .is_some_and(|row| row.slots == [None, None] && row.rest.is_empty())
    }

    /// Empties `action`'s `slot`, leaving the other slot and any bindings beyond two alone.
    pub fn clear(&mut self, action: SearchlightAction, slot: Slot) {
        if let Some(row) = self.rows.iter_mut().find(|row| row.action == action) {
            row.slots[index(slot)] = None;
        }
    }

    /// Puts every action back on its default bindings.
    pub fn reset_all(&mut self) {
        for row in &mut self.rows {
            *row = Row::from_bindings(row.action, row.action.default_bindings());
        }
    }

    /// Answers a pending conflict: the capturing action takes the input in the requested slot and
    /// the input leaves every other position, in both actions. Does nothing with none pending.
    pub fn replace(&mut self) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        if let Some(row) = self.rows.iter_mut().find(|row| row.action == pending.by) {
            row.remove(pending.binding);
        }
        if let Some(row) = self
            .rows
            .iter_mut()
            .find(|row| row.action == pending.target)
        {
            row.remove(pending.binding);
            row.slots[index(pending.slot)] = Some(pending.binding);
        }
    }

    /// Answers a pending conflict by dropping it.
    pub fn cancel(&mut self) {
        self.pending = None;
    }

    /// Puts the plain press of `input` in `action`'s `slot` unless it is reserved, another action
    /// holds it, or `action` already holds it in any position. A conflict stays pending until
    /// `replace` or `cancel`; any later capture drops it.
    pub fn capture(&mut self, action: SearchlightAction, requested: Slot, input: Input) -> Capture {
        self.pending = None;
        let binding = Binding::from(input);
        if binding.is_protected() {
            return Capture::Reserved;
        }
        if let Some(other) = self
            .rows
            .iter()
            .find(|row| row.action != action && row.holds(binding))
        {
            let slot = if other.slots[0] == Some(binding) {
                Some(Slot::Primary)
            } else if other.slots[1] == Some(binding) {
                Some(Slot::Secondary)
            } else {
                None
            };
            let by = other.action;
            self.pending = Some(Pending {
                target: action,
                slot: requested,
                binding,
                by,
            });
            return Capture::Conflict { by, slot };
        }
        match self.rows.iter_mut().find(|row| row.action == action) {
            Some(row) if row.holds(binding) => Capture::Unchanged,
            Some(row) => {
                row.slots[index(requested)] = Some(binding);
                Capture::Set
            }
            None => Capture::Unchanged,
        }
    }
}

fn index(slot: Slot) -> usize {
    match slot {
        Slot::Primary => 0,
        Slot::Secondary => 1,
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
        SearchlightAction::CheckGear => "Check gear",
        SearchlightAction::Restart => "Restart",
        SearchlightAction::LeanLeft => "Lean left",
        SearchlightAction::LeanRight => "Lean right",
        SearchlightAction::Use => "Use",
        SearchlightAction::Inventory => "Inventory",
        SearchlightAction::Aim => "Aim",
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
            let outcome = draft.capture(SearchlightAction::Reload, Slot::Primary, Input::Key(key));

            assert_eq!(outcome, Capture::Reserved, "{key:?}");
        }
        assert_eq!(draft.bindings(), defaults(), "a refusal changes nothing");
    }

    #[test]
    fn a_free_input_becomes_the_primary_binding() {
        let mut draft = draft();

        let key = draft.capture(SearchlightAction::Reload, Slot::Primary, Input::Key(Key::T));
        let button = draft.capture(
            SearchlightAction::Fire,
            Slot::Primary,
            Input::Mouse(MouseButton::Middle),
        );

        let bindings = draft.bindings();
        assert_eq!((key, button), (Capture::Set, Capture::Set));
        assert_eq!(
            bindings.get(SearchlightAction::Reload),
            [Binding::key(Key::T)]
        );
        assert_eq!(
            bindings.get(SearchlightAction::Fire),
            [Binding::mouse(MouseButton::Middle)]
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

        let outcome = draft.capture(
            SearchlightAction::CheckMagazine,
            Slot::Primary,
            Input::Key(Key::Z),
        );

        assert_eq!(outcome, Capture::Set);
        assert_eq!(
            draft.bindings().get(SearchlightAction::CheckMagazine),
            [Binding::key(Key::Z), Binding::key(Key::R).held()]
        );
    }

    #[test]
    fn recapturing_the_same_input_changes_nothing() {
        let mut draft = draft();

        let first = draft.capture(SearchlightAction::Reload, Slot::Primary, Input::Key(Key::R));
        let second = draft.capture(SearchlightAction::Reload, Slot::Primary, Input::Key(Key::R));

        assert_eq!((first, second), (Capture::Unchanged, Capture::Unchanged));
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
        let outcome = draft.capture(SearchlightAction::Jump, Slot::Primary, Input::Key(Key::Z));

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
    fn capturing_an_input_the_action_already_holds_changes_nothing() {
        let mut bindings = defaults();
        let held = vec![
            Binding::key(Key::Space),
            Binding::key(Key::X),
            Binding::key(Key::Y),
        ];
        bindings.set(SearchlightAction::Jump, held.clone());
        let mut from_secondary = ControlsDraft::new(&bindings);
        let mut from_rest = ControlsDraft::new(&bindings);

        let by_secondary =
            from_secondary.capture(SearchlightAction::Jump, Slot::Primary, Input::Key(Key::X));
        let by_rest = from_rest.capture(SearchlightAction::Jump, Slot::Primary, Input::Key(Key::Y));

        assert_eq!(
            (by_secondary, by_rest),
            (Capture::Unchanged, Capture::Unchanged)
        );
        assert_eq!(from_secondary.bindings().get(SearchlightAction::Jump), held);
        assert_eq!(from_rest.bindings().get(SearchlightAction::Jump), held);
    }

    #[test]
    fn a_conflict_changes_nothing_until_answered() {
        let mut draft = draft();

        let outcome = draft.capture(SearchlightAction::Reload, Slot::Primary, Input::Key(Key::Q));

        assert_eq!(
            outcome,
            Capture::Conflict {
                by: SearchlightAction::LeanLeft,
                slot: Some(Slot::Primary)
            }
        );
        assert_eq!(
            draft.bindings(),
            defaults(),
            "a pending conflict is a draft no-op"
        );
        assert!(!draft.is_unbound(SearchlightAction::LeanLeft));
    }

    #[test]
    fn replace_moves_the_input_and_leaves_the_other_slot_unbound() {
        let mut draft = draft();
        let outcome = draft.capture(SearchlightAction::Reload, Slot::Primary, Input::Key(Key::Q));

        draft.replace();

        assert!(matches!(outcome, Capture::Conflict { .. }));
        assert_eq!(
            draft.slots(SearchlightAction::Reload),
            [Some(Binding::key(Key::Q)), None]
        );
        assert_eq!(draft.slots(SearchlightAction::LeanLeft), [None, None]);
        assert!(draft.is_unbound(SearchlightAction::LeanLeft));
        assert!(!draft.is_unbound(SearchlightAction::Reload));
        assert!(draft.bindings().get(SearchlightAction::LeanLeft).is_empty());
        assert_eq!(
            draft.bindings().get(SearchlightAction::LeanRight),
            SearchlightAction::LeanRight.default_bindings(),
            "another action is untouched"
        );
    }

    #[test]
    fn cancel_keeps_both_actions() {
        let mut draft = draft();
        let outcome = draft.capture(SearchlightAction::Reload, Slot::Primary, Input::Key(Key::Q));

        draft.cancel();
        draft.replace();

        assert!(matches!(outcome, Capture::Conflict { .. }));
        assert_eq!(
            draft.bindings(),
            defaults(),
            "cancel dropped the pending input"
        );
        assert!(!draft.is_unbound(SearchlightAction::LeanLeft));
    }

    #[test]
    fn replace_empties_only_the_slot_that_held_the_input() {
        let mut draft = draft();
        let outcome = draft.capture(SearchlightAction::Fire, Slot::Primary, Input::Key(Key::C));

        draft.replace();

        assert_eq!(
            outcome,
            Capture::Conflict {
                by: SearchlightAction::CheckMagazine,
                slot: Some(Slot::Primary)
            }
        );
        assert_eq!(
            draft.slots(SearchlightAction::CheckMagazine),
            [None, Some(Binding::key(Key::R).held())],
            "Check magazine keeps Hold R"
        );
        assert_eq!(
            draft.slots(SearchlightAction::Fire),
            [Some(Binding::key(Key::C)), None]
        );
        assert!(!draft.is_unbound(SearchlightAction::CheckMagazine));
    }

    #[test]
    fn replace_empties_a_secondary_slot_and_removes_an_input_held_beyond_two() {
        let mut bindings = defaults();
        bindings.set(
            SearchlightAction::Jump,
            vec![
                Binding::key(Key::Space),
                Binding::key(Key::X),
                Binding::key(Key::Y),
            ],
        );
        let mut draft = ControlsDraft::new(&bindings);

        let secondary = draft.capture(SearchlightAction::Fire, Slot::Primary, Input::Key(Key::X));
        draft.replace();
        let beyond = draft.capture(SearchlightAction::Reload, Slot::Primary, Input::Key(Key::Y));
        draft.replace();

        assert_eq!(
            secondary,
            Capture::Conflict {
                by: SearchlightAction::Jump,
                slot: Some(Slot::Secondary)
            }
        );
        assert_eq!(
            beyond,
            Capture::Conflict {
                by: SearchlightAction::Jump,
                slot: None
            }
        );
        assert_eq!(
            draft.bindings().get(SearchlightAction::Jump),
            [Binding::key(Key::Space)]
        );
        assert_eq!(
            draft.slots(SearchlightAction::Jump),
            [Some(Binding::key(Key::Space)), None]
        );
        assert!(!draft.is_unbound(SearchlightAction::Jump));
    }

    #[test]
    fn a_new_capture_replaces_a_pending_conflict() {
        let mut draft = draft();
        draft.capture(SearchlightAction::Reload, Slot::Primary, Input::Key(Key::Q));

        let second = draft.capture(SearchlightAction::Fire, Slot::Primary, Input::Key(Key::E));
        draft.replace();

        assert_eq!(
            second,
            Capture::Conflict {
                by: SearchlightAction::LeanRight,
                slot: Some(Slot::Primary)
            }
        );
        assert_eq!(
            draft.slots(SearchlightAction::Fire),
            [Some(Binding::key(Key::E)), None]
        );
        assert!(draft.is_unbound(SearchlightAction::LeanRight));
        assert_eq!(
            draft.slots(SearchlightAction::Reload),
            [Some(Binding::key(Key::R)), None],
            "the first pending capture was dropped"
        );
        assert_eq!(
            draft.slots(SearchlightAction::LeanLeft),
            [Some(Binding::key(Key::Q)), None]
        );
    }

    #[test]
    fn a_free_capture_after_a_conflict_drops_the_pending_one() {
        let mut draft = draft();
        draft.capture(SearchlightAction::Reload, Slot::Primary, Input::Key(Key::Q));

        let free = draft.capture(SearchlightAction::Jump, Slot::Primary, Input::Key(Key::Z));
        draft.replace();

        assert_eq!(free, Capture::Set);
        assert_eq!(
            draft.slots(SearchlightAction::Reload),
            [Some(Binding::key(Key::R)), None]
        );
        assert_eq!(
            draft.slots(SearchlightAction::LeanLeft),
            [Some(Binding::key(Key::Q)), None]
        );
    }

    #[test]
    fn replace_leaves_the_input_once_when_the_capturing_action_already_held_it() {
        let mut bindings = defaults();
        bindings.set(
            SearchlightAction::Reload,
            vec![Binding::key(Key::R), Binding::key(Key::Q)],
        );
        let mut draft = ControlsDraft::new(&bindings);
        let outcome = draft.capture(SearchlightAction::Reload, Slot::Primary, Input::Key(Key::Q));

        draft.replace();

        assert!(matches!(outcome, Capture::Conflict { .. }));
        assert_eq!(
            draft.slots(SearchlightAction::Reload),
            [Some(Binding::key(Key::Q)), None]
        );
        assert_eq!(
            draft.bindings().get(SearchlightAction::Reload),
            [Binding::key(Key::Q)]
        );
    }

    #[test]
    fn capturing_the_other_slots_input_changes_nothing() {
        let mut draft = draft();

        let outcome = draft.capture(
            SearchlightAction::Reload,
            Slot::Secondary,
            Input::Key(Key::R),
        );

        assert_eq!(outcome, Capture::Unchanged);
        assert_eq!(
            draft.slots(SearchlightAction::Reload),
            [Some(Binding::key(Key::R)), None]
        );
        assert_eq!(draft.bindings(), defaults());
    }

    #[test]
    fn a_secondary_capture_reports_the_conflicting_action() {
        let mut draft = draft();

        let outcome = draft.capture(
            SearchlightAction::Reload,
            Slot::Secondary,
            Input::Key(Key::Q),
        );
        let pending_changed_nothing = draft.bindings() == defaults();
        draft.replace();

        assert_eq!(
            outcome,
            Capture::Conflict {
                by: SearchlightAction::LeanLeft,
                slot: Some(Slot::Primary)
            }
        );
        assert!(pending_changed_nothing);
        assert_eq!(
            draft.slots(SearchlightAction::Reload),
            [Some(Binding::key(Key::R)), Some(Binding::key(Key::Q))]
        );
        assert_eq!(draft.slots(SearchlightAction::LeanLeft), [None, None]);
    }

    #[test]
    fn an_empty_primary_with_a_secondary_saves_the_secondary_first() {
        let mut bindings = defaults();
        bindings.set(SearchlightAction::Jump, vec![]);
        let mut draft = ControlsDraft::new(&bindings);

        let secondary = draft.capture(SearchlightAction::Jump, Slot::Secondary, Input::Key(Key::Z));

        assert_eq!(secondary, Capture::Set);
        assert_eq!(
            draft.slots(SearchlightAction::Jump),
            [None, Some(Binding::key(Key::Z))]
        );
        assert_eq!(
            draft.bindings().get(SearchlightAction::Jump),
            [Binding::key(Key::Z)]
        );
        assert!(!draft.is_unbound(SearchlightAction::Jump));
    }

    #[test]
    fn a_secondary_capture_replaces_check_magazines_hold_r() {
        let mut draft = draft();

        let outcome = draft.capture(
            SearchlightAction::CheckMagazine,
            Slot::Secondary,
            Input::Key(Key::Z),
        );

        assert_eq!(outcome, Capture::Set);
        assert_eq!(
            draft.bindings().get(SearchlightAction::CheckMagazine),
            [Binding::key(Key::C), Binding::key(Key::Z)]
        );
    }

    #[test]
    fn a_secondary_capture_keeps_the_bindings_beyond_two() {
        let mut bindings = defaults();
        bindings.set(
            SearchlightAction::Jump,
            vec![
                Binding::key(Key::Space),
                Binding::key(Key::X),
                Binding::key(Key::Y),
            ],
        );
        let mut draft = ControlsDraft::new(&bindings);

        let outcome = draft.capture(SearchlightAction::Jump, Slot::Secondary, Input::Key(Key::Z));

        assert_eq!(outcome, Capture::Set);
        assert_eq!(
            draft.bindings().get(SearchlightAction::Jump),
            [
                Binding::key(Key::Space),
                Binding::key(Key::Z),
                Binding::key(Key::Y)
            ]
        );
    }

    #[test]
    fn a_secondary_replace_takes_an_input_held_beyond_two() {
        let mut bindings = defaults();
        bindings.set(
            SearchlightAction::Jump,
            vec![
                Binding::key(Key::Space),
                Binding::key(Key::X),
                Binding::key(Key::Y),
            ],
        );
        let mut draft = ControlsDraft::new(&bindings);

        let outcome = draft.capture(
            SearchlightAction::Reload,
            Slot::Secondary,
            Input::Key(Key::Y),
        );
        draft.replace();

        assert_eq!(
            outcome,
            Capture::Conflict {
                by: SearchlightAction::Jump,
                slot: None
            }
        );
        assert_eq!(
            draft.slots(SearchlightAction::Reload),
            [Some(Binding::key(Key::R)), Some(Binding::key(Key::Y))]
        );
        assert_eq!(
            draft.bindings().get(SearchlightAction::Jump),
            [Binding::key(Key::Space), Binding::key(Key::X)]
        );
    }

    #[test]
    fn a_protected_input_in_the_secondary_slot_is_reserved() {
        let mut draft = draft();

        let outcome = draft.capture(
            SearchlightAction::Reload,
            Slot::Secondary,
            Input::Key(Key::Escape),
        );

        assert_eq!(outcome, Capture::Reserved);
        assert_eq!(draft.bindings(), defaults());
    }

    #[test]
    fn recapturing_the_primarys_input_as_the_secondary_keeps_both_slots() {
        let mut bindings = defaults();
        bindings.set(
            SearchlightAction::Reload,
            vec![Binding::key(Key::R), Binding::key(Key::T)],
        );
        let mut draft = ControlsDraft::new(&bindings);

        let outcome = draft.capture(
            SearchlightAction::Reload,
            Slot::Secondary,
            Input::Key(Key::R),
        );

        assert_eq!(outcome, Capture::Unchanged);
        assert_eq!(
            draft.slots(SearchlightAction::Reload),
            [Some(Binding::key(Key::R)), Some(Binding::key(Key::T))]
        );
    }

    #[test]
    fn an_action_with_every_slot_empty_is_unbound() {
        let mut bindings = defaults();
        bindings.set(SearchlightAction::Jump, vec![]);

        let draft = ControlsDraft::new(&bindings);

        assert!(draft.is_unbound(SearchlightAction::Jump));
        assert!(!draft.is_unbound(SearchlightAction::Fire));
    }

    #[test]
    fn clearing_both_slots_leaves_the_action_unbound() {
        let mut draft = draft();

        draft.clear(SearchlightAction::MoveForward, Slot::Primary);
        draft.clear(SearchlightAction::MoveForward, Slot::Secondary);

        assert!(draft.is_unbound(SearchlightAction::MoveForward));
        assert_eq!(draft.slots(SearchlightAction::MoveForward), [None, None]);
        assert!(
            draft
                .bindings()
                .get(SearchlightAction::MoveForward)
                .is_empty()
        );
        assert_eq!(
            draft.bindings().get(SearchlightAction::MoveBack),
            SearchlightAction::MoveBack.default_bindings(),
            "another action is untouched"
        );
    }

    #[test]
    fn clearing_one_slot_leaves_the_other_and_those_beyond_two() {
        let mut bindings = defaults();
        bindings.set(
            SearchlightAction::Jump,
            vec![
                Binding::key(Key::Space),
                Binding::key(Key::X),
                Binding::key(Key::Y),
            ],
        );
        let mut draft = ControlsDraft::new(&bindings);

        draft.clear(SearchlightAction::Jump, Slot::Secondary);

        assert_eq!(
            draft.slots(SearchlightAction::Jump),
            [Some(Binding::key(Key::Space)), None]
        );
        assert_eq!(
            draft.bindings().get(SearchlightAction::Jump),
            [Binding::key(Key::Space), Binding::key(Key::Y)]
        );
        assert!(!draft.is_unbound(SearchlightAction::Jump));
    }

    #[test]
    fn clearing_an_empty_slot_changes_nothing() {
        let mut draft = draft();

        draft.clear(SearchlightAction::Reload, Slot::Secondary);

        assert_eq!(
            draft.slots(SearchlightAction::Reload),
            [Some(Binding::key(Key::R)), None]
        );
        assert_eq!(draft.bindings(), defaults());

        draft.clear(SearchlightAction::Reload, Slot::Primary);
        let once = draft.bindings();
        draft.clear(SearchlightAction::Reload, Slot::Primary);

        assert_eq!(draft.slots(SearchlightAction::Reload), [None, None]);
        assert_eq!(draft.bindings(), once, "clearing twice is clearing once");
    }

    #[test]
    fn reset_all_restores_check_magazines_two_defaults() {
        let mut draft = draft();
        draft.capture(
            SearchlightAction::CheckMagazine,
            Slot::Primary,
            Input::Key(Key::Z),
        );
        draft.capture(SearchlightAction::Reload, Slot::Primary, Input::Key(Key::T));
        draft.clear(SearchlightAction::Jump, Slot::Primary);

        draft.reset_all();

        assert_eq!(
            draft.bindings().get(SearchlightAction::CheckMagazine),
            [Binding::key(Key::C), Binding::key(Key::R).held()]
        );
        assert_eq!(draft.bindings(), defaults());
        assert!(!draft.is_unbound(SearchlightAction::Jump));
    }

    #[test]
    fn resetting_twice_is_resetting_once() {
        let mut draft = draft();
        draft.clear(SearchlightAction::Reload, Slot::Primary);
        draft.capture(SearchlightAction::Fire, Slot::Primary, Input::Key(Key::T));

        draft.reset_all();
        let once = draft.bindings();
        draft.reset_all();

        assert_eq!(once, defaults());
        assert_eq!(draft.bindings(), defaults());
    }

    #[test]
    fn reset_all_rebinds_an_action_left_unbound() {
        let mut draft = draft();
        draft.clear(SearchlightAction::MoveForward, Slot::Primary);
        draft.clear(SearchlightAction::MoveForward, Slot::Secondary);

        draft.reset_all();

        assert!(!draft.is_unbound(SearchlightAction::MoveForward));
        assert_eq!(
            draft.bindings().get(SearchlightAction::MoveForward),
            SearchlightAction::MoveForward.default_bindings()
        );
    }

    #[test]
    fn a_cleared_primary_takes_a_free_input_again() {
        let mut draft = draft();
        draft.clear(SearchlightAction::Reload, Slot::Primary);

        let outcome = draft.capture(SearchlightAction::Reload, Slot::Primary, Input::Key(Key::G));

        assert_eq!(outcome, Capture::Set);
        assert_eq!(
            draft.slots(SearchlightAction::Reload),
            [Some(Binding::key(Key::G)), None]
        );
    }

    #[test]
    fn clearing_the_only_binding_leaves_the_action_unbound_and_the_rest_alone() {
        let mut draft = draft();

        draft.clear(SearchlightAction::Reload, Slot::Primary);

        assert!(draft.is_unbound(SearchlightAction::Reload));
        assert!(draft.bindings().get(SearchlightAction::Reload).is_empty());
        for &other in SearchlightAction::ALL {
            if other != SearchlightAction::Reload {
                assert_eq!(
                    draft.bindings().get(other),
                    other.default_bindings(),
                    "{other:?} is untouched"
                );
            }
        }
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
