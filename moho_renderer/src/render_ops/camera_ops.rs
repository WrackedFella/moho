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
/// * `cut` - View-height cut, from `view_height_uniform`
///
/// # GPU Layout
/// The camera buffer is laid out as:
/// - 16 floats: view-projection matrix (column-major)
/// - 4 floats: camera position + view-height cut
pub fn update_camera_uniforms(
    queue: &Queue,
    camera_buffer: &Buffer,
    view_mat: Mat4,
    proj_mat: Mat4,
    cam_pos: Vec3,
    cut: f32,
) {
    // Debug: log camera values
    tracing::trace!(
        cam_pos = ?cam_pos,
        viewproj0 = ?(proj_mat * view_mat).to_cols_array()[0],
        "[camera_ops] update"
    );

    let data = camera_uniform_data(view_mat, proj_mat, cam_pos, cut);
    queue.write_buffer(camera_buffer, 0, bytemuck::cast_slice(&data));
}

/// Camera uniform contents: view-projection (column-major), then camera
/// position with the view-height cut in `w`.
fn camera_uniform_data(view_mat: Mat4, proj_mat: Mat4, cam_pos: Vec3, cut: f32) -> [f32; 20] {
    let mut data = [0.0f32; 20];
    data[..16].copy_from_slice(&(proj_mat * view_mat).to_cols_array());
    data[16..].copy_from_slice(&[cam_pos.x, cam_pos.y, cam_pos.z, cut]);
    data
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::camera::rh::{proj::directx::perspective, view::look_at_mat4};

    #[test]
    fn camera_uniform_carries_the_cut_in_cam_pos_w() {
        let view = look_at_mat4(Vec3::new(1.0, 2.0, 3.0), Vec3::ZERO, Vec3::Y);
        let proj = perspective(1.0, 1.5, 0.1, 100.0);

        let data = camera_uniform_data(view, proj, Vec3::new(1.0, 2.0, 3.0), 12.5);

        assert_eq!(data[16..19], [1.0, 2.0, 3.0]);
        assert_eq!(data[19], 12.5);
        assert_eq!(data[..16], (proj * view).to_cols_array());
    }
}
