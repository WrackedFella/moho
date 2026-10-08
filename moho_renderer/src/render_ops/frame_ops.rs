//! Frame finalization operations.
//!
//! This module handles frame callbacks, command submission, and presentation.

use wgpu::{CommandEncoder, Queue, SurfaceTexture, TextureView};

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
    frame_callback: Option<&std::sync::Arc<std::sync::Mutex<dyn crate::FrameCallback>>>,
    surface_width: u32,
    surface_height: u32,
    device: &wgpu::Device,
    draw_count: usize,
) {
    // Call frame callback if registered (for UI rendering, etc.)
    if let (Some(cb_arc), Some(view)) = (frame_callback, pending_frame_view) {
        tracing::debug!("[frame_ops] calling frame_callback_arc");
        if let Ok(mut guard) = cb_arc.lock() {
            guard.call(
                device,
                queue,
                view,
                &mut encoder,
                surface_width,
                surface_height,
            );
            tracing::debug!("[frame_ops] frame_callback_arc returned");
        } else {
            tracing::warn!("[frame_ops] failed to lock frame_callback_arc");
        }
    }

    // Submit command buffer and present frame
    let finished = encoder.finish();
    tracing::debug!(count = draw_count, "[frame_ops] submitting draws");
    if let Some(frame) = pending_frame {
        queue.submit(Some(finished));
        frame.present();
    }
}
