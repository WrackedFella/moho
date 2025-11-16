//! Staged builder pattern for Renderer initialization.
//!
//! This module provides a type-safe, staged builder for constructing a [`Renderer`].
//! Each stage returns a different builder type, ensuring that initialization
//! happens in the correct order and that all required steps are completed.
//!
//! # Architecture
//!
//! The builder composes three major initialization modules:
//! - [`DeviceSetup`](crate::device::DeviceSetup) - wgpu device and surface initialization
//! - [`PipelineSetup`](crate::pipeline::PipelineSetup) - shader and pipeline creation
//! - [`ResourcePool`](crate::resources::ResourcePool) - GPU buffer and texture allocation
//!
//! # Builder Stages
//!
//! 1. **RendererBuilder** - Entry point, holds window reference
//! 2. **DeviceBuilder** - Device initialized, ready for pipeline creation
//! 3. **PipelineBuilder** - Pipelines created, ready for resource allocation
//! 4. **ResourceBuilder** - Resources allocated, ready to build final Renderer
//!
//! Each stage transition is a fallible operation that returns a `Result`,
//! allowing for clear error propagation throughout the initialization process.
//!
//! # Example
//!
//! ```no_run
//! use moho_renderer::RendererBuilder;
//! # use winit::window::Window;
//! # fn example(window: &Window) -> Result<(), Box<dyn std::error::Error>> {
//! let renderer = RendererBuilder::new(window)
//!     .init_device()?
//!     .create_pipelines()?
//!     .allocate_resources()?
//!     .build()?;
//! # Ok(())
//! # }
//! ```
//!
//! # Design Rationale
//!
//! The staged builder pattern ensures:
//! - **Type safety**: Each stage can only be called once in the correct order
//! - **Clear error handling**: Each stage returns a `Result` with specific error types
//! - **Composability**: Reuses existing DeviceSetup, PipelineSetup, ResourcePool modules
//! - **Testability**: Each stage can be tested independently
//! - **Discoverability**: IDE autocomplete guides users through required steps

use crate::shadow::ShadowSystem;
use crate::{Renderer, device::DeviceSetup, pipeline::PipelineSetup, resources::ResourcePool};

/// Entry point for staged renderer initialization.
///
/// Start here to create a new renderer. The builder pattern ensures that
/// initialization happens in the correct order.
pub struct RendererBuilder<'a> {
    window: &'a winit::window::Window,
}

/// Builder after device initialization.
///
/// At this stage, the wgpu device, queue, and surface are initialized and ready.
/// Next step: create rendering pipelines.
pub struct DeviceBuilder<'a> {
    window: &'a winit::window::Window,
    device_setup: DeviceSetup<'a>,
}

/// Builder after pipeline creation.
///
/// At this stage, shaders are loaded and render pipelines are created.
/// Next step: allocate GPU resources (buffers, textures).
pub struct PipelineBuilder<'a> {
    window: &'a winit::window::Window,
    device_setup: DeviceSetup<'a>,
    pipeline_setup: PipelineSetup,
}

/// Builder after resource allocation.
///
/// At this stage, all GPU resources are allocated and initialized.
/// Final step: construct the complete Renderer.
pub struct ResourceBuilder<'a> {
    window: &'a winit::window::Window,
    device_setup: DeviceSetup<'a>,
    pipeline_setup: PipelineSetup,
    resources: ResourcePool,
}

impl<'a> RendererBuilder<'a> {
    /// Create a new renderer builder.
    ///
    /// # Arguments
    ///
    /// * `window` - The window to render into. Must remain valid for the lifetime of the renderer.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use moho_renderer::RendererBuilder;
    /// # use winit::window::Window;
    /// # fn example(window: &Window) {
    /// let builder = RendererBuilder::new(window);
    /// # }
    /// ```
    pub fn new(window: &'a winit::window::Window) -> Self {
        Self { window }
    }

    /// Initialize the wgpu device, surface, and configuration.
    ///
    /// This stage:
    /// - Creates wgpu instance
    /// - Creates surface for the window
    /// - Requests graphics adapter
    /// - Creates device and queue
    /// - Configures surface (format, size, present mode)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Surface creation fails
    /// - No suitable adapter is found
    /// - Device creation fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// use moho_renderer::RendererBuilder;
    /// # use winit::window::Window;
    /// # fn example(window: &Window) -> Result<(), Box<dyn std::error::Error>> {
    /// let device_builder = RendererBuilder::new(window)
    ///     .init_device()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn init_device(self) -> Result<DeviceBuilder<'a>, Box<dyn std::error::Error>> {
        log::info!("(wgpu) Initializing renderer (instanced cubes)");
        let device_setup = DeviceSetup::new(self.window)?;
        Ok(DeviceBuilder {
            window: self.window,
            device_setup,
        })
    }
}

impl<'a> DeviceBuilder<'a> {
    /// Create rendering pipelines (shaders, layouts, pipelines).
    ///
    /// This stage:
    /// - Loads and compiles WGSL shaders
    /// - Creates bind group layouts (camera, shadow)
    /// - Creates pipeline layouts
    /// - Creates render pipelines (main, skybox)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Shader files cannot be read
    /// - Shader compilation fails
    /// - Pipeline creation fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// use moho_renderer::RendererBuilder;
    /// # use winit::window::Window;
    /// # fn example(window: &Window) -> Result<(), Box<dyn std::error::Error>> {
    /// let pipeline_builder = RendererBuilder::new(window)
    ///     .init_device()?
    ///     .create_pipelines()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn create_pipelines(self) -> Result<PipelineBuilder<'a>, Box<dyn std::error::Error>> {
        let pipeline_setup =
            PipelineSetup::new(&self.device_setup.device, &self.device_setup.config)?;
        Ok(PipelineBuilder {
            window: self.window,
            device_setup: self.device_setup,
            pipeline_setup,
        })
    }
}

impl<'a> PipelineBuilder<'a> {
    /// Allocate GPU resources (buffers, textures, bind groups).
    ///
    /// This stage:
    /// - Creates uniform buffers (camera, lighting)
    /// - Creates storage buffers (materials)
    /// - Creates vertex buffers (instance data, skybox)
    /// - Creates depth texture
    /// - Creates bind groups
    ///
    /// All resources are initialized with sensible defaults.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use moho_renderer::RendererBuilder;
    /// # use winit::window::Window;
    /// # fn example(window: &Window) -> Result<(), Box<dyn std::error::Error>> {
    /// let resource_builder = RendererBuilder::new(window)
    ///     .init_device()?
    ///     .create_pipelines()?
    ///     .allocate_resources()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn allocate_resources(self) -> Result<ResourceBuilder<'a>, Box<dyn std::error::Error>> {
        let resources = ResourcePool::new(
            &self.device_setup.device,
            self.pipeline_setup.camera_bind_group_layout(),
            self.pipeline_setup.depth_format(),
            self.device_setup.config.width,
            self.device_setup.config.height,
        );
        Ok(ResourceBuilder {
            window: self.window,
            device_setup: self.device_setup,
            pipeline_setup: self.pipeline_setup,
            resources,
        })
    }
}

impl<'a> ResourceBuilder<'a> {
    /// Build the final Renderer instance.
    ///
    /// This stage:
    /// - Initializes shadow system
    /// - Constructs Renderer with all initialized resources
    ///
    /// After this point, the renderer is ready to accept draw commands.
    ///
    /// # Errors
    ///
    /// Returns an error if shadow system initialization fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use moho_renderer::RendererBuilder;
    /// # use winit::window::Window;
    /// # fn example(window: &Window) -> Result<(), Box<dyn std::error::Error>> {
    /// let renderer = RendererBuilder::new(window)
    ///     .init_device()?
    ///     .create_pipelines()?
    ///     .allocate_resources()?
    ///     .build()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn build(self) -> Result<Renderer<'a>, Box<dyn std::error::Error>> {
        let camera_bgl = self.pipeline_setup.camera_bind_group_layout();
        let shadow_bind_group_layout = self.pipeline_setup.shadow_bind_group_layout();
        let shadow = ShadowSystem::new(
            &self.device_setup.device,
            camera_bgl,
            shadow_bind_group_layout,
        )?;

        Ok(Renderer::from_components(
            self.window,
            self.device_setup,
            &self.pipeline_setup,
            self.resources,
            shadow,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_compiles() {
        // This test verifies that the builder API compiles correctly
        // Actual rendering tests require a GPU and are in integration tests
        fn _example_usage(
            window: &winit::window::Window,
        ) -> Result<(), Box<dyn std::error::Error>> {
            let _renderer = RendererBuilder::new(window)
                .init_device()?
                .create_pipelines()?
                .allocate_resources()?
                .build()?;
            Ok(())
        }
    }
}
