//! Rendering without a window: device creation and texture readback.
//!
//! Used by tests, the command-line tools and anything that needs pictures of the model without a user
//! interface. Interactive applications should use the device of their windowing layer instead.

use std::time::Duration;

use crate::RenderError;

/// A GPU device created without a window.
#[derive(Debug)]
pub struct HeadlessGpu {
    /// The adapter.
    pub adapter: wgpu::Adapter,
    /// The device.
    pub device: wgpu::Device,
    /// The queue.
    pub queue: wgpu::Queue,
}

impl HeadlessGpu {
    /// Creates a device on the best available adapter, falling back to a software adapter when no hardware one is
    /// available. Honours the `WGPU_BACKEND` and related environment variables. Blocks until ready.
    pub fn new() -> Result<HeadlessGpu, RenderError> {
        pollster::block_on(Self::new_async())
    }

    /// Asynchronous form of [`HeadlessGpu::new`].
    pub async fn new_async() -> Result<HeadlessGpu, RenderError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let options = |fallback| wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: fallback,
            compatible_surface: None,
            apply_limit_buckets: false,
        };
        let adapter = match instance.request_adapter(&options(false)).await {
            Ok(a) => a,
            Err(first) => instance
                .request_adapter(&options(true))
                .await
                .map_err(|e| RenderError::NoAdapter(format!("{first}; software fallback: {e}")))?,
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("kanavos headless"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
                ..wgpu::DeviceDescriptor::default()
            })
            .await
            .map_err(|e| RenderError::Gpu(e.to_string()))?;
        Ok(HeadlessGpu { adapter, device, queue })
    }

    /// Human-readable adapter description, e.g. for test logs.
    pub fn adapter_description(&self) -> String {
        let info = self.adapter.get_info();
        format!("{} ({:?}, {:?})", info.name, info.backend, info.device_type)
    }
}

/// An RGBA image in memory, 8 bits per channel, sRGB, rows top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Pixel data, `width * height * 4` bytes.
    pub rgba: Vec<u8>,
}

impl Image {
    /// The pixel at (x, y), or `None` outside the image.
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = ((y as usize) * (self.width as usize) + x as usize) * 4;
        match self.rgba.get(i..i + 4) {
            Some(&[r, g, b, a]) => Some([r, g, b, a]),
            _ => None,
        }
    }
}

/// Copies an `Rgba8Unorm`/`Rgba8UnormSrgb` texture into memory. Blocks until the GPU has finished.
pub fn read_rgba8(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
) -> Result<Image, RenderError> {
    let (width, height) = (texture.width(), texture.height());
    let unpadded = width * 4;
    let pitch = unpadded.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("kanavos readback"),
        size: u64::from(pitch) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("kanavos readback") });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(pitch),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    let submission = queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer.map_async(wgpu::MapMode::Read, .., move |r| {
        let _ = tx.send(r);
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(30)),
        })
        .map_err(|e| RenderError::Gpu(e.to_string()))?;
    match rx.recv_timeout(Duration::from_secs(30)) {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(RenderError::Readback(e.to_string())),
        Err(e) => return Err(RenderError::Readback(e.to_string())),
    }
    let mut rgba = Vec::with_capacity((unpadded * height) as usize);
    {
        let view = buffer.get_mapped_range(..).map_err(|e| RenderError::Readback(e.to_string()))?;
        for row in view.chunks(pitch as usize).take(height as usize) {
            rgba.extend_from_slice(row.get(..unpadded as usize).unwrap_or(row));
        }
    }
    buffer.unmap();
    Ok(Image { width, height, rgba })
}
