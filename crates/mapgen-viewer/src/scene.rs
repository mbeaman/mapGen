//! World → GPU mesh conversion + the world render pipeline.
//!
//! Each Voronoi cell is fan-triangulated from its centre site. The biome
//! colour is written to every emitted vertex (so cells read as
//! flat-coloured polygons — no per-vertex blend). World 2D `(x, y)` is
//! laid out on the XZ ground plane, then lifted along +Y by the cell's
//! elevation (Stage 1): land cells rise above the plane; sea cells stay
//! at `y = 0`. Each *shared* mesh vertex uses the **average** of touching
//! cells' (clamped-to-zero) elevations, so adjacent cells meet smoothly
//! instead of forming vertical walls at every cell boundary. Cell
//! centres keep the cell's own elevation, so each cell still reads as a
//! slight dome with its own colour.
//!
//! Lighting is done in the fragment shader via screen-space derivatives
//! of world position — that gives a true per-triangle face normal
//! without paying for per-vertex normals on the CPU side, and naturally
//! produces a faceted "topographic relief" look that suits the
//! biome-coloured aesthetic.
//!
//! The camera uniform is updated each frame from
//! [`crate::camera::OrbitCamera`] via [`WorldScene::update_view_proj`];
//! the scene itself has no opinion about the matrix it holds.

use bytemuck::{Pod, Zeroable};
use mapgen_core::world_data::{MeshData, WorldData};
use mapgen_world::biomes;
use wgpu::util::DeviceExt;

/// World units of vertical lift per unit of elevation. Seed-42 land
/// runs roughly `[0, 1]` (p99 ≈ 0.8); at 150 the highest peaks rise
/// ~150 world units — about 7% of world width, ~12% of world depth.
/// Visible from the default ~60° tilt without dominating the frame.
const HEIGHT_SCALE: f32 = 150.0;

/// Depth buffer format used by [`WorldScene`]. The screenshot path and
/// the interactive renderer must agree, so it lives here as a single
/// source of truth.
pub(crate) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct Vertex {
    /// World-space `(x, elevation, y)`. Elevation is stubbed to 0 in
    /// Stage 0c.2; Stage 1 fills it in from `terrain.elevation`.
    position: [f32; 3],
    color: [f32; 3],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
}

pub(crate) struct WorldScene {
    pipeline: wgpu::RenderPipeline,
    vertex_buf: wgpu::Buffer,
    vertex_count: u32,
    camera_buf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl WorldScene {
    pub(crate) fn build(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        world: &WorldData,
    ) -> Self {
        let vertices = build_vertices(world);
        let vertex_count = vertices.len() as u32;
        log::info!(
            "world scene: {} cells → {} vertices",
            world.mesh.sites.len(),
            vertex_count,
        );

        let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("world vertex buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        // Initialise with identity so a missed update_view_proj doesn't
        // crash — caller is expected to push a real matrix each frame.
        let initial = CameraUniform {
            view_proj: IDENTITY_4X4,
        };
        let camera_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("camera uniform"),
            contents: bytemuck::bytes_of(&initial),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera bind layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera bind group"),
            layout: &bind_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buf.as_entire_binding(),
            }],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("world shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("world pipeline layout"),
            bind_group_layouts: &[Some(&bind_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("world pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x3,
                        1 => Float32x3,
                    ],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        Self {
            pipeline,
            vertex_buf,
            vertex_count,
            camera_buf,
            bind_group,
        }
    }

    pub(crate) fn update_view_proj(&self, queue: &wgpu::Queue, view_proj: &[[f32; 4]; 4]) {
        let uniform = CameraUniform {
            view_proj: *view_proj,
        };
        queue.write_buffer(&self.camera_buf, 0, bytemuck::bytes_of(&uniform));
    }

    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
        pass.draw(0..self.vertex_count, 0..1);
    }
}

const IDENTITY_4X4: [[f32; 4]; 4] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

fn build_vertices(world: &WorldData) -> Vec<Vertex> {
    let mesh = &world.mesh;
    let elev = &world.terrain.elevation;
    let biome = &world.climate.biome;
    let vertex_height = per_vertex_height(mesh, elev);

    // Pre-size: most Voronoi cells have ~6 vertices → ~6 triangles → ~18 verts.
    let mut out = Vec::with_capacity(mesh.sites.len() * 18);

    for cell_id in 0..mesh.sites.len() {
        let poly = &mesh.cell_vertices[cell_id];
        if poly.len() < 3 {
            continue;
        }
        let cell_elev = elev.get(cell_id).copied().unwrap_or(0.0);
        // Sea floor stays at y = 0 so the ocean reads as a flat plane.
        let center_height = cell_elev.max(0.0) * HEIGHT_SCALE;
        let center = lift(mesh.sites[cell_id], center_height);
        let cell_biome = biome.get(cell_id).copied().unwrap_or(biomes::UNASSIGNED);
        let color = biome_color_rgb(cell_biome, cell_elev);

        // Fan from the cell centre. Winding doesn't matter — culling is off.
        for i in 0..poly.len() {
            let idx_a = poly[i] as usize;
            let idx_b = poly[(i + 1) % poly.len()] as usize;
            let a = lift(mesh.vertices[idx_a], vertex_height[idx_a]);
            let b = lift(mesh.vertices[idx_b], vertex_height[idx_b]);
            out.push(Vertex {
                position: center,
                color,
            });
            out.push(Vertex { position: a, color });
            out.push(Vertex { position: b, color });
        }
    }
    out
}

/// World 2D `(x, y)` → 3D `(x, height, y)`. Sea-level vertices use
/// `height = 0`; land vertices use their averaged lift.
fn lift(p: [f32; 2], height: f32) -> [f32; 3] {
    [p[0], height, p[1]]
}

/// For each *shared* mesh vertex, the mean of its touching cells'
/// land-elevations (negatives clamped to 0). A coastal vertex shared
/// between sea (elev<0) and land (elev>0) cells lands just above sea
/// level — so land slopes down to meet the ocean instead of dropping
/// off a cliff at every coastline.
fn per_vertex_height(mesh: &MeshData, elev: &[f32]) -> Vec<f32> {
    let n = mesh.vertices.len();
    let mut sums = vec![0.0f32; n];
    let mut counts = vec![0u32; n];
    for cell_id in 0..mesh.sites.len() {
        let h = elev.get(cell_id).copied().unwrap_or(0.0).max(0.0);
        for &v_idx in &mesh.cell_vertices[cell_id] {
            let i = v_idx as usize;
            sums[i] += h;
            counts[i] += 1;
        }
    }
    sums.iter()
        .zip(counts.iter())
        .map(|(&s, &c)| if c > 0 { s / c as f32 } else { 0.0 } * HEIGHT_SCALE)
        .collect()
}

/// Mirror of `mapgen_render::style::ornate_antique::ornate_biome_color`
/// (private there), translated to linear RGB floats. Stage 5 will replace
/// this with shader-side ornate treatment; for now we want the live view
/// to read at-a-glance like the SVG.
fn biome_color_rgb(biome: u8, elev: f32) -> [f32; 3] {
    if elev <= 0.0 {
        return if elev < -0.3 {
            hex(0x6a, 0x85, 0xa0)
        } else {
            hex(0x9b, 0xb5, 0xc8)
        };
    }
    match biome {
        biomes::SNOW => hex(0xe8, 0xe2, 0xd4),
        biomes::TUNDRA => hex(0xbf, 0xbc, 0xa8),
        biomes::TAIGA => hex(0x5e, 0x7a, 0x55),
        biomes::TEMPERATE_FOREST => hex(0x7a, 0x9e, 0x6e),
        biomes::TEMPERATE_GRASSLAND => hex(0xc2, 0xb8, 0x70),
        biomes::TEMPERATE_RAINFOREST => hex(0x5d, 0x8a, 0x65),
        biomes::DESERT => hex(0xdc, 0xc0, 0x80),
        biomes::SAVANNA => hex(0xc9, 0xa5, 0x5a),
        biomes::TROPICAL_RAINFOREST => hex(0x4a, 0x8a, 0x4a),
        biomes::TROPICAL_DRY_FOREST => hex(0x9a, 0xa0, 0x50),
        biomes::SHRUBLAND => hex(0xa5, 0x98, 0x68),
        biomes::ALPINE => hex(0x9a, 0x9a, 0x8a),
        biomes::RIPARIAN => hex(0x6a, 0x9a, 0x4a),
        biomes::WETLAND => hex(0x7e, 0x94, 0x83),
        _ => hex(0xb8, 0xa8, 0x80),
    }
}

fn hex(r: u8, g: u8, b: u8) -> [f32; 3] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
}
