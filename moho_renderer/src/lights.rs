/// Dynamic light management system for point and spot lights
/// Handles light registration, updates, frustum culling, and GPU buffer management
use glam::{Mat4, Vec3, Vec4};

use crate::gpu_types::{DynamicLightsGpu, MAX_DYNAMIC_LIGHTS, PointLightGpu};

/// Dynamic light shape (point vs directional spot)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightShape {
    /// Omnidirectional point light
    Point,
    /// Directional spotlight (future — not yet implemented)
    Spot,
}

/// CPU-side light representation with full parameters
#[derive(Debug, Clone)]
pub struct Light {
    /// Unique identifier for this light
    pub id: u32,
    /// Light shape (point or spot)
    pub light_shape: LightShape,
    /// World position
    pub position: Vec3,
    /// Light color (RGB)
    pub color: Vec3,
    /// Light intensity multiplier
    pub intensity: f32,
    /// Maximum range (distance attenuation)
    pub range: f32,
    /// Whether this light is currently active
    pub enabled: bool,

    // Spot light parameters (future use)
    /// Spot light direction (for spotlight)
    pub direction: Option<Vec3>,
    /// Inner cone angle in radians (for spotlight)
    pub inner_angle: Option<f32>,
    /// Outer cone angle in radians (for spotlight)
    pub outer_angle: Option<f32>,
}

impl Light {
    /// Create a new point light
    pub fn new_point(position: Vec3, color: Vec3, intensity: f32, range: f32) -> Self {
        static NEXT_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        Self {
            id,
            light_shape: LightShape::Point,
            position,
            color,
            intensity,
            range,
            enabled: true,
            direction: None,
            inner_angle: None,
            outer_angle: None,
        }
    }

    /// Convert to GPU representation
    fn to_gpu(&self) -> PointLightGpu {
        PointLightGpu {
            position_range: [
                self.position.x,
                self.position.y,
                self.position.z,
                self.range,
            ],
            color_intensity: [self.color.x, self.color.y, self.color.z, self.intensity],
        }
    }

    /// Check if the light's bounding sphere intersects the camera frustum.
    ///
    /// Uses Gribb-Hartmann frustum plane extraction: if the sphere is
    /// entirely behind any of the 6 frustum planes, the light is culled.
    pub fn is_in_frustum(&self, view_proj: &Mat4) -> bool {
        let planes = extract_frustum_planes(view_proj);
        let pos = Vec4::new(self.position.x, self.position.y, self.position.z, 1.0);

        for plane in &planes {
            // Signed distance from light center to plane
            let dist = plane.dot(pos);
            if dist < -self.range {
                return false; // Entirely outside this half-space
            }
        }
        true
    }
}

/// Light manager handles collections of dynamic lights
#[derive(Debug)]
pub struct LightManager {
    /// All registered lights
    lights: Vec<Light>,
    /// Culled lights (visible this frame)
    culled_lights: Vec<usize>,
    /// GPU buffer data
    gpu_data: DynamicLightsGpu,
    /// Whether GPU data needs updating
    dirty: bool,
}

impl LightManager {
    /// Create a new light manager
    pub fn new() -> Self {
        Self {
            lights: Vec::new(),
            culled_lights: Vec::new(),
            gpu_data: DynamicLightsGpu::default(),
            dirty: true,
        }
    }

    /// Add a new light and return its ID
    pub fn add_light(&mut self, light: Light) -> u32 {
        let id = light.id;
        self.lights.push(light);
        self.dirty = true;
        id
    }

    /// Remove a light by ID
    pub fn remove_light(&mut self, id: u32) -> bool {
        if let Some(pos) = self.lights.iter().position(|l| l.id == id) {
            self.lights.remove(pos);
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Get a mutable reference to a light by ID
    pub fn get_light_mut(&mut self, id: u32) -> Option<&mut Light> {
        self.dirty = true; // Assume mutation will occur
        self.lights.iter_mut().find(|l| l.id == id)
    }

    /// Get an immutable reference to a light by ID
    pub fn get_light(&self, id: u32) -> Option<&Light> {
        self.lights.iter().find(|l| l.id == id)
    }

    /// Update light position
    pub fn set_light_position(&mut self, id: u32, position: Vec3) {
        if let Some(light) = self.get_light_mut(id) {
            light.position = position;
        }
    }

    /// Enable or disable a light
    pub fn set_light_enabled(&mut self, id: u32, enabled: bool) {
        if let Some(light) = self.get_light_mut(id) {
            light.enabled = enabled;
        }
    }

    /// Perform frustum culling on all lights
    /// Updates internal culled_lights list with visible light indices
    pub fn cull_lights(&mut self, view_proj: &Mat4) {
        self.culled_lights.clear();

        for (idx, light) in self.lights.iter().enumerate() {
            if !light.enabled {
                continue;
            }

            if light.is_in_frustum(view_proj) {
                self.culled_lights.push(idx);
            }
        }

        self.dirty = true;
    }

    /// Update GPU buffer with culled lights
    /// Should be called after frustum culling
    pub fn update_gpu_data(&mut self) {
        if !self.dirty {
            return;
        }

        // Limit to MAX_DYNAMIC_LIGHTS
        let num_lights = self.culled_lights.len().min(MAX_DYNAMIC_LIGHTS);

        // Update light count
        self.gpu_data.light_count[0] = num_lights as u32;

        // Copy culled lights to GPU buffer
        for (dst_idx, &src_idx) in self.culled_lights.iter().take(num_lights).enumerate() {
            self.gpu_data.lights[dst_idx] = self.lights[src_idx].to_gpu();
        }

        // Zero out remaining slots (not strictly necessary but good practice)
        for dst_idx in num_lights..MAX_DYNAMIC_LIGHTS {
            self.gpu_data.lights[dst_idx] = PointLightGpu::default();
        }

        self.dirty = false;
    }

    /// Get GPU data for uploading to buffer
    pub fn gpu_data(&self) -> &DynamicLightsGpu {
        &self.gpu_data
    }

    /// Get number of visible lights after culling
    pub fn visible_light_count(&self) -> usize {
        self.culled_lights.len().min(MAX_DYNAMIC_LIGHTS)
    }

    /// Get total number of registered lights
    pub fn total_light_count(&self) -> usize {
        self.lights.len()
    }

    /// Mark as dirty (forces GPU update next frame)
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }
}

impl Default for LightManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract the 6 frustum planes from a view-projection matrix (Gribb-Hartmann method).
///
/// Each returned `Vec4` encodes a plane `(A, B, C, D)` such that
/// `Ax + By + Cz + D >= 0` for points inside the frustum.
/// The planes are normalized so that `(A,B,C).length() == 1`,
/// making `plane.dot(point)` the signed distance from the plane.
fn extract_frustum_planes(vp: &Mat4) -> [Vec4; 6] {
    let row = |i: usize| Vec4::new(vp.col(0)[i], vp.col(1)[i], vp.col(2)[i], vp.col(3)[i]);

    let r0 = row(0);
    let r1 = row(1);
    let r2 = row(2);
    let r3 = row(3);

    let mut planes = [
        r3 + r0, // Left
        r3 - r0, // Right
        r3 + r1, // Bottom
        r3 - r1, // Top
        r3 + r2, // Near  (wgpu uses depth [0,1] but adding works for both)
        r3 - r2, // Far
    ];

    for p in &mut planes {
        let len = Vec3::new(p.x, p.y, p.z).length();
        if len > 1e-8 {
            *p /= len;
        }
    }

    planes
}
