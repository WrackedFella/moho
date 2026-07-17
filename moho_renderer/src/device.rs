//! Device initialization for wgpu renderer
//!
//! This module handles the initialization of wgpu device resources including:
//! - Instance creation
//! - Surface creation from window
//! - Adapter selection
//! - Device and queue creation
//! - Surface configuration
//!
//! # Example
//! ```no_run
//! use moho_renderer::device::DeviceSetup;
//!
//! // Given a winit window:
//! // let window: &winit::window::Window = ...;
//! // let setup = DeviceSetup::new(window)?;
//! ```

use std::error::Error;
use std::fmt;

/// Error types that can occur during device initialization
#[derive(Debug)]
pub enum DeviceInitError {
    /// Failed to create surface from window
    SurfaceCreation(String),
    /// Failed to find suitable adapter
    AdapterRequest(String),
    /// Failed to create device
    DeviceCreation(String),
}

impl fmt::Display for DeviceInitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceInitError::SurfaceCreation(msg) => write!(f, "Surface creation failed: {}", msg),
            DeviceInitError::AdapterRequest(msg) => write!(f, "Adapter request failed: {}", msg),
            DeviceInitError::DeviceCreation(msg) => write!(f, "Device creation failed: {}", msg),
        }
    }
}

impl Error for DeviceInitError {}

/// Complete device setup including instance, surface, adapter, device, queue, and config
pub struct DeviceSetup<'a> {
    pub(crate) _instance: wgpu::Instance,
    pub(crate) surface: wgpu::Surface<'a>,
    pub(crate) _adapter: wgpu::Adapter,
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) config: wgpu::SurfaceConfiguration,
}

impl<'a> DeviceSetup<'a> {
    /// Initialize wgpu device from a window
    ///
    /// This performs the complete device initialization sequence:
    /// 1. Create wgpu Instance with all available backends
    /// 2. Create Surface from the window
    /// 3. Request Adapter with high performance preference
    /// 4. Request Device and Queue with optional push constants feature
    /// 5. Configure Surface with appropriate format and present mode
    ///
    /// # Arguments
    /// * `window` - Reference to winit window with same lifetime as DeviceSetup
    ///
    /// # Returns
    /// * `Ok(DeviceSetup)` - Complete device setup ready for rendering
    /// * `Err(DeviceInitError)` - If any initialization step fails
    ///
    /// # Example
    /// ```no_run
    /// # use moho_renderer::device::DeviceSetup;
    /// # let window: &winit::window::Window = todo!();
    /// let setup = DeviceSetup::new(window)?;
    /// // Use setup.device, setup.queue, setup.surface, setup.config
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn new(window: &'a winit::window::Window) -> Result<Self, DeviceInitError> {
        log::info!("Initializing wgpu device");

        let size = window.inner_size();

        // 1. Create Instance
        let instance_desc = wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        };
        let instance = wgpu::Instance::new(instance_desc);

        // 2. Create Surface
        let surface = instance
            .create_surface(window)
            .map_err(|e| DeviceInitError::SurfaceCreation(format!("{:?}", e)))?;

        // 3. Request Adapter
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .map_err(|e| DeviceInitError::AdapterRequest(format!("{:?}", e)))?;

        // 4. Prepare device features and limits
        let experimental = {
            #[cfg(feature = "wgpu-experimental")]
            {
                unsafe { wgpu::ExperimentalFeatures::enabled() }
            }
            #[cfg(not(feature = "wgpu-experimental"))]
            {
                wgpu::ExperimentalFeatures::disabled()
            }
        };

        // Only request features the adapter actually supports
        let desired_features = wgpu::Features::IMMEDIATES;
        let required_features = desired_features & adapter.features();

        // Set immediate-data size limit if feature is supported
        let mut limits = wgpu::Limits::default();
        if required_features.contains(wgpu::Features::IMMEDIATES) {
            limits.max_immediate_size = 128; // Minimum guaranteed by spec
        }

        // 5. Request Device and Queue
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: None,
            required_features,
            required_limits: limits,
            memory_hints: Default::default(),
            trace: Default::default(),
            experimental_features: experimental,
        }))
        .map_err(|e| DeviceInitError::DeviceCreation(format!("{:?}", e)))?;

        // 6. Configure Surface
        let supported_formats = surface.get_capabilities(&adapter).formats;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: supported_formats[0],
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 0,
        };
        surface.configure(&device, &config);

        log::info!(
            "wgpu device initialized: format={:?}, size={}x{}",
            config.format,
            config.width,
            config.height
        );

        Ok(DeviceSetup {
            _instance: instance,
            surface,
            _adapter: adapter,
            device,
            queue,
            config,
        })
    }

    /// Get the surface format
    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    /// Get the surface size as (width, height)
    pub fn surface_size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_init_error_display() {
        let surface_err = DeviceInitError::SurfaceCreation("test error".to_string());
        assert_eq!(
            surface_err.to_string(),
            "Surface creation failed: test error"
        );

        let adapter_err = DeviceInitError::AdapterRequest("no adapter".to_string());
        assert_eq!(
            adapter_err.to_string(),
            "Adapter request failed: no adapter"
        );

        let device_err = DeviceInitError::DeviceCreation("device failed".to_string());
        assert_eq!(
            device_err.to_string(),
            "Device creation failed: device failed"
        );
    }

    #[test]
    fn test_device_init_error_is_error() {
        let err = DeviceInitError::SurfaceCreation("test".to_string());
        let _: &dyn Error = &err; // Verify it implements Error trait
    }

    // Note: Actual device creation tests require GPU hardware and are in renderer_init.rs
}
