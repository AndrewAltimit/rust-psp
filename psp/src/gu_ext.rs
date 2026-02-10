//! GU rendering extensions for 2D sprite batching.
//!
//! Provides state snapshot/restore, 2D setup helpers, and a sprite batcher
//! that draws textured quads efficiently using `GuPrimitive::Sprites`.

use crate::sys::{
    BlendFactor, BlendOp, GuState, MatrixMode, VertexType, sceGuBlendFunc, sceGuDisable,
    sceGuEnable, sceGuGetAllStatus, sceGuSetAllStatus, sceGumLoadIdentity, sceGumMatrixMode,
    sceGumOrtho,
};

/// Snapshot of all 22 GU boolean states.
///
/// Only covers the states toggled by `sceGuEnable`/`sceGuDisable`.
/// Other state (blend func, texture mode, scissor) must be saved manually.
pub struct GuStateSnapshot {
    bits: i32,
}

impl GuStateSnapshot {
    /// Capture the current GU boolean state.
    pub fn capture() -> Self {
        Self {
            bits: unsafe { sceGuGetAllStatus() },
        }
    }

    /// Restore the captured state.
    pub fn restore(&self) {
        unsafe { sceGuSetAllStatus(self.bits) };
    }
}

/// Set up GU for 2D rendering.
///
/// Configures an orthographic projection from (0,0) to (480,272), disables
/// depth testing, and enables texture mapping and alpha blending.
///
/// # Safety
///
/// Must be called within an active GU display list.
pub unsafe fn setup_2d() {
    unsafe {
        sceGumMatrixMode(MatrixMode::Projection);
        sceGumLoadIdentity();
        sceGumOrtho(0.0, 480.0, 272.0, 0.0, -1.0, 1.0);

        sceGumMatrixMode(MatrixMode::View);
        sceGumLoadIdentity();

        sceGumMatrixMode(MatrixMode::Model);
        sceGumLoadIdentity();

        sceGuDisable(GuState::DepthTest);
        sceGuEnable(GuState::Texture2D);
        sceGuEnable(GuState::Blend);
        sceGuBlendFunc(
            BlendOp::Add,
            BlendFactor::SrcAlpha,
            BlendFactor::OneMinusSrcAlpha,
            0,
            0,
        );
    }
}

/// 2D sprite vertex: texture coords + color + position.
///
/// Layout matches `SPRITE_VERTEX_TYPE` for use with `GuPrimitive::Sprites`.
#[repr(C, align(4))]
#[derive(Clone, Copy)]
pub struct SpriteVertex {
    pub u: f32,
    pub v: f32,
    pub color: u32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// Vertex type flags for [`SpriteVertex`].
pub const SPRITE_VERTEX_TYPE: VertexType = VertexType::from_bits_truncate(
    VertexType::TEXTURE_32BITF.bits()
        | VertexType::COLOR_8888.bits()
        | VertexType::VERTEX_32BITF.bits()
        | VertexType::TRANSFORM_2D.bits(),
);

/// Batches textured quads for efficient 2D rendering.
///
/// Each sprite is a pair of vertices (top-left, bottom-right) drawn with
/// `GuPrimitive::Sprites`. Call [`flush`](SpriteBatch::flush) to submit
/// all queued sprites in a single draw call.
#[cfg(not(feature = "stub-only"))]
pub struct SpriteBatch {
    vertices: alloc::vec::Vec<SpriteVertex>,
}

#[cfg(not(feature = "stub-only"))]
impl SpriteBatch {
    /// Create a new sprite batch with capacity for `max_sprites` sprites.
    ///
    /// Each sprite uses 2 vertices, so this allocates `max_sprites * 2` entries.
    pub fn new(max_sprites: usize) -> Self {
        Self {
            vertices: alloc::vec::Vec::with_capacity(max_sprites * 2),
        }
    }

    /// Add a textured rectangle.
    ///
    /// `(x, y)` is the top-left corner, `(w, h)` is the size.
    /// `(u0, v0)` to `(u1, v1)` are texture coordinates.
    /// `color` is ABGR format (0xAABBGGRR).
    pub fn draw_rect(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        u0: f32,
        v0: f32,
        u1: f32,
        v1: f32,
        color: u32,
    ) {
        self.vertices.push(SpriteVertex {
            u: u0,
            v: v0,
            color,
            x,
            y,
            z: 0.0,
        });
        self.vertices.push(SpriteVertex {
            u: u1,
            v: v1,
            color,
            x: x + w,
            y: y + h,
            z: 0.0,
        });
    }

    /// Add an untextured colored rectangle.
    ///
    /// Texture coordinates are set to 0; bind a 1x1 white texture or
    /// disable texturing before flushing.
    pub fn draw_colored_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: u32) {
        self.draw_rect(x, y, w, h, 0.0, 0.0, 0.0, 0.0, color);
    }

    /// Number of sprites currently queued.
    pub fn count(&self) -> usize {
        self.vertices.len() / 2
    }

    /// Discard all queued sprites.
    pub fn clear(&mut self) {
        self.vertices.clear();
    }

    /// Submit all queued sprites to the GU and clear the batch.
    ///
    /// Vertex data is copied into display-list memory (via `sceGuGetMemory`)
    /// so it remains valid until `sceGuFinish`, regardless of when this
    /// `SpriteBatch` is dropped.
    ///
    /// # Safety
    ///
    /// Must be called within an active GU display list with an appropriate
    /// texture bound (for textured sprites).
    pub unsafe fn flush(&mut self) {
        use crate::sys::{
            GuPrimitive, sceGuDrawArray, sceGuGetMemory,
        };
        use core::ffi::c_void;

        if self.vertices.is_empty() {
            return;
        }
        unsafe {
            let count = self.vertices.len();
            let byte_size = count * core::mem::size_of::<SpriteVertex>();

            // Allocate from the display list so the GE can safely read the
            // vertex data even after this SpriteBatch is dropped.
            let dl_verts =
                sceGuGetMemory(byte_size as i32) as *mut SpriteVertex;
            if dl_verts.is_null() {
                self.vertices.clear();
                return;
            }

            // Copy vertices into display-list memory.
            core::ptr::copy_nonoverlapping(
                self.vertices.as_ptr(),
                dl_verts,
                count,
            );

            sceGuDrawArray(
                GuPrimitive::Sprites,
                SPRITE_VERTEX_TYPE,
                count as i32,
                core::ptr::null::<c_void>(),
                dl_verts as *const c_void,
            );
        }
        self.vertices.clear();
    }
}
