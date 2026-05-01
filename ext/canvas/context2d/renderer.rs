// Copyright 2018-2026 the Deno authors. MIT license.
//
// Headless vello renderer used by CanvasRenderingContext2D when the canvas
// is backed by a CPU `DynamicImage`. A single `VelloDevice` is lazily
// constructed and shared across all 2d contexts in the runtime.

use std::sync::Arc;
use std::sync::OnceLock;

use deno_core::futures::executor::block_on;
use deno_error::JsErrorBox;
use vello::AaConfig;
use vello::AaSupport;
use vello::RenderParams;
use vello::Renderer;
use vello::RendererOptions;
use vello::Scene;
use vello::peniko::color::AlphaColor;
use vello::peniko::color::Srgb;
use vello::wgpu;

pub struct VelloDevice {
  pub device: wgpu::Device,
  pub queue: wgpu::Queue,
}

static DEVICE: OnceLock<Arc<VelloDevice>> = OnceLock::new();

pub fn shared_device() -> Result<Arc<VelloDevice>, JsErrorBox> {
  if let Some(d) = DEVICE.get() {
    return Ok(d.clone());
  }
  let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
  let adapter =
    block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
      power_preference: wgpu::PowerPreference::HighPerformance,
      force_fallback_adapter: false,
      compatible_surface: None,
    }))
    .ok_or_else(|| JsErrorBox::generic("vello: no suitable wgpu adapter"))?;
  let (device, queue) = block_on(adapter.request_device(
    &wgpu::DeviceDescriptor {
      label: Some("vello canvas2d"),
      required_features: wgpu::Features::empty(),
      required_limits: wgpu::Limits::default(),
      memory_hints: wgpu::MemoryHints::default(),
    },
    None,
  ))
  .map_err(|e| {
    JsErrorBox::generic(format!("vello: failed to request device: {e}"))
  })?;
  let dev = Arc::new(VelloDevice { device, queue });
  let _ = DEVICE.set(dev.clone());
  Ok(dev)
}

/// Render `scene` at `width`x`height` and return the resulting RGBA8 buffer.
/// `clear` is the background color the scene composites over.
pub fn render_scene_to_rgba(
  scene: &Scene,
  width: u32,
  height: u32,
  clear: AlphaColor<Srgb>,
) -> Result<Vec<u8>, JsErrorBox> {
  let dev = shared_device()?;
  let device = &dev.device;
  let queue = &dev.queue;

  let mut renderer = Renderer::new(
    device,
    RendererOptions {
      use_cpu: false,
      antialiasing_support: AaSupport::area_only(),
      num_init_threads: None,
      pipeline_cache: None,
    },
  )
  .map_err(|e| {
    JsErrorBox::generic(format!("vello: failed to create renderer: {e}"))
  })?;

  let texture = device.create_texture(&wgpu::TextureDescriptor {
    label: Some("vello canvas2d target"),
    size: wgpu::Extent3d {
      width,
      height,
      depth_or_array_layers: 1,
    },
    mip_level_count: 1,
    sample_count: 1,
    dimension: wgpu::TextureDimension::D2,
    format: wgpu::TextureFormat::Rgba8Unorm,
    usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
    view_formats: &[],
  });
  let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

  renderer
    .render_to_texture(
      device,
      queue,
      scene,
      &view,
      &RenderParams {
        base_color: clear,
        width,
        height,
        antialiasing_method: AaConfig::Area,
      },
    )
    .map_err(|e| JsErrorBox::generic(format!("vello: render failed: {e}")))?;

  // Copy texture -> buffer -> CPU. Rows must be aligned to
  // COPY_BYTES_PER_ROW_ALIGNMENT (256).
  let bytes_per_pixel = 4u32;
  let unpadded_bpr = width * bytes_per_pixel;
  let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
  let padded_bpr = unpadded_bpr.div_ceil(align) * align;
  let buffer_size = (padded_bpr as u64) * (height as u64);

  let buffer = device.create_buffer(&wgpu::BufferDescriptor {
    label: Some("vello canvas2d readback"),
    size: buffer_size,
    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
    mapped_at_creation: false,
  });

  let mut encoder =
    device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
      label: Some("vello canvas2d copy"),
    });
  encoder.copy_texture_to_buffer(
    wgpu::TexelCopyTextureInfo {
      texture: &texture,
      mip_level: 0,
      origin: wgpu::Origin3d::ZERO,
      aspect: wgpu::TextureAspect::All,
    },
    wgpu::TexelCopyBufferInfo {
      buffer: &buffer,
      layout: wgpu::TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(padded_bpr),
        rows_per_image: Some(height),
      },
    },
    wgpu::Extent3d {
      width,
      height,
      depth_or_array_layers: 1,
    },
  );
  queue.submit(Some(encoder.finish()));

  let slice = buffer.slice(..);
  let (tx, rx) = std::sync::mpsc::channel();
  slice.map_async(wgpu::MapMode::Read, move |r| {
    let _ = tx.send(r);
  });
  device.poll(wgpu::Maintain::Wait);
  rx.recv()
    .unwrap()
    .map_err(|e| JsErrorBox::generic(format!("vello: map failed: {e}")))?;

  let data = slice.get_mapped_range();
  let mut out = vec![0u8; (unpadded_bpr as usize) * (height as usize)];
  for row in 0..height as usize {
    let src = &data[row * padded_bpr as usize..][..unpadded_bpr as usize];
    let dst = &mut out[row * unpadded_bpr as usize..][..unpadded_bpr as usize];
    dst.copy_from_slice(src);
  }
  drop(data);
  buffer.unmap();
  Ok(out)
}
