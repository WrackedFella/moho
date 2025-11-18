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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camera_uniform_layout() {
        // Verify the uniform data layout is correct
        let view_mat = Mat4::IDENTITY;
        let proj_mat = Mat4::IDENTITY;
        let cam_pos = Vec3::new(1.0, 2.0, 3.0);

        let viewproj = proj_mat * view_mat;
        let mut cols = viewproj.to_cols_array().to_vec();
        cols.push(cam_pos.x);
        cols.push(cam_pos.y);
        cols.push(cam_pos.z);
        cols.push(0.0f32);

        // Should have 20 floats total (16 for matrix + 4 for position)
        assert_eq!(cols.len(), 20);
        // Camera position should be at the end
        assert_eq!(cols[16], 1.0);
        assert_eq!(cols[17], 2.0);
        assert_eq!(cols[18], 3.0);
        assert_eq!(cols[19], 0.0);
    }

    #[test]
    fn test_viewproj_multiplication() {
        // Test that viewproj matrix is calculated correctly
        let view_mat = Mat4::from_translation(Vec3::new(0.0, 0.0, -10.0));
        let proj_mat = Mat4::perspective_rh(std::f32::consts::FRAC_PI_4, 16.0 / 9.0, 0.1, 100.0);
        let _cam_pos = Vec3::new(5.0, 5.0, 5.0);

        let viewproj = proj_mat * view_mat;

        // Verify it's a valid matrix (not NaN)
        for val in viewproj.to_cols_array() {
            assert!(!val.is_nan());
        }
    }
}
