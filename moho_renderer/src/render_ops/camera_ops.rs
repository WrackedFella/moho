//! Camera uniform buffer operations.
//!
//! This module handles updating camera uniform buffers with view-projection
//! matrices and camera position for shader access.

use glam::{Mat4, Vec3};
use wgpu::{Buffer, Queue};

/// Update camera uniform buffer with view-projection matrix and camera position.
///
/// # Arguments
/// * `queue` - WGPU queue for buffer writes
/// * `camera_buffer` - Camera uniform buffer to update
/// * `view_mat` - View matrix (camera transform)
/// * `proj_mat` - Projection matrix (perspective/orthographic)
/// * `cam_pos` - Camera position in world space
///
/// # GPU Layout
/// The camera buffer is laid out as:
/// - 16 floats: view-projection matrix (column-major)
/// - 4 floats: camera position + padding
pub fn update_camera_uniforms(
    queue: &Queue,
    camera_buffer: &Buffer,
    view_mat: Mat4,
    proj_mat: Mat4,
    cam_pos: Vec3,
) {
    let viewproj = proj_mat * view_mat;

    // Debug: log camera values
    log::trace!(
        "[camera_ops] update: cam_pos={:?} viewproj0={:?}",
        cam_pos,
        viewproj.to_cols_array()[0]
    );

    // Build camera uniform data: viewproj matrix + camera position
    let mut cols = viewproj.to_cols_array().to_vec();
    cols.push(cam_pos.x);
    cols.push(cam_pos.y);
    cols.push(cam_pos.z);
    cols.push(0.0f32); // padding for alignment

    queue.write_buffer(camera_buffer, 0, bytemuck::cast_slice(&cols));
}
