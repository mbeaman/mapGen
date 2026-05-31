//! Headless render-to-buffer for screenshots and image-diff tests.
//!
//! Uses the same [`WorldScene`](crate::scene::WorldScene) and
//! [`OrbitCamera`](crate::camera::OrbitCamera) the interactive viewer
//! does, but renders to an offscreen texture (no surface, no window),
//! then copies the texture back to CPU memory as tightly-packed RGBA.
//! The caller wraps the bytes into a PNG/JPEG/whatever it likes.
//!
//! Designed for: README hero images, doc examples, future CI smoke
//! tests that exercise the render pipeline without a display.

use anyhow::{anyhow, Result};
use mapgen_core::world_data::WorldData;

use crate::camera::OrbitCamera;
use crate::scene::{WorldScene, DEPTH_FORMAT};

/// Camera pose override for [`screenshot_with`]. Defaults match the
/// interactive viewer's startup framing: yaw 0 (looking along -Z),
/// pitch ≈ 1.05 rad (≈60° above the ground plane),
/// `distance_scale = 1.0` (matches `OrbitCamera::fit_world`).
#[derive(Clone, Copy, Debug)]
pub struct Pose {
    pub yaw: f32,
    pub pitch: f32,
    /// Multiplied with the auto-framed distance. `0.8` zooms in; `1.4`
    /// zooms out. Clamped above 0 internally.
    pub distance_scale: f32,
}

impl Default for Pose {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 1.05,
            distance_scale: 1.0,
        }
    }
}

/// Render `world` at `width × height` from the default framing and
/// return tightly-packed RGBA bytes (length `width * height * 4`).
/// Pixel format is sRGB-encoded; safe to hand straight to
/// `image::RgbaImage::from_raw`.
pub async fn screenshot(world: &WorldData, width: u32, height: u32) -> Result<Vec<u8>> {
    screenshot_with(world, width, height, Pose::default()).await
}

/// Like [`screenshot`] but with a caller-chosen camera pose.
pub async fn screenshot_with(
    world: &WorldData,
    width: u32,
    height: u32,
    pose: Pose,
) -> Result<Vec<u8>> {
    let width = width.max(1);
    let height = height.max(1);

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        })
        .await?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("screenshot device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            experimental_features: wgpu::ExperimentalFeatures::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
        })
        .await?;

    let format = wgpu::TextureFormat::Rgba8UnormSrgb;

    // Build the scene against a fake config that just carries the
    // target format — WorldScene only reads `config.format` from this.
    let fake_config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width,
        height,
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    };
    let scene = WorldScene::build(&device, &fake_config, world);

    let aspect = width as f32 / height as f32;
    let camera = OrbitCamera::fit_world(world.mesh.width, world.mesh.height, aspect).with_pose(
        pose.yaw,
        pose.pitch,
        pose.distance_scale,
    );
    scene.update_view_proj(&queue, &camera.view_proj().to_cols_array_2d());

    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("screenshot target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());

    let depth_tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("screenshot depth"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_view = depth_tex.create_view(&wgpu::TextureViewDescriptor::default());

    // wgpu requires copy rows aligned to COPY_BYTES_PER_ROW_ALIGNMENT (256).
    let unpadded_bpr = width * 4;
    let padded_bpr = align_to(unpadded_bpr, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("screenshot readback"),
        size: (padded_bpr as u64) * (height as u64),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("screenshot encoder"),
    });

    {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("screenshot pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.949,
                        g: 0.910,
                        b: 0.831,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        scene.draw(&mut rpass);
    }

    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &target,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
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

    queue.submit(std::iter::once(encoder.finish()));

    let slice = readback.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::PollType::wait_indefinitely())?;
    rx.recv()
        .map_err(|e| anyhow!("readback channel closed: {e}"))?
        .map_err(|e| anyhow!("buffer map failed: {e:?}"))?;

    let padded = slice.get_mapped_range();
    let mut out = Vec::with_capacity((unpadded_bpr * height) as usize);
    for row in 0..height as usize {
        let start = row * padded_bpr as usize;
        let end = start + unpadded_bpr as usize;
        out.extend_from_slice(&padded[start..end]);
    }
    drop(padded);
    readback.unmap();

    Ok(out)
}

fn align_to(value: u32, alignment: u32) -> u32 {
    value.div_ceil(alignment) * alignment
}
