//! The `gilrs` backend: the only module that names `gilrs` types.

use crate::action_map::ActionMap;
use crate::bindings::Action;
use crate::pad::{PadButton, Stick, StickAxis};

/// Connected gamepads, all feeding one action map.
pub struct Gamepads {
    gilrs: gilrs::Gilrs,
}

impl Gamepads {
    /// Starts the backend; `None` plus one warning when it cannot start.
    pub fn new() -> Option<Self> {
        Self::from_backend(gilrs::Gilrs::new())
    }

    /// `None` plus exactly one warning when the backend failed to start.
    pub fn from_backend(_backend: Result<gilrs::Gilrs, gilrs::Error>) -> Option<Self> {
        todo!()
    }

    /// Drains pending events into the map.
    pub fn poll<A: Action>(&mut self, _map: &mut ActionMap<A>) {
        let _ = &self.gilrs;
        todo!()
    }
}

pub(crate) fn map_button(_button: gilrs::Button) -> Option<PadButton> {
    todo!()
}

pub(crate) fn map_axis(_axis: gilrs::Axis) -> Option<(Stick, StickAxis)> {
    todo!()
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
}
