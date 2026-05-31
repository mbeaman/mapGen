//! World → GPU mesh conversion + the Stage 0c.1 render pipeline.
//!
//! Each Voronoi cell is fan-triangulated from its centre site, with the
//! cell's biome colour written to every emitted vertex (so cells read as
//! flat-coloured polygons — no per-vertex blend). Coordinates stay in
//! world space (`[0, width] × [0, height]`); a column-major orthographic
//! matrix maps that to NDC with Y flipped (world +Y is down; NDC +Y is up).

use bytemuck::{Pod, Zeroable};
use mapgen_core::world_data::WorldData;
use mapgen_world::biomes;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 3],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct CameraUniform {
    proj: [[f32; 4]; 4],
}

pub(crate) struct WorldScene {
    pipeline: wgpu::RenderPipeline,
    vertex_buf: wgpu::Buffer,
    vertex_count: u32,
    /// Held alive so Stage 0c.2 can update the camera via
    /// [`wgpu::Queue::write_buffer`] when input moves the orbit camera.
    /// Not read directly in 0c.1 — the bind group references it.
    #[allow(dead_code)]
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

        let proj = ortho_world_to_ndc(world.mesh.width, world.mesh.height);
        let camera_uniform = CameraUniform { proj };
        let camera_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("camera uniform"),
            contents: bytemuck::bytes_of(&camera_uniform),
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
                        0 => Float32x2,
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
            depth_stencil: None,
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

    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
        pass.draw(0..self.vertex_count, 0..1);
    }
}

/// Column-major orthographic projection from world-space rect
/// `[0, width] × [0, height]` (origin top-left, +Y down) to NDC
/// `[-1, 1]²` (origin centre, +Y up). Z is collapsed to 0.
fn ortho_world_to_ndc(width: f32, height: f32) -> [[f32; 4]; 4] {
    let w = width.max(1.0);
    let h = height.max(1.0);
    // column-major layout: each [..; 4] is a column.
    [
        [2.0 / w, 0.0, 0.0, 0.0],
        [0.0, -2.0 / h, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [-1.0, 1.0, 0.0, 1.0],
    ]
}

fn build_vertices(world: &WorldData) -> Vec<Vertex> {
    let mesh = &world.mesh;
    let elev = &world.terrain.elevation;
    let biome = &world.climate.biome;

    // Pre-size: most Voronoi cells have ~6 vertices → ~6 triangles → ~18 verts.
    let mut out = Vec::with_capacity(mesh.sites.len() * 18);

    for cell_id in 0..mesh.sites.len() {
        let poly = &mesh.cell_vertices[cell_id];
        if poly.len() < 3 {
            continue;
        }
        let center = mesh.sites[cell_id];
        let cell_elev = elev.get(cell_id).copied().unwrap_or(0.0);
        let cell_biome = biome.get(cell_id).copied().unwrap_or(biomes::UNASSIGNED);
        let color = biome_color_rgb(cell_biome, cell_elev);

        // Fan from the cell centre. Winding doesn't matter — culling is off.
        for i in 0..poly.len() {
            let a = mesh.vertices[poly[i] as usize];
            let b = mesh.vertices[poly[(i + 1) % poly.len()] as usize];
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
