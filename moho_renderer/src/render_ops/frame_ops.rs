//! Frame finalization operations.
//!
//! This module handles command submission and presentation.

use wgpu::{CommandEncoder, Queue, SurfaceTexture};

/// Submits the command buffer and presents the frame.
///
/// # Arguments
/// * `encoder` - Command encoder with recorded commands
/// * `queue` - WGPU queue for submitting commands
/// * `pending_frame` - Surface texture to present (consumed)
/// * `draw_count` - Number of draws that were recorded, for logging
pub fn finish_frame(
    encoder: CommandEncoder,
    queue: &Queue,
    pending_frame: Option<SurfaceTexture>,
    draw_count: usize,
) {
    // Submit command buffer and present frame
    let finished = encoder.finish();
    tracing::debug!(count = draw_count, "[frame_ops] submitting draws");
    if let Some(frame) = pending_frame {
        queue.submit(Some(finished));
        frame.present();
    }
}
