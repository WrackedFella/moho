//! The `gilrs` backend: the only module that names `gilrs` types.

use crate::action_map::ActionMap;
use crate::bindings::Action;
use crate::pad::{PadButton, Stick, StickAxis};

/// Connected gamepads, all feeding one action map.
pub struct Gamepads {
    gilrs: gilrs::Gilrs,
}

impl Gamepads {
    /// `None` plus one warning when the backend cannot start.
    pub fn new() -> Option<Self> {
        Self::from_backend(gilrs::Gilrs::new())
    }

    pub(crate) fn from_backend(backend: Result<gilrs::Gilrs, gilrs::Error>) -> Option<Self> {
        match backend {
            Ok(gilrs) => Some(Self { gilrs }),
            Err(error) => {
                tracing::warn!(%error, "gamepad support unavailable");
                None
            }
        }
    }

    pub fn poll<A: Action>(&mut self, map: &mut ActionMap<A>) {
        while let Some(gilrs::Event { event, .. }) = self.gilrs.next_event() {
            apply_event(event, map);
        }
    }
}

/// Feeds one backend event to the map; events the engine has no use for are ignored.
pub(crate) fn apply_event<A: Action>(event: gilrs::EventType, map: &mut ActionMap<A>) {
    match event {
        gilrs::EventType::ButtonPressed(button, _)
        | gilrs::EventType::ButtonReleased(button, _) => {
            if let Some(button) = map_button(button) {
                map.pad_button(button, matches!(event, gilrs::EventType::ButtonPressed(..)));
            }
        }
        gilrs::EventType::AxisChanged(axis, value, _) => {
            if let Some((stick, axis)) = map_axis(axis) {
                map.pad_axis(stick, axis, value);
            }
        }
        gilrs::EventType::Disconnected => map.pad_disconnected(),
        _ => {}
    }
}

pub(crate) fn map_button(button: gilrs::Button) -> Option<PadButton> {
    use gilrs::Button as B;
    Some(match button {
        B::South => PadButton::South,
        B::East => PadButton::East,
        B::North => PadButton::North,
        B::West => PadButton::West,
        B::LeftTrigger => PadButton::LeftBumper,
        B::LeftTrigger2 => PadButton::LeftTrigger,
        B::RightTrigger => PadButton::RightBumper,
        B::RightTrigger2 => PadButton::RightTrigger,
        B::Select => PadButton::Select,
        B::Start => PadButton::Start,
        B::Mode => PadButton::Mode,
        B::LeftThumb => PadButton::LeftThumb,
        B::RightThumb => PadButton::RightThumb,
        B::DPadUp => PadButton::DPadUp,
        B::DPadDown => PadButton::DPadDown,
        B::DPadLeft => PadButton::DPadLeft,
        B::DPadRight => PadButton::DPadRight,
        B::C | B::Z | B::Unknown => return None,
    })
}

pub(crate) fn map_axis(axis: gilrs::Axis) -> Option<(Stick, StickAxis)> {
    match axis {
        gilrs::Axis::LeftStickX => Some((Stick::LeftStick, StickAxis::X)),
        gilrs::Axis::LeftStickY => Some((Stick::LeftStick, StickAxis::Y)),
        gilrs::Axis::RightStickX => Some((Stick::RightStick, StickAxis::X)),
        gilrs::Axis::RightStickY => Some((Stick::RightStick, StickAxis::Y)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gilrs::{Axis, Button};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tracing::Level;

    struct WarnCounter(Arc<AtomicUsize>);

    impl tracing::Subscriber for WarnCounter {
        fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
            true
        }
        fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            tracing::span::Id::from_u64(1)
        }
        fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
        fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
        fn event(&self, event: &tracing::Event<'_>) {
            if *event.metadata().level() == Level::WARN {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        fn enter(&self, _: &tracing::span::Id) {}
        fn exit(&self, _: &tracing::span::Id) {}
    }

    #[test]
    fn backend_failure_yields_none() {
        let warnings = Arc::new(AtomicUsize::new(0));
        let failure = gilrs::Error::Other(Box::new(std::io::Error::other("no udev")));

        let pads = tracing::subscriber::with_default(WarnCounter(warnings.clone()), || {
            Gamepads::from_backend(Err(failure))
        });

        assert!(pads.is_none());
        assert_eq!(warnings.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn gilrs_buttons_map_to_pad_buttons() {
        let cases = [
            (Button::South, Some(PadButton::South)),
            (Button::East, Some(PadButton::East)),
            (Button::North, Some(PadButton::North)),
            (Button::West, Some(PadButton::West)),
            (Button::LeftTrigger, Some(PadButton::LeftBumper)),
            (Button::LeftTrigger2, Some(PadButton::LeftTrigger)),
            (Button::RightTrigger, Some(PadButton::RightBumper)),
            (Button::RightTrigger2, Some(PadButton::RightTrigger)),
            (Button::Select, Some(PadButton::Select)),
            (Button::Start, Some(PadButton::Start)),
            (Button::Mode, Some(PadButton::Mode)),
            (Button::LeftThumb, Some(PadButton::LeftThumb)),
            (Button::RightThumb, Some(PadButton::RightThumb)),
            (Button::DPadUp, Some(PadButton::DPadUp)),
            (Button::DPadDown, Some(PadButton::DPadDown)),
            (Button::DPadLeft, Some(PadButton::DPadLeft)),
            (Button::DPadRight, Some(PadButton::DPadRight)),
            (Button::C, None),
            (Button::Z, None),
            (Button::Unknown, None),
        ];

        for (button, expected) in cases {
            assert_eq!(map_button(button), expected, "{button:?}");
        }
        let mapped: std::collections::HashSet<_> = cases.iter().filter_map(|&(_, p)| p).collect();
        assert_eq!(mapped.len(), PadButton::ALL.len());
    }

    #[test]
    fn gilrs_axes_map_to_sticks() {
        let cases = [
            (Axis::LeftStickX, Some((Stick::LeftStick, StickAxis::X))),
            (Axis::LeftStickY, Some((Stick::LeftStick, StickAxis::Y))),
            (Axis::RightStickX, Some((Stick::RightStick, StickAxis::X))),
            (Axis::RightStickY, Some((Stick::RightStick, StickAxis::Y))),
            (Axis::LeftZ, None),
            (Axis::RightZ, None),
            (Axis::DPadX, None),
            (Axis::DPadY, None),
            (Axis::Unknown, None),
        ];

        for (axis, expected) in cases {
            assert_eq!(map_axis(axis), expected, "{axis:?}");
        }
    }
    #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
    enum PadAction {
        Jump,
    }

    impl Action for PadAction {
        const ALL: &'static [Self] = &[PadAction::Jump];

        fn name(self) -> &'static str {
            "jump"
        }

        fn default_bindings(self) -> &'static [crate::bindings::Binding] {
            &[crate::bindings::Binding::Pad(crate::pad::PadInput::Button(
                PadButton::South,
            ))]
        }
    }

    fn held_jump_map() -> ActionMap<PadAction> {
        let (bindings, _) =
            crate::bindings::ActionBindings::<PadAction>::load(&std::collections::BTreeMap::new());
        let mut map = ActionMap::new(bindings, 1.0);
        map.pad_button(PadButton::South, true);
        map.end_tick();
        map
    }

    #[test]
    fn disconnect_event_releases_pad_bindings() {
        let mut map = held_jump_map();

        apply_event(gilrs::EventType::Disconnected, &mut map);
        let frame = map.end_tick();

        assert!(frame.released(PadAction::Jump));
        assert!(!frame.held(PadAction::Jump));
    }

    #[test]
    fn ignored_event_changes_nothing() {
        let mut map = held_jump_map();

        apply_event(gilrs::EventType::Connected, &mut map);
        let frame = map.end_tick();

        assert!(frame.held(PadAction::Jump));
        assert!(!frame.released(PadAction::Jump));
    }
}
