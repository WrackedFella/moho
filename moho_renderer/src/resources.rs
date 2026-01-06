//! Resource management for the renderer.
//!
//! This module handles:
//! - Buffer creation (camera, lighting, material, instance, vertex)
//! - Bind group creation
//! - Texture creation (depth texture)
//! - Initial resource allocation
//!
//! # Architecture
//!
//! The `ResourcePool` struct encapsulates all GPU resources needed by the renderer.
//! It's created during initialization and provides methods to create and manage
//! buffers, textures, and bind groups throughout the renderer's lifetime.

use wgpu::util::DeviceExt;

use crate::MaterialGpu;
use crate::gpu_types::LightingGpu;
use crate::types::GpuInstance;

/// Encapsulates all GPU resources for the renderer.
///
/// This struct holds buffers, textures, and bind groups created during initialization.
/// Resources can be recreated or resized as needed during the renderer's lifetime.
pub struct ResourcePool {
    /// Camera uniform buffer (view + projection matrices)
    pub camera_buffer: wgpu::Buffer,

    /// Lighting uniform buffer (sun/moon/ambient lighting)
    pub lighting_buffer: wgpu::Buffer,

    /// Material storage buffer (array of materials)
    pub material_buffer: wgpu::Buffer,

    /// Camera bind group (combines camera, materials, and lighting)
    pub camera_bind_group: wgpu::BindGroup,

    /// Depth texture for depth testing
    pub depth_texture: wgpu::Texture,

    /// Depth texture view for rendering
    pub depth_texture_view: wgpu::TextureView,

    /// Instance buffer (per-instance data)
    pub instance_buffer: wgpu::Buffer,

    /// Current capacity of instance buffer
    pub instance_capacity: usize,

    /// Skybox vertex buffer (fullscreen quad)
    pub skybox_vertex_buffer: wgpu::Buffer,

    /// Number of vertices in skybox
    pub skybox_vertex_count: u32,
}

impl ResourcePool {
    /// Create a new resource pool with all initial resources.
    ///
    /// This creates:
    /// - Camera buffer (80 bytes for view/proj matrices)
    /// - Lighting buffer (96 bytes for sun/moon/ambient with defaults)
    /// - Material buffer (32 bytes for 1 initial white material)
    /// - Camera bind group (combines above 3 buffers)
    /// - Depth texture (matches surface dimensions)
    /// - Instance buffer (1 element to avoid special cases)
    /// - Skybox vertex buffer (6 vertices for fullscreen quad)
    ///
    /// # Arguments
    ///
    /// * `device` - The wgpu device to create resources on
    /// * `camera_bgl` - The camera bind group layout
    /// * `depth_format` - The depth texture format
    /// * `surface_width` - Width of the rendering surface
    /// * `surface_height` - Height of the rendering surface
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use moho_renderer::resources::ResourcePool;
    /// # use wgpu::{Device, BindGroupLayout, TextureFormat};
    /// # fn example(device: &Device, camera_bgl: &BindGroupLayout) {
    /// let resources = ResourcePool::new(
    ///     device,
    ///     camera_bgl,
    ///     TextureFormat::Depth24Plus,
    ///     1920,
    ///     1080
    /// );
    /// # }
    /// ```
    pub fn new(
        device: &wgpu::Device,
        camera_bgl: &wgpu::BindGroupLayout,
        depth_format: wgpu::TextureFormat,
        surface_width: u32,
        surface_height: u32,
    ) -> Self {
        // Create buffers
        let camera_buffer = Self::create_camera_buffer(device);
        let lighting_buffer = Self::create_lighting_buffer(device);
        let material_buffer = Self::create_initial_material_buffer(device);

        // Create placeholder SSAO texture and sampler for initial bind group
        // (Will be replaced when SSAO system is initialized)
        let placeholder_ssao_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("placeholder-ssao-texture"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let placeholder_ssao_view =
            placeholder_ssao_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let placeholder_ssao_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("placeholder-ssao-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Create placeholder dynamic lights buffer (will be replaced when renderer initializes light manager)
        use crate::gpu_types::DynamicLightsGpu;
        let placeholder_dynamic_lights = DynamicLightsGpu::default();
        let placeholder_dynamic_lights_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("placeholder-dynamic-lights-buffer"),
                contents: bytemuck::bytes_of(&placeholder_dynamic_lights),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            });

        // Create camera bind group
        let camera_bind_group = Self::create_camera_bind_group(
            device,
            camera_bgl,
            &camera_buffer,
            &material_buffer,
            &lighting_buffer,
            &placeholder_ssao_view,
            &placeholder_ssao_sampler,
            &placeholder_dynamic_lights_buffer,
        );

        // Create depth texture
        let (depth_texture, depth_texture_view) =
            Self::create_depth_texture(device, depth_format, surface_width, surface_height);

        // Create instance buffer
        let (instance_buffer, instance_capacity) = Self::create_initial_instance_buffer(device);

        // Create skybox vertex buffer
        let (skybox_vertex_buffer, skybox_vertex_count) = Self::create_skybox_vertex_buffer(device);

        Self {
            camera_buffer,
            lighting_buffer,
            material_buffer,
            camera_bind_group,
            depth_texture,
            depth_texture_view,
            instance_buffer,
            instance_capacity,
            skybox_vertex_buffer,
            skybox_vertex_count,
        }
    }

    /// Create the camera uniform buffer.
    ///
    /// Size: 80 bytes (mat4x4 view + mat4x4 proj + vec4 cam_pos = 16+16+4 floats = 36 floats but padded to 20 floats for alignment)
    fn create_camera_buffer(device: &wgpu::Device) -> wgpu::Buffer {
        let camera_size = std::mem::size_of::<[f32; 20]>() as u64;
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera-buffer"),
            size: camera_size as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Create the lighting uniform buffer with default lighting.
    ///
    /// Size: 96 bytes (6 vec4s: sun_dir, sun_col, moon_dir, moon_col, ambient, time_of_day)
    fn create_lighting_buffer(device: &wgpu::Device) -> wgpu::Buffer {
        let initial_lighting = LightingGpu::default();
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("lighting-buffer"),
            contents: bytemuck::bytes_of(&initial_lighting),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        })
    }

    /// Create the initial material storage buffer with one white material.
    ///
    /// This creates a single-element material buffer so we can create the bind group
    /// immediately. It will be replaced when the app uploads real materials.
    ///
    /// Size: 32 bytes per material (2 vec4s: albedo + params)
    fn create_initial_material_buffer(device: &wgpu::Device) -> wgpu::Buffer {
        let initial_material = MaterialGpu {
            albedo: [1.0, 1.0, 1.0, 0.0],
            params: [0.0, 0.0, 0.0, 0.0],
        };
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("material-buffer-initial"),
            contents: bytemuck::bytes_of(&initial_material),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        })
    }

    /// Create the camera bind group.
    ///
    /// This combines buffers and SSAO textures:
    /// - Binding 0: Camera uniform buffer
    /// - Binding 1: Material storage buffer
    /// - Binding 2: Lighting uniform buffer
    /// - Binding 3: SSAO texture (placeholder initially)
    /// - Binding 4: SSAO sampler (placeholder initially)
    fn create_camera_bind_group(
        device: &wgpu::Device,
        camera_bgl: &wgpu::BindGroupLayout,
        camera_buffer: &wgpu::Buffer,
        material_buffer: &wgpu::Buffer,
        lighting_buffer: &wgpu::Buffer,
        ssao_texture_view: &wgpu::TextureView,
        ssao_sampler: &wgpu::Sampler,
        dynamic_lights_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: camera_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: material_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: lighting_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(ssao_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(ssao_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: dynamic_lights_buffer.as_entire_binding(),
                },
            ],
            label: Some("camera-bind-group"),
        })
    }

    /// Create the depth texture and view.
    ///
    /// Format: Depth24Plus (24-bit depth, no stencil)
    /// Usage: Render attachment + texture binding (for SSAO)
    fn create_depth_texture(
        device: &wgpu::Device,
        depth_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth-texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: depth_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());
        (depth_texture, depth_view)
    }

    /// Create the initial instance buffer with one element.
    ///
    /// This creates a tiny initial buffer to avoid special cases in the rendering code.
    /// It will be resized as needed when instances are uploaded.
    ///
    /// Returns: (buffer, capacity)
    fn create_initial_instance_buffer(device: &wgpu::Device) -> (wgpu::Buffer, usize) {
        let initial_instance = GpuInstance {
            model: [[0.0; 4]; 4],
            material: 0,
            object_type: 0,
            padding: [0, 0],
        };
        let instance_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("instance-buffer-initial"),
            contents: bytemuck::cast_slice(&[initial_instance]),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        (instance_buf, 1)
    }

    /// Create the skybox vertex buffer.
    ///
    /// Generates a fullscreen quad (two triangles covering the screen) for skybox rendering.
    /// The skybox shader will project this into a sphere using the inverse view-projection matrix.
    ///
    /// Returns: (buffer, vertex_count)
    fn create_skybox_vertex_buffer(device: &wgpu::Device) -> (wgpu::Buffer, u32) {
        let (skybox_vertices, skybox_vertex_count) = Self::generate_skybox_quad();
        log::info!(
            "Generated skybox fullscreen quad with {} vertices",
            skybox_vertex_count
        );
        let skybox_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("skybox-vertex-buffer"),
            contents: bytemuck::cast_slice(&skybox_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        (skybox_vertex_buffer, skybox_vertex_count)
    }

    /// Generate a fullscreen quad for skybox rendering.
    ///
    /// Returns vertices in clip space coordinates [-1, 1] covering the entire screen.
    /// Two triangles are used to form the quad.
    ///
    /// Returns: (vertices, vertex_count)
    fn generate_skybox_quad() -> (Vec<[f32; 3]>, u32) {
        let vertices = vec![
            // First triangle (bottom-left, top-left, bottom-right)
            [-1.0, -1.0, 0.0], // Bottom-left
            [-1.0, 1.0, 0.0],  // Top-left
            [1.0, -1.0, 0.0],  // Bottom-right
            // Second triangle (bottom-right, top-left, top-right)
            [1.0, -1.0, 0.0], // Bottom-right
            [-1.0, 1.0, 0.0], // Top-left
            [1.0, 1.0, 0.0],  // Top-right
        ];
        let vertex_count = vertices.len() as u32;
        (vertices, vertex_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skybox_quad_generation() {
        let (vertices, count) = ResourcePool::generate_skybox_quad();
        assert_eq!(count, 6, "Skybox quad should have 6 vertices (2 triangles)");
        assert_eq!(vertices.len(), 6);

        // Check that vertices cover clip space
        assert_eq!(vertices[0], [-1.0, -1.0, 0.0]); // Bottom-left
        assert_eq!(vertices[2], [1.0, -1.0, 0.0]); // Bottom-right
        assert_eq!(vertices[5], [1.0, 1.0, 0.0]); // Top-right
    }

    #[test]
    fn test_camera_buffer_size() {
        // Camera buffer should be 80 bytes (20 floats * 4 bytes/float)
        let expected_size = std::mem::size_of::<[f32; 20]>() as u64;
        assert_eq!(expected_size, 80);
    }

    #[test]
    fn test_initial_material() {
        let material = MaterialGpu {
            albedo: [1.0, 1.0, 1.0, 0.0],
            params: [0.0, 0.0, 0.0, 0.0],
        };

        // Should be white, non-transparent
        assert_eq!(material.albedo[0], 1.0);
        assert_eq!(material.albedo[1], 1.0);
        assert_eq!(material.albedo[2], 1.0);
        assert!(!material.is_transparent());
    }
}
