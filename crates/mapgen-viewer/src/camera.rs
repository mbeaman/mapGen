//! Orbit camera (Stage 0c.2).
//!
//! Looks at `target` from a sphere of radius `distance`, yawing around
//! world-up and pitching above the ground plane. World is laid out on
//! the XZ plane (Y is up), so the camera orbits a point on that plane.
//!
//! Coordinate convention: world's 2D `(x, y)` maps to 3D `(x, 0, y)`
//! — see [`crate::scene::WorldScene`]. That way `world_y` flows into
//! `world_z`, and the camera's "up" is the global +Y. Stage 1 will lift
//! land cells along +Y by their elevation; sea cells stay on the plane.
//!
//! Sign conventions for input:
//! - [`pan`](OrbitCamera::pan) takes raw cursor pixel deltas (+dx right,
//!   +dy down per winit) and moves the camera so the world drifts under
//!   the cursor.
//! - [`orbit`](OrbitCamera::orbit) takes cursor pixel deltas; +dx swings
//!   the camera to the right around the target, +dy tilts it up.
//! - [`zoom`](OrbitCamera::zoom) takes a scroll-wheel delta in "lines"
//!   (winit's [`MouseScrollDelta::LineDelta`] units); positive zooms in.

use glam::{Mat4, Vec3};

const MIN_PITCH: f32 = 0.05;
const MAX_PITCH: f32 = std::f32::consts::FRAC_PI_2 - 0.05;
const Z_NEAR: f32 = 1.0;
const FOV_Y: f32 = std::f32::consts::FRAC_PI_4;

/// Sensitivity knobs. Tuned by feel on a 1280×800 window; if a future
/// reader wants different defaults, change here rather than scattering
/// constants through the input plumbing.
const ORBIT_RADIANS_PER_PIXEL: f32 = 0.005;
const ZOOM_FACTOR_PER_LINE: f32 = 0.1;

#[derive(Clone, Debug)]
pub(crate) struct OrbitCamera {
    target: Vec3,
    distance: f32,
    yaw: f32,
    pitch: f32,
    aspect: f32,
    /// Snapshot of the initial pose so [`reset`](Self::reset) can
    /// restore the framing the binary chose at startup.
    home: HomePose,
}

#[derive(Clone, Copy, Debug)]
struct HomePose {
    target: Vec3,
    distance: f32,
    yaw: f32,
    pitch: f32,
}

impl OrbitCamera {
    /// Frame the world: centre target on the world's middle, sit the
    /// camera high enough to see the whole map with a modest tilt so
    /// the 3D-ness is immediately legible (otherwise the first frame
    /// looks identical to the 0c.1 top-down view and the user can't
    /// tell whether input is working).
    pub(crate) fn fit_world(world_w: f32, world_h: f32, aspect: f32) -> Self {
        let target = Vec3::new(world_w * 0.5, 0.0, world_h * 0.5);
        // Fit the world's *vertical* extent into the screen at FOV_Y,
        // with some margin. Horizontal extent is handled by aspect on
        // the projection side; if the window is narrower than the
        // world's aspect, sides clip — pan/zoom from there.
        let distance = (world_h * 0.5) / (FOV_Y * 0.5).tan() * 1.4;
        let yaw = 0.0;
        let pitch = 1.05; // ≈60°: high enough to read like a map, low enough to feel 3D.
        Self {
            target,
            distance,
            yaw,
            pitch,
            aspect,
            home: HomePose {
                target,
                distance,
                yaw,
                pitch,
            },
        }
    }

    pub(crate) fn set_aspect(&mut self, aspect: f32) {
        self.aspect = aspect.max(0.01);
    }

    pub(crate) fn reset(&mut self) {
        self.target = self.home.target;
        self.distance = self.home.distance;
        self.yaw = self.home.yaw;
        self.pitch = self.home.pitch;
    }

    /// Eye position derived from `target + spherical(yaw, pitch, distance)`.
    fn eye(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        self.target
            + Vec3::new(
                self.distance * cp * sy,
                self.distance * sp,
                self.distance * cp * cy,
            )
    }

    pub(crate) fn view_proj(&self) -> Mat4 {
        let eye = self.eye();
        // Z-far scales with distance so a zoomed-out view doesn't clip
        // the back of the world (and a zoomed-in view doesn't waste
        // depth precision).
        let z_far = (self.distance * 4.0).max(world_diag(&self.target) * 2.0);
        let view = Mat4::look_at_rh(eye, self.target, Vec3::Y);
        let proj = Mat4::perspective_rh(FOV_Y, self.aspect, Z_NEAR, z_far);
        proj * view
    }

    /// Drag-to-pan in the ground plane. `viewport_height` lets us
    /// translate cursor pixels into world units at the current distance
    /// (a perspective-foreshortening approximation — close enough that
    /// the world tracks the cursor without feeling laggy or fast).
    pub(crate) fn pan(&mut self, dx_px: f32, dy_px: f32, viewport_height: f32) {
        let world_per_pixel =
            (2.0 * self.distance * (FOV_Y * 0.5).tan()) / viewport_height.max(1.0);
        let dx_w = dx_px * world_per_pixel;
        let dy_w = dy_px * world_per_pixel;
        // Camera basis projected onto the ground plane.
        let (sy, cy) = self.yaw.sin_cos();
        let right = Vec3::new(cy, 0.0, -sy);
        let forward_xz = Vec3::new(-sy, 0.0, -cy);
        // Cursor right (+dx) → world should drift right under cursor → target moves left.
        // Cursor down (+dy) → world should drift down (toward viewer) → target moves toward camera.
        self.target -= right * dx_w;
        self.target += forward_xz * dy_w;
    }

    pub(crate) fn orbit(&mut self, dx_px: f32, dy_px: f32) {
        self.yaw -= dx_px * ORBIT_RADIANS_PER_PIXEL;
        self.pitch = (self.pitch - dy_px * ORBIT_RADIANS_PER_PIXEL).clamp(MIN_PITCH, MAX_PITCH);
    }

    pub(crate) fn zoom(&mut self, lines: f32) {
        let factor = (1.0 - lines * ZOOM_FACTOR_PER_LINE).clamp(0.5, 2.0);
        self.distance = (self.distance * factor).clamp(2.0, 100_000.0);
    }
}

/// Rough "world size" for z-far heuristics: distance from target to a
/// far corner of the world. Cheap proxy for "how big is the scene the
/// camera might be looking at."
fn world_diag(target: &Vec3) -> f32 {
    (target.x * target.x + target.z * target.z).sqrt() + target.length()
}
