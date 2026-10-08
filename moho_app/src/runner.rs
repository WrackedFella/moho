//! The winit `ApplicationHandler` that drives a [`crate::Game`].

use winit::event::WindowEvent;

/// What the runner does with a window event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Run a frame; the game does not see the event.
    Frame,
    /// Resize the renderer, then forward the event.
    Resize { width: u32, height: u32 },
    /// Forward the event, then exit the loop.
    Close,
    /// Hand the event to the game unchanged.
    Forward,
}

/// Decides how the runner handles `event`, without needing a display.
pub fn route(event: &WindowEvent) -> Route {
    let _ = event;
    Route::Forward
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::dpi::{PhysicalPosition, PhysicalSize};

    #[test]
    fn resized_routes_to_resize_with_the_new_size() {
        let event = WindowEvent::Resized(PhysicalSize::new(1280, 720));

        assert_eq!(
            route(&event),
            Route::Resize {
                width: 1280,
                height: 720
            }
        );
    }

    #[test]
    fn resized_to_a_different_size_carries_that_size() {
        let event = WindowEvent::Resized(PhysicalSize::new(3, 9));

        assert_eq!(
            route(&event),
            Route::Resize {
                width: 3,
                height: 9
            }
        );
    }

    #[test]
    fn close_requested_routes_to_close() {
        assert_eq!(route(&WindowEvent::CloseRequested), Route::Close);
    }

    #[test]
    fn redraw_requested_routes_to_frame() {
        assert_eq!(route(&WindowEvent::RedrawRequested), Route::Frame);
    }

    #[test]
    fn other_window_events_route_to_forward() {
        let others = [
            WindowEvent::Focused(true),
            WindowEvent::Occluded(false),
            WindowEvent::Moved(PhysicalPosition::new(4, 5)),
        ];

        for event in &others {
            assert_eq!(route(event), Route::Forward, "{event:?}");
        }
    }
}
