//! Frame finalization operations.
//!
//! This module handles frame callbacks, command submission, and presentation.

use wgpu::{CommandEncoder, Queue, SurfaceTexture, TextureView};

/// Frame callback wrapper for safe access to frame callbacks.
///
/// The renderer supports two types of callbacks:
/// - Arc<Mutex<dyn FrameCallback>> (safe, preferred)
/// - *mut dyn FrameCallback (raw pointer, unsafe)
pub enum FrameCallbackWrapper<'a> {
    Arc(&'a std::sync::Arc<std::sync::Mutex<dyn crate::FrameCallback>>),
    Raw(*mut dyn crate::FrameCallback),
    None,
}

/// Finish rendering and present the frame.
///
/// This handles:
/// 1. Frame callbacks (UI rendering, etc.)
/// 2. Submitting the command buffer to the GPU
/// 3. Presenting the surface texture
///
/// # Arguments
/// * `encoder` - Command encoder with recorded commands
/// * `queue` - WGPU queue for submitting commands
/// * `pending_frame` - Surface texture to present (consumed)
/// * `pending_frame_view` - View into the surface texture for callbacks
/// * `frame_callback` - Optional callback for additional rendering (UI, etc.)
/// * `surface_width` - Surface width for callback
/// * `surface_height` - Surface height for callback
/// * `device` - WGPU device for callback access
///
/// # Returns
/// Number of draws that were submitted
#[allow(clippy::too_many_arguments)] // Render operations naturally have many parameters
pub fn finish_frame(
    mut encoder: CommandEncoder,
    queue: &Queue,
    pending_frame: Option<SurfaceTexture>,
    pending_frame_view: Option<&TextureView>,
    frame_callback: FrameCallbackWrapper,
    surface_width: u32,
    surface_height: u32,
    device: &wgpu::Device,
    draw_count: usize,
) {
    // Call frame callback if registered (for UI rendering, etc.)
    match frame_callback {
        FrameCallbackWrapper::Arc(cb_arc) => {
            log::debug!("[frame_ops] calling frame_callback_arc");
            if let Some(view) = pending_frame_view {
                if let Ok(mut guard) = cb_arc.lock() {
                    guard.call(
                        device,
                        queue,
                        view,
                        &mut encoder,
                        surface_width,
                        surface_height,
                    );
                    log::debug!("[frame_ops] frame_callback_arc returned");
                } else {
                    log::warn!("[frame_ops] failed to lock frame_callback_arc");
                }
            }
        }
        FrameCallbackWrapper::Raw(cb_ptr) => unsafe {
            log::info!("[frame_ops] calling frame_callback_raw");
            if let Some(view) = pending_frame_view {
                let cb: &mut dyn crate::FrameCallback = &mut *cb_ptr;
                cb.call(
                    device,
                    queue,
                    view,
                    &mut encoder,
                    surface_width,
                    surface_height,
                );
                log::info!("[frame_ops] frame_callback_raw returned");
            }
        },
        FrameCallbackWrapper::None => {
            // No callback registered
        }
    }

    // Submit command buffer and present frame
    let finished = encoder.finish();
    log::debug!("[frame_ops] submitting {} draws", draw_count);
    if let Some(frame) = pending_frame {
        queue.submit(Some(finished));
        frame.present();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_callback_wrapper_none() {
        // Test that None variant exists and can be matched
        let wrapper = FrameCallbackWrapper::None;
        match wrapper {
            FrameCallbackWrapper::None => {
                // Success
            }
            _ => panic!("Expected None variant"),
        }
    }

    #[test]
    fn test_frame_callback_wrapper_size() {
        // Verify wrapper size is reasonable (should be pointer-sized)
        let size = std::mem::size_of::<FrameCallbackWrapper>();
        // Should be at most 24 bytes (two pointers + discriminant on 64-bit)
        assert!(size <= 24, "FrameCallbackWrapper size: {} bytes", size);
    }
}
