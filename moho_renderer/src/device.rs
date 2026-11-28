//! Device initialization for wgpu renderer
//!
//! This module handles the initialization of wgpu device resources including:
//! - Instance creation
//! - Surface creation from window
//! - Adapter selection
//! - Device and queue creation
//! - Surface configuration

use crate::block_on::block_on;
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
            DeviceInitError::SurfaceCreation(msg) => write!(f, "Surface creation failed: {msg}"),
            DeviceInitError::AdapterRequest(msg) => write!(f, "Adapter request failed: {msg}"),
            DeviceInitError::DeviceCreation(msg) => write!(f, "Device creation failed: {msg}"),
        }
    }
}

impl Error for DeviceInitError {}

/// The surface present mode picked for a vsync setting, and why vsync stays on
/// when the GPU offers no low-latency mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresentChoice {
    pub mode: wgpu::PresentMode,
    /// Set when vsync was turned off but the GPU has no mode that skips the display wait.
    pub fallback: Option<&'static str>,
}

/// Picks the present mode for `vsync` from the modes the surface `supported`.
///
/// On gives `Fifo`. Off gives `Immediate`, else `Mailbox`, else `Fifo` with a fallback reason.
pub fn choose_present_mode(vsync: bool, supported: &[wgpu::PresentMode]) -> PresentChoice {
    use wgpu::PresentMode::{Fifo, Immediate, Mailbox};

    if vsync {
        return PresentChoice {
            mode: Fifo,
            fallback: None,
        };
    }
    match [Immediate, Mailbox]
        .into_iter()
        .find(|mode| supported.contains(mode))
    {
        Some(mode) => PresentChoice {
            mode,
            fallback: None,
        },
        None => PresentChoice {
            mode: Fifo,
            fallback: Some("the GPU offers neither Immediate nor Mailbox presentation"),
        },
    }
}

/// Complete device setup including instance, surface, adapter, device, queue, and config
pub struct DeviceSetup<'a> {
    pub(crate) _instance: wgpu::Instance,
    pub(crate) surface: wgpu::Surface<'a>,
    pub(crate) present_modes: Vec<wgpu::PresentMode>,
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
    pub fn new(window: &'a winit::window::Window) -> Result<Self, DeviceInitError> {
        tracing::info!("Initializing wgpu device");

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
            .map_err(|e| DeviceInitError::SurfaceCreation(format!("{e:?}")))?;

        // 3. Request Adapter
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .map_err(|e| DeviceInitError::AdapterRequest(format!("{e:?}")))?;

        // 4. Prepare device features and limits
        let experimental = {
            #[cfg(feature = "wgpu-experimental")]
            #[allow(unsafe_code)]
            {
                // SAFETY: experimental wgpu features may be unsound or unimplemented on
                // some backends. Building with `wgpu-experimental` is the explicit opt-in
                // to that risk; default builds take the `disabled()` branch.
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
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: None,
            required_features,
            required_limits: limits,
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::default(),
            experimental_features: experimental,
        }))
        .map_err(|e| DeviceInitError::DeviceCreation(format!("{e:?}")))?;

        // 6. Configure Surface
        let capabilities = surface.get_capabilities(&adapter);
        let supported_formats = capabilities.formats;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: supported_formats[0],
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 0,
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &config);

        tracing::info!(
            format = ?config.format,
            width = config.width,
            height = config.height,
            "wgpu device initialized"
        );

        Ok(DeviceSetup {
            _instance: instance,
            surface,
            present_modes: capabilities.present_modes,
            device,
            queue,
            config,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::PresentMode;

    const ALL: [PresentMode; 4] = [
        PresentMode::Fifo,
        PresentMode::FifoRelaxed,
        PresentMode::Mailbox,
        PresentMode::Immediate,
    ];

    #[test]
    fn vsync_on_chooses_fifo() {
        let choice = choose_present_mode(true, &ALL);

        assert_eq!(choice.mode, PresentMode::Fifo);
        assert_eq!(choice.fallback, None);
    }

    #[test]
    fn vsync_off_prefers_immediate_then_mailbox() {
        let with_immediate = choose_present_mode(false, &ALL);
        let mailbox_only = choose_present_mode(false, &[PresentMode::Fifo, PresentMode::Mailbox]);

        assert_eq!(with_immediate.mode, PresentMode::Immediate);
        assert_eq!(with_immediate.fallback, None);
        assert_eq!(mailbox_only.mode, PresentMode::Mailbox);
        assert_eq!(mailbox_only.fallback, None);
    }

    #[test]
    fn vsync_off_without_immediate_or_mailbox_keeps_fifo_with_a_reason() {
        let choice = choose_present_mode(false, &[PresentMode::Fifo, PresentMode::FifoRelaxed]);

        assert_eq!(choice.mode, PresentMode::Fifo);
        assert!(choice.fallback.is_some_and(|reason| !reason.is_empty()));
    }

    #[test]
    fn vsync_off_never_picks_fifo_relaxed() {
        let choice = choose_present_mode(false, &[PresentMode::FifoRelaxed, PresentMode::Fifo]);

        assert_eq!(choice.mode, PresentMode::Fifo);
    }

    #[test]
    fn vsync_on_ignores_low_latency_modes_and_gives_no_reason() {
        let choice = choose_present_mode(true, &[PresentMode::Immediate, PresentMode::Fifo]);

        assert_eq!(
            choice,
            PresentChoice {
                mode: PresentMode::Fifo,
                fallback: None
            }
        );
    }
}
