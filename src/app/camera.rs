//! Camera initialization and configuration.
//!
//! This module provides a builder pattern for camera setup with sensible defaults
//! for the voxel terrain view. The camera configuration includes:
//! - Eye position (camera location)
//! - Center/look-at target
//! - Field of view (FOV)
//! - Aspect ratio
//! - Near and far clipping planes
//!
//! # Design
//!
//! The camera is represented as a tuple of (view_matrix, projection_matrix, eye_position)
//! which matches the existing App structure. This can be refactored later to use a
//! dedicated Camera type from moho_types if needed.

use glam::{Mat4, Vec3};

/// Camera configuration result: (view_matrix, projection_matrix, eye_position)
pub type CameraSetup = (Mat4, Mat4, Vec3);

/// Builder for camera configuration with sensible defaults.
///
/// # Examples
///
/// ```
/// use moho::app::camera::CameraBuilder;
///
/// // Use default camera (good view of voxel terrain)
/// let camera = CameraBuilder::default().build();
///
/// // Customize camera position
/// let camera = CameraBuilder::default()
///     .with_eye(glam::Vec3::new(50.0, 30.0, 50.0))
///     .with_center(glam::Vec3::new(0.0, 10.0, 0.0))
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct CameraBuilder {
    /// Camera position in world space
    eye: Vec3,

    /// Point the camera is looking at
    center: Vec3,

    /// Up vector (typically +Y)
    up: Vec3,

    /// Field of view in degrees
    fov_degrees: f32,

    /// Aspect ratio (width / height)
    aspect_ratio: f32,

    /// Near clipping plane
    near: f32,

    /// Far clipping plane
    far: f32,
}

impl Default for CameraBuilder {
    /// Create a camera with default settings for voxel terrain viewing.
    ///
    /// Default position: (40, 25, 40) looking at (0, 8, 0)
    /// FOV: 45 degrees, Aspect: 16:9, Near: 0.1, Far: 1500
    fn default() -> Self {
        Self {
            eye: Vec3::new(40.0, 25.0, 40.0),
            center: Vec3::new(0.0, 8.0, 0.0),
            up: Vec3::new(0.0, 1.0, 0.0),
            fov_degrees: 45.0,
            aspect_ratio: 16.0 / 9.0,
            near: 0.1,
            far: 1500.0, // Increased for skybox visibility
        }
    }
}

impl CameraBuilder {
    /// Create a new camera builder with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the camera position (eye point).
    pub fn with_eye(mut self, eye: Vec3) -> Self {
        self.eye = eye;
        self
    }

    /// Set the look-at target (center point).
    pub fn with_center(mut self, center: Vec3) -> Self {
        self.center = center;
        self
    }

    /// Set the up vector (default is +Y).
    pub fn with_up(mut self, up: Vec3) -> Self {
        self.up = up;
        self
    }

    /// Set the field of view in degrees.
    pub fn with_fov_degrees(mut self, fov_degrees: f32) -> Self {
        self.fov_degrees = fov_degrees;
        self
    }

    /// Set the aspect ratio (width / height).
    pub fn with_aspect_ratio(mut self, aspect_ratio: f32) -> Self {
        self.aspect_ratio = aspect_ratio;
        self
    }

    /// Set the near clipping plane.
    pub fn with_near(mut self, near: f32) -> Self {
        self.near = near;
        self
    }

    /// Set the far clipping plane.
    pub fn with_far(mut self, far: f32) -> Self {
        self.far = far;
        self
    }

    /// Build the camera setup.
    ///
    /// Returns: (view_matrix, projection_matrix, eye_position)
    pub fn build(self) -> CameraSetup {
        let view = Mat4::look_at_rh(self.eye, self.center, self.up);
        let proj = Mat4::perspective_rh(
            self.fov_degrees.to_radians(),
            self.aspect_ratio,
            self.near,
            self.far,
        );

        (view, proj, self.eye)
    }
}

/// Create a camera with default settings.
///
/// This is a convenience function equivalent to `CameraBuilder::default().build()`.
pub fn create_default_camera() -> CameraSetup {
    CameraBuilder::default().build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_camera_values() {
        let builder = CameraBuilder::default();

        assert_eq!(builder.eye, Vec3::new(40.0, 25.0, 40.0));
        assert_eq!(builder.center, Vec3::new(0.0, 8.0, 0.0));
        assert_eq!(builder.up, Vec3::new(0.0, 1.0, 0.0));
        assert_eq!(builder.fov_degrees, 45.0);
        assert_eq!(builder.aspect_ratio, 16.0 / 9.0);
        assert_eq!(builder.near, 0.1);
        assert_eq!(builder.far, 1500.0);
    }

    #[test]
    fn test_camera_build_creates_matrices() {
        let (view, proj, eye) = CameraBuilder::default().build();

        // Verify matrices are not NaN or infinite
        assert!(!view.is_nan(), "View matrix should be valid");
        assert!(!proj.is_nan(), "Projection matrix should be valid");

        // Verify eye position is preserved
        assert_eq!(eye, Vec3::new(40.0, 25.0, 40.0));
    }

    #[test]
    fn test_camera_builder_customization() {
        let custom_eye = Vec3::new(100.0, 50.0, 100.0);
        let custom_center = Vec3::new(0.0, 0.0, 0.0);

        let (_, _, eye) = CameraBuilder::default()
            .with_eye(custom_eye)
            .with_center(custom_center)
            .with_fov_degrees(60.0)
            .build();

        assert_eq!(eye, custom_eye);
    }

    #[test]
    fn test_create_default_camera() {
        let (view, proj, eye) = create_default_camera();

        // Should be identical to builder default
        let (view2, proj2, eye2) = CameraBuilder::default().build();

        assert_eq!(view, view2);
        assert_eq!(proj, proj2);
        assert_eq!(eye, eye2);
    }

    #[test]
    fn test_camera_builder_chaining() {
        // Verify all builder methods can be chained
        let camera = CameraBuilder::new()
            .with_eye(Vec3::new(1.0, 2.0, 3.0))
            .with_center(Vec3::new(4.0, 5.0, 6.0))
            .with_up(Vec3::new(0.0, 1.0, 0.0))
            .with_fov_degrees(90.0)
            .with_aspect_ratio(1.0)
            .with_near(0.5)
            .with_far(2000.0)
            .build();

        let (_, _, eye) = camera;
        assert_eq!(eye, Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn test_camera_matrices_are_different() {
        // View and projection matrices should not be identity
        let (view, proj, _) = CameraBuilder::default().build();

        let identity = Mat4::IDENTITY;
        assert_ne!(view, identity, "View matrix should not be identity");
        assert_ne!(proj, identity, "Projection matrix should not be identity");
    }
}
