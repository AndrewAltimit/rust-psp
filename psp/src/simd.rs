//! VFPU-accelerated SIMD math library for PSP.
//!
//! The PSP's Vector Floating Point Unit (VFPU) provides hardware-accelerated
//! operations on vectors (2/3/4 component) and 4x4 matrices. This module
//! exposes ready-to-use math functions built on top of the raw `vfpu_asm!`
//! macro.
//!
//! # Categories
//!
//! - **Vector operations**: lerp, dot product, normalize, cross product
//! - **Matrix operations**: multiply, transpose, transform
//! - **Color operations**: RGBA blending, HSV↔RGB conversion
//! - **Easing functions**: Quadratic, cubic, spring-damped interpolation

// ── Vector Types ────────────────────────────────────────────────────

/// A 4-component f32 vector, 16-byte aligned for VFPU register loads.
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec4(pub [f32; 4]);

impl Vec4 {
    pub const ZERO: Self = Self([0.0, 0.0, 0.0, 0.0]);
    pub const ONE: Self = Self([1.0, 1.0, 1.0, 1.0]);

    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self([x, y, z, w])
    }

    pub fn x(&self) -> f32 {
        self.0[0]
    }
    pub fn y(&self) -> f32 {
        self.0[1]
    }
    pub fn z(&self) -> f32 {
        self.0[2]
    }
    pub fn w(&self) -> f32 {
        self.0[3]
    }
}

/// A 4x4 f32 matrix, 16-byte aligned for VFPU matrix loads.
/// Stored in column-major order (matches OpenGL and GU conventions).
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat4(pub [[f32; 4]; 4]);

impl Mat4 {
    pub const IDENTITY: Self = Self([
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]);

    pub const ZERO: Self = Self([[0.0; 4]; 4]);
}

// ── Vector Operations ───────────────────────────────────────────────

/// Linearly interpolate between two Vec4 values.
///
/// `t = 0.0` returns `a`, `t = 1.0` returns `b`.
/// Uses VFPU for all four components simultaneously.
pub fn vec4_lerp(a: &Vec4, b: &Vec4, t: f32) -> Vec4 {
    let mut out = Vec4::ZERO;
    let a_ptr = a.0.as_ptr();
    let b_ptr = b.0.as_ptr();
    let t_bits = t.to_bits();
    let out_ptr = out.0.as_mut_ptr();
    unsafe {
        vfpu_asm!(
            "lv.q C000, 0({a_ptr})",
            "lv.q C010, 0({b_ptr})",
            "mtv {t_bits}, S020",
            // out = a + t * (b - a)
            "vsub.q C010, C010, C000",  // C010 = b - a
            "vscl.q C010, C010, S020",  // C010 = t * (b - a)
            "vadd.q C000, C000, C010",  // C000 = a + t * (b - a)
            "sv.q C000, 0({out_ptr})",
            a_ptr = in(reg) a_ptr,
            b_ptr = in(reg) b_ptr,
            t_bits = in(reg) t_bits,
            out_ptr = in(reg) out_ptr,
            options(nostack),
        );
    }
    out
}

/// Compute the dot product of two Vec4 values.
pub fn vec4_dot(a: &Vec4, b: &Vec4) -> f32 {
    let result: f32;
    let a_ptr = a.0.as_ptr();
    let b_ptr = b.0.as_ptr();
    unsafe {
        vfpu_asm!(
            "lv.q C000, 0({a_ptr})",
            "lv.q C010, 0({b_ptr})",
            "vdot.q S020, C000, C010",
            "mfv {tmp}, S020",
            "mtc1 {tmp}, {fout}",
            "nop",
            a_ptr = in(reg) a_ptr,
            b_ptr = in(reg) b_ptr,
            tmp = out(reg) _,
            fout = out(freg) result,
            options(nostack),
        );
    }
    result
}

/// Normalize a Vec4 (make unit length).
///
/// Returns the zero vector if the input has zero length.
pub fn vec4_normalize(v: &Vec4) -> Vec4 {
    let mut out = Vec4::ZERO;
    let v_ptr = v.0.as_ptr();
    let out_ptr = out.0.as_mut_ptr();
    unsafe {
        vfpu_asm!(
            "lv.q C000, 0({v_ptr})",
            "vdot.q S010, C000, C000",   // S010 = dot(v, v)
            "vrsq.s S010, S010",          // S010 = 1/sqrt(dot)
            "vscl.q C000, C000, S010",    // scale by 1/length
            "sv.q C000, 0({out_ptr})",
            v_ptr = in(reg) v_ptr,
            out_ptr = in(reg) out_ptr,
            options(nostack),
        );
    }
    out
}

/// Add two Vec4 values component-wise.
pub fn vec4_add(a: &Vec4, b: &Vec4) -> Vec4 {
    let mut out = Vec4::ZERO;
    let a_ptr = a.0.as_ptr();
    let b_ptr = b.0.as_ptr();
    let out_ptr = out.0.as_mut_ptr();
    unsafe {
        vfpu_asm!(
            "lv.q C000, 0({a_ptr})",
            "lv.q C010, 0({b_ptr})",
            "vadd.q C000, C000, C010",
            "sv.q C000, 0({out_ptr})",
            a_ptr = in(reg) a_ptr,
            b_ptr = in(reg) b_ptr,
            out_ptr = in(reg) out_ptr,
            options(nostack),
        );
    }
    out
}

/// Subtract two Vec4 values component-wise (a - b).
pub fn vec4_sub(a: &Vec4, b: &Vec4) -> Vec4 {
    let mut out = Vec4::ZERO;
    let a_ptr = a.0.as_ptr();
    let b_ptr = b.0.as_ptr();
    let out_ptr = out.0.as_mut_ptr();
    unsafe {
        vfpu_asm!(
            "lv.q C000, 0({a_ptr})",
            "lv.q C010, 0({b_ptr})",
            "vsub.q C000, C000, C010",
            "sv.q C000, 0({out_ptr})",
            a_ptr = in(reg) a_ptr,
            b_ptr = in(reg) b_ptr,
            out_ptr = in(reg) out_ptr,
            options(nostack),
        );
    }
    out
}

/// Scale a Vec4 by a scalar value.
pub fn vec4_scale(v: &Vec4, s: f32) -> Vec4 {
    let mut out = Vec4::ZERO;
    let v_ptr = v.0.as_ptr();
    let s_bits = s.to_bits();
    let out_ptr = out.0.as_mut_ptr();
    unsafe {
        vfpu_asm!(
            "lv.q C000, 0({v_ptr})",
            "mtv {s_bits}, S010",
            "vscl.q C000, C000, S010",
            "sv.q C000, 0({out_ptr})",
            v_ptr = in(reg) v_ptr,
            s_bits = in(reg) s_bits,
            out_ptr = in(reg) out_ptr,
            options(nostack),
        );
    }
    out
}

/// Compute the cross product of two 3D vectors (w component set to 0).
pub fn vec3_cross(a: &Vec4, b: &Vec4) -> Vec4 {
    let mut out = Vec4::ZERO;
    let a_ptr = a.0.as_ptr();
    let b_ptr = b.0.as_ptr();
    let out_ptr = out.0.as_mut_ptr();
    unsafe {
        vfpu_asm!(
            "lv.q C000, 0({a_ptr})",
            "lv.q C010, 0({b_ptr})",
            "vcrsp.t C020, C000, C010",
            "vzero.s S023",                // w = 0
            "sv.q C020, 0({out_ptr})",
            a_ptr = in(reg) a_ptr,
            b_ptr = in(reg) b_ptr,
            out_ptr = in(reg) out_ptr,
            options(nostack),
        );
    }
    out
}

// ── Matrix Operations ───────────────────────────────────────────────

/// Multiply two 4x4 matrices.
///
/// Returns `a * b` (in column-major order, matching OpenGL conventions).
pub fn mat4_multiply(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = Mat4::ZERO;
    let a_ptr = a.0.as_ptr();
    let b_ptr = b.0.as_ptr();
    let out_ptr = out.0.as_mut_ptr();
    unsafe {
        vfpu_asm!(
            // Load matrix A into M000-M003 (columns 0-3)
            "lv.q C000, 0({a_ptr})",
            "lv.q C010, 16({a_ptr})",
            "lv.q C020, 32({a_ptr})",
            "lv.q C030, 48({a_ptr})",
            // Load matrix B into M100-M103
            "lv.q C100, 0({b_ptr})",
            "lv.q C110, 16({b_ptr})",
            "lv.q C120, 32({b_ptr})",
            "lv.q C130, 48({b_ptr})",
            // Multiply: M200 = M000 * M100
            "vmmul.q M200, M000, M100",
            // Store result
            "sv.q C200, 0({out_ptr})",
            "sv.q C210, 16({out_ptr})",
            "sv.q C220, 32({out_ptr})",
            "sv.q C230, 48({out_ptr})",
            a_ptr = in(reg) a_ptr,
            b_ptr = in(reg) b_ptr,
            out_ptr = in(reg) out_ptr,
            options(nostack),
        );
    }
    out
}

/// Transpose a 4x4 matrix.
pub fn mat4_transpose(m: &Mat4) -> Mat4 {
    let mut out = Mat4::ZERO;
    let m_ptr = m.0.as_ptr();
    let out_ptr = out.0.as_mut_ptr();
    unsafe {
        vfpu_asm!(
            "lv.q C000, 0({m_ptr})",
            "lv.q C010, 16({m_ptr})",
            "lv.q C020, 32({m_ptr})",
            "lv.q C030, 48({m_ptr})",
            // Transpose: rows become columns
            "sv.q R000, 0({out_ptr})",
            "sv.q R001, 16({out_ptr})",
            "sv.q R002, 32({out_ptr})",
            "sv.q R003, 48({out_ptr})",
            m_ptr = in(reg) m_ptr,
            out_ptr = in(reg) out_ptr,
            options(nostack),
        );
    }
    out
}

/// Transform a Vec4 by a Mat4 (matrix * vector).
pub fn mat4_transform(m: &Mat4, v: &Vec4) -> Vec4 {
    let mut out = Vec4::ZERO;
    let m_ptr = m.0.as_ptr();
    let v_ptr = v.0.as_ptr();
    let out_ptr = out.0.as_mut_ptr();
    unsafe {
        vfpu_asm!(
            "lv.q C000, 0({m_ptr})",
            "lv.q C010, 16({m_ptr})",
            "lv.q C020, 32({m_ptr})",
            "lv.q C030, 48({m_ptr})",
            "lv.q C100, 0({v_ptr})",
            "vtfm4.q C110, M000, C100",
            "sv.q C110, 0({out_ptr})",
            m_ptr = in(reg) m_ptr,
            v_ptr = in(reg) v_ptr,
            out_ptr = in(reg) out_ptr,
            options(nostack),
        );
    }
    out
}

/// Load the 4x4 identity matrix.
pub fn mat4_identity() -> Mat4 {
    let mut out = Mat4::ZERO;
    let out_ptr = out.0.as_mut_ptr();
    unsafe {
        vfpu_asm!(
            "vmidt.q M000",
            "sv.q C000, 0({out_ptr})",
            "sv.q C010, 16({out_ptr})",
            "sv.q C020, 32({out_ptr})",
            "sv.q C030, 48({out_ptr})",
            out_ptr = in(reg) out_ptr,
            options(nostack),
        );
    }
    out
}

// ── Color Operations ────────────────────────────────────────────────

/// Blend two RGBA colors using alpha blending.
///
/// Colors are in `[R, G, B, A]` format with components in `0.0..=1.0`.
/// Uses standard "over" compositing: `result = src * src.a + dst * (1 - src.a)`.
pub fn color_blend_rgba(src: &Vec4, dst: &Vec4) -> Vec4 {
    let mut out = Vec4::ZERO;
    let src_ptr = src.0.as_ptr();
    let dst_ptr = dst.0.as_ptr();
    let out_ptr = out.0.as_mut_ptr();
    unsafe {
        vfpu_asm!(
            "lv.q C000, 0({src_ptr})",          // C000 = src RGBA
            "lv.q C010, 0({dst_ptr})",          // C010 = dst RGBA
            // Extract src alpha and compute (1 - src.a)
            "vscl.q C020, C000, S003",      // C020 = src * src.a
            "vone.s S030",                  // S030 = 1.0
            "vsub.s S030, S030, S003",      // S030 = 1 - src.a
            "vscl.q C010, C010, S030",      // C010 = dst * (1 - src.a)
            "vadd.q C000, C020, C010",      // C000 = src*a + dst*(1-a)
            "sv.q C000, 0({out_ptr})",
            src_ptr = in(reg) src_ptr,
            dst_ptr = in(reg) dst_ptr,
            out_ptr = in(reg) out_ptr,
            options(nostack),
        );
    }
    out
}

/// Convert HSV color to RGB.
///
/// Input: `[H, S, V, A]` where H is in `0.0..=360.0`, S/V/A in `0.0..=1.0`.
/// Output: `[R, G, B, A]` with components in `0.0..=1.0`.
pub fn color_hsv_to_rgb(hsv: &Vec4) -> Vec4 {
    let h = hsv.0[0];
    let s = hsv.0[1];
    let v = hsv.0[2];
    let a = hsv.0[3];

    if s <= 0.0 {
        return Vec4::new(v, v, v, a);
    }

    let hh = if h >= 360.0 { 0.0 } else { h / 60.0 };
    let i = hh as u32;
    let ff = hh - i as f32;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * ff);
    let t = v * (1.0 - s * (1.0 - ff));

    match i {
        0 => Vec4::new(v, t, p, a),
        1 => Vec4::new(q, v, p, a),
        2 => Vec4::new(p, v, t, a),
        3 => Vec4::new(p, q, v, a),
        4 => Vec4::new(t, p, v, a),
        _ => Vec4::new(v, p, q, a),
    }
}

/// Convert RGB color to HSV.
///
/// Input: `[R, G, B, A]` with components in `0.0..=1.0`.
/// Output: `[H, S, V, A]` where H is in `0.0..=360.0`.
pub fn color_rgb_to_hsv(rgb: &Vec4) -> Vec4 {
    let r = rgb.0[0];
    let g = rgb.0[1];
    let b = rgb.0[2];
    let a = rgb.0[3];

    let max = if r > g {
        if r > b { r } else { b }
    } else if g > b {
        g
    } else {
        b
    };
    let min = if r < g {
        if r < b { r } else { b }
    } else if g < b {
        g
    } else {
        b
    };
    let delta = max - min;

    let v = max;
    let s = if max > 0.0 { delta / max } else { 0.0 };

    let h = if delta < 0.00001 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta) % 6.0)
    } else if max == g {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };

    let h = if h < 0.0 { h + 360.0 } else { h };

    Vec4::new(h, s, v, a)
}

// ── Easing Functions ────────────────────────────────────────────────

/// Quadratic ease-in-out.
///
/// `t` is in `0.0..=1.0`. Returns a smoothly accelerating/decelerating value.
pub fn ease_in_out_quad(t: f32) -> f32 {
    if t < 0.5 {
        2.0 * t * t
    } else {
        let t2 = -2.0 * t + 2.0;
        1.0 - t2 * t2 / 2.0
    }
}

/// Cubic ease-in-out.
pub fn ease_in_out_cubic(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        let t2 = -2.0 * t + 2.0;
        1.0 - t2 * t2 * t2 / 2.0
    }
}

/// Quadratic ease-in (accelerating from zero).
pub fn ease_in_quad(t: f32) -> f32 {
    t * t
}

/// Quadratic ease-out (decelerating to zero).
pub fn ease_out_quad(t: f32) -> f32 {
    1.0 - (1.0 - t) * (1.0 - t)
}

/// Cubic ease-in.
pub fn ease_in_cubic(t: f32) -> f32 {
    t * t * t
}

/// Cubic ease-out.
pub fn ease_out_cubic(t: f32) -> f32 {
    let t2 = 1.0 - t;
    1.0 - t2 * t2 * t2
}

/// Spring-damped interpolation.
///
/// Simulates a damped spring system. Good for "bouncy" UI animations.
///
/// - `t`: Progress `0.0..=1.0`
/// - `damping`: Damping factor (higher = less bounce). Typical: `0.5..0.8`
/// - `frequency`: Oscillation frequency. Typical: `8.0..15.0`
pub fn spring_damped(t: f32, damping: f32, frequency: f32) -> f32 {
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }

    // Damped harmonic oscillator: 1 - e^(-d*t) * cos(f*t)
    let decay = libm::expf(-damping * t * 10.0);
    let angle = frequency * t * core::f32::consts::PI;
    let oscillation = libm::cosf(angle);
    1.0 - decay * oscillation
}

/// Smoothstep (Hermite interpolation).
///
/// `t` is clamped to `0.0..=1.0`. Returns a smooth S-curve.
pub fn smoothstep(t: f32) -> f32 {
    let t = if t < 0.0 {
        0.0
    } else if t > 1.0 {
        1.0
    } else {
        t
    };
    t * t * (3.0 - 2.0 * t)
}

/// Smoother step (Ken Perlin's improved smoothstep).
pub fn smootherstep(t: f32) -> f32 {
    let t = if t < 0.0 {
        0.0
    } else if t > 1.0 {
        1.0
    } else {
        t
    };
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

// ── Utility ─────────────────────────────────────────────────────────

/// Clamp a float to a range.
pub fn clampf(val: f32, min: f32, max: f32) -> f32 {
    if val < min {
        min
    } else if val > max {
        max
    } else {
        val
    }
}

/// Remap a value from one range to another.
pub fn remapf(val: f32, in_min: f32, in_max: f32, out_min: f32, out_max: f32) -> f32 {
    let t = (val - in_min) / (in_max - in_min);
    out_min + t * (out_max - out_min)
}
