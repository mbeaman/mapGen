//! Real-time wgpu 3D explorer over generated worlds.
//!
//! See `docs/adr/0002-live-3d-explorer.md` for architectural decisions,
//! and `docs/SESSION.md` "Currently in flight" for the active stage.
//!
//! Stage 0c.2: native-only substrate, flat-coloured world rendering, and
//! an orbit camera driven by mouse/keyboard input. The [`Renderer`] owns
//! the wgpu surface + device, an optional [`scene::WorldScene`] (built
//! when a world is loaded via [`Renderer::load_world`]), and an optional
//! [`camera::OrbitCamera`] (constructed alongside the scene to frame the
//! loaded world). Each frame the renderer pushes the camera's
//! view-projection matrix to the GPU and draws the scene inside the same
//! render pass that clears to a parchment tone.
//!
//! Input is plumbed in via thin methods on [`Renderer`]
//! ([`pan`](Renderer::pan), [`orbit`](Renderer::orbit),
//! [`zoom`](Renderer::zoom), [`reset_camera`](Renderer::reset_camera));
//! the binary turns winit events into these calls so this crate stays
//! winit-agnostic at the camera/scene layer.

mod camera;
mod scene;
mod screenshot;

pub use screenshot::{screenshot, screenshot_with, Pose};

use std::sync::Arc;

use anyhow::Result;
use camera::OrbitCamera;
use mapgen_core::world_data::WorldData;
use scene::WorldScene;
use winit::dpi::PhysicalSize;
use winit::window::Window;

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: PhysicalSize<u32>,
    scene: Option<WorldScene>,
    camera: Option<OrbitCamera>,
}

impl Renderer {
    pub async fn new(window: Arc<Window>) -> Result<Self> {
        let raw = window.inner_size();
        let size = PhysicalSize::new(raw.width.max(1), raw.height.max(1));

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });

        let surface = instance.create_surface(window.clone())?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("mapgen-viewer device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: wgpu::ExperimentalFeatures::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            })
            .await?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width,
            height: size.height,
            present_mode: caps.present_modes[0],
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            size,
            scene: None,
            camera: None,
        })
    }

    pub fn load_world(&mut self, world: &WorldData) {
        self.scene = Some(WorldScene::build(&self.device, &self.config, world));
        self.camera = Some(OrbitCamera::fit_world(
            world.mesh.width,
            world.mesh.height,
            self.aspect(),
        ));
    }

    pub fn size(&self) -> PhysicalSize<u32> {
        self.size
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.size = new_size;
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        self.surface.configure(&self.device, &self.config);
        let aspect = aspect_of(new_size);
        if let Some(cam) = self.camera.as_mut() {
            cam.set_aspect(aspect);
        }
    }

    pub fn pan(&mut self, dx_px: f32, dy_px: f32) {
        if let Some(cam) = self.camera.as_mut() {
            cam.pan(dx_px, dy_px, self.size.height as f32);
        }
    }

    pub fn orbit(&mut self, dx_px: f32, dy_px: f32) {
        if let Some(cam) = self.camera.as_mut() {
            cam.orbit(dx_px, dy_px);
        }
    }

    pub fn zoom(&mut self, lines: f32) {
        if let Some(cam) = self.camera.as_mut() {
            cam.zoom(lines);
        }
    }

    pub fn reset_camera(&mut self) {
        if let Some(cam) = self.camera.as_mut() {
            cam.reset();
        }
    }

    fn aspect(&self) -> f32 {
        aspect_of(self.size)
    }

    pub fn render(&mut self) -> Result<()> {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                log::warn!("surface validation error; skipping frame");
                return Ok(());
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Push the current camera state to the GPU before recording the
        // pass — wgpu serialises buffer writes against the same queue
        // submission, so the draw below sees this matrix.
        if let (Some(scene), Some(cam)) = (self.scene.as_ref(), self.camera.as_ref()) {
            scene.update_view_proj(&self.queue, &cam.view_proj().to_cols_array_2d());
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame encoder"),
            });

        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("clear + world pass"),
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
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some(scene) = self.scene.as_ref() {
                scene.draw(&mut rpass);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        Ok(())
    }
}

fn aspect_of(size: PhysicalSize<u32>) -> f32 {
    size.width as f32 / size.height.max(1) as f32
}
