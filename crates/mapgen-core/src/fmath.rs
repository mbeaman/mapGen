//! Cross-platform deterministic float math. Every transcendental call in the
//! pipeline routes through `libm` so x86_64 native and wasm32 produce
//! bit-identical results.
//!
//! Hard rule across the workspace: never call `f32::sin` / `f64::sin` /
//! `powi` / etc. directly. Always go through this module.

#[inline]
pub fn sin(x: f32) -> f32 {
    libm::sinf(x)
}
#[inline]
pub fn cos(x: f32) -> f32 {
    libm::cosf(x)
}
#[inline]
pub fn sqrt(x: f32) -> f32 {
    libm::sqrtf(x)
}
#[inline]
pub fn pow(x: f32, y: f32) -> f32 {
    libm::powf(x, y)
}
#[inline]
pub fn exp(x: f32) -> f32 {
    libm::expf(x)
}
#[inline]
pub fn ln(x: f32) -> f32 {
    libm::logf(x)
}
#[inline]
pub fn floor(x: f32) -> f32 {
    libm::floorf(x)
}
#[inline]
pub fn ceil(x: f32) -> f32 {
    libm::ceilf(x)
}
#[inline]
pub fn round(x: f32) -> f32 {
    libm::roundf(x)
}
#[inline]
pub fn atan2(y: f32, x: f32) -> f32 {
    libm::atan2f(y, x)
}

#[inline]
pub fn hypot(dx: f32, dy: f32) -> f32 {
    sqrt(dx * dx + dy * dy)
}

#[inline]
pub fn clamp(x: f32, lo: f32, hi: f32) -> f32 {
    if x < lo {
        lo
    } else if x > hi {
        hi
    } else {
        x
    }
}

#[inline]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
