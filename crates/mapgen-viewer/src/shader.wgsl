// Vertex passes 3D world position through a view-projection matrix;
// fragment computes a face normal from screen-space derivatives of
// world position (gives flat per-triangle shading without paying for
// per-vertex normals on the CPU) and applies a simple diffuse + ambient
// lighting model. Stage 1 — replaces the previous flat-color shader.

struct CameraUniform {
    view_proj: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) world_pos: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.color = in.color;
    out.world_pos = in.position;
    return out;
}

// Light direction in world space: roughly noon-sun-ish from the
// north-west, with a strong vertical component so flat ocean isn't
// completely black. Same convention as the camera: +Y up.
const LIGHT_DIR: vec3<f32> = vec3<f32>(-0.4, 0.85, 0.35);
const AMBIENT: f32 = 0.55;
const DIFFUSE_STRENGTH: f32 = 0.45;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Face normal from the gradient of world position across this
    // fragment's pixel quad. abs() keeps backfaces (cull is off) lit
    // the same as front-faces, which is what we want for terrain.
    let n = normalize(cross(dpdx(in.world_pos), dpdy(in.world_pos)));
    let l = normalize(LIGHT_DIR);
    let diffuse = abs(dot(n, l));
    let intensity = AMBIENT + DIFFUSE_STRENGTH * diffuse;
    return vec4<f32>(in.color * intensity, 1.0);
}
