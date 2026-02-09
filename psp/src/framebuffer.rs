//! Framebuffer management and layer compositing for the PSP display.
//!
//! Provides higher-level abstractions over the raw VRAM pointers and
//! display syscalls:
//!
//! - [`DoubleBuffer`]: Vsync-aware page flipping with two framebuffers
//! - [`DirtyRect`]: Track modified regions to minimize VRAM writes
//! - [`LayerCompositor`]: Compose multiple layers (background, content,
//!   overlay) with DMA-driven blits
//!
//! # PSP Display Model
//!
//! The PSP has 2 MiB of VRAM at physical address `0x0400_0000`. The
//! display controller reads from a configurable base address within VRAM.
//! At 32bpp (PSM8888) with a 512-pixel stride, one framebuffer is
//! `512 * 272 * 4 = 557,056` bytes (~544 KiB). Two framebuffers fit
//! comfortably in VRAM with room for textures.

use crate::sys::{DisplayPixelFormat, DisplaySetBufSync};

/// PSP screen width in pixels.
pub const SCREEN_WIDTH: u32 = 480;
/// PSP screen height in pixels.
pub const SCREEN_HEIGHT: u32 = 272;
/// Framebuffer stride in pixels (power-of-two padded).
pub const BUF_WIDTH: u32 = 512;

/// Bytes per pixel for each display pixel format.
pub const fn bytes_per_pixel(fmt: DisplayPixelFormat) -> u32 {
    match fmt {
        DisplayPixelFormat::Psm5650 | DisplayPixelFormat::Psm5551 | DisplayPixelFormat::Psm4444 => {
            2
        },
        DisplayPixelFormat::Psm8888 => 4,
    }
}

/// Size of one framebuffer in bytes.
pub const fn framebuffer_size(fmt: DisplayPixelFormat) -> u32 {
    BUF_WIDTH * SCREEN_HEIGHT * bytes_per_pixel(fmt)
}

// ── DoubleBuffer ────────────────────────────────────────────────────

/// Double-buffered framebuffer manager with vsync-aware page flipping.
///
/// Maintains two framebuffers in VRAM. While the display controller shows
/// one buffer, the application draws into the other. On swap, the display
/// pointer is updated to the newly drawn buffer (optionally synced to
/// vsync to avoid tearing).
///
/// # Example
///
/// ```ignore
/// use psp::framebuffer::DoubleBuffer;
/// use psp::sys::DisplayPixelFormat;
///
/// let mut db = DoubleBuffer::new(DisplayPixelFormat::Psm8888, true);
/// db.init();
///
/// loop {
///     let buf = db.draw_buffer();
///     // ... draw into buf ...
///     db.swap();
/// }
/// ```
pub struct DoubleBuffer {
    /// VRAM offsets for the two buffers (relative to VRAM base).
    offsets: [u32; 2],
    /// Which buffer is currently being displayed (0 or 1).
    display_buf: u8,
    /// Pixel format.
    format: DisplayPixelFormat,
    /// Whether to sync swaps to vsync.
    vsync: bool,
}

impl DoubleBuffer {
    /// Create a new double buffer manager.
    ///
    /// `vsync`: If true, `swap()` waits for vertical blank before
    /// switching buffers, preventing tearing.
    ///
    /// # Important
    ///
    /// You **must** call [`init()`](Self::init) before using the double
    /// buffer. Without it, the display mode is not configured and you
    /// will get a black screen with no error.
    pub fn new(format: DisplayPixelFormat, vsync: bool) -> Self {
        let fb_size = framebuffer_size(format);
        Self {
            offsets: [0, fb_size],
            display_buf: 0,
            format,
            vsync,
        }
    }

    /// Initialize the display mode and set the first framebuffer.
    pub fn init(&self) {
        unsafe {
            crate::sys::sceDisplaySetMode(
                crate::sys::DisplayMode::Lcd,
                SCREEN_WIDTH as usize,
                SCREEN_HEIGHT as usize,
            );
            self.set_display_buffer(self.display_buf);
        }
    }

    /// Get a mutable pointer to the draw buffer (the one NOT being displayed).
    ///
    /// Returns a pointer to uncached VRAM suitable for direct pixel writes.
    pub fn draw_buffer(&self) -> *mut u8 {
        let draw_idx = 1 - self.display_buf;
        self.vram_ptr(draw_idx)
    }

    /// Get a pointer to the display buffer (the one currently shown).
    pub fn display_buffer(&self) -> *const u8 {
        self.vram_ptr(self.display_buf) as *const u8
    }

    /// Get the VRAM offset of the draw buffer.
    pub fn draw_buffer_offset(&self) -> u32 {
        let draw_idx = 1 - self.display_buf;
        self.offsets[draw_idx as usize]
    }

    /// Swap the draw and display buffers.
    ///
    /// If vsync is enabled, this blocks until the next vertical blank
    /// before switching the display pointer.
    pub fn swap(&mut self) {
        if self.vsync {
            unsafe {
                crate::sys::sceDisplayWaitVblankStart();
            }
        }

        // Switch to showing the draw buffer
        self.display_buf = 1 - self.display_buf;

        unsafe {
            self.set_display_buffer(self.display_buf);
        }
    }

    /// Get the pixel format.
    pub fn format(&self) -> DisplayPixelFormat {
        self.format
    }

    /// Enable or disable vsync.
    pub fn set_vsync(&mut self, vsync: bool) {
        self.vsync = vsync;
    }

    /// Get a raw pointer to a VRAM buffer.
    fn vram_ptr(&self, idx: u8) -> *mut u8 {
        const VRAM_UNCACHED: u32 = 0x4400_0000;
        (VRAM_UNCACHED + self.offsets[idx as usize]) as *mut u8
    }

    /// Set the display controller to show the given buffer index.
    unsafe fn set_display_buffer(&self, idx: u8) {
        let sync = if self.vsync {
            DisplaySetBufSync::NextFrame
        } else {
            DisplaySetBufSync::Immediate
        };
        unsafe {
            crate::sys::sceDisplaySetFrameBuf(
                self.vram_ptr(idx) as *const u8,
                BUF_WIDTH as usize,
                self.format,
                sync,
            );
        }
    }
}

// ── DirtyRect ───────────────────────────────────────────────────────

/// A dirty-rectangle tracker.
///
/// Tracks which rectangular regions of the framebuffer have been modified,
/// so you can minimize the amount of data copied (e.g., via DMA) when
/// updating the display.
///
/// # Example
///
/// ```ignore
/// use psp::framebuffer::DirtyRect;
///
/// let mut dirty = DirtyRect::new();
/// dirty.mark(10, 20, 100, 50); // Mark region (10,20)-(110,70) as dirty
/// dirty.mark(200, 100, 80, 30);
///
/// if let Some((x, y, w, h)) = dirty.bounds() {
///     // Copy only the bounding rectangle of all dirty regions
///     blit_region(x, y, w, h);
/// }
///
/// dirty.clear();
/// ```
pub struct DirtyRect {
    min_x: u32,
    min_y: u32,
    max_x: u32,
    max_y: u32,
    dirty: bool,
}

impl DirtyRect {
    /// Create a new (clean) dirty-rect tracker.
    pub const fn new() -> Self {
        Self {
            min_x: u32::MAX,
            min_y: u32::MAX,
            max_x: 0,
            max_y: 0,
            dirty: false,
        }
    }

    /// Mark a rectangular region as dirty.
    pub fn mark(&mut self, x: u32, y: u32, width: u32, height: u32) {
        self.dirty = true;
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(x + width);
        self.max_y = self.max_y.max(y + height);
    }

    /// Mark the entire screen as dirty.
    pub fn mark_all(&mut self) {
        self.mark(0, 0, SCREEN_WIDTH, SCREEN_HEIGHT);
    }

    /// Get the bounding rectangle of all dirty regions.
    ///
    /// Returns `Some((x, y, width, height))` if any region is dirty,
    /// or `None` if everything is clean.
    pub fn bounds(&self) -> Option<(u32, u32, u32, u32)> {
        if !self.dirty {
            return None;
        }
        let x = self.min_x.min(SCREEN_WIDTH);
        let y = self.min_y.min(SCREEN_HEIGHT);
        let max_x = self.max_x.min(SCREEN_WIDTH);
        let max_y = self.max_y.min(SCREEN_HEIGHT);
        if max_x <= x || max_y <= y {
            return None;
        }
        Some((x, y, max_x - x, max_y - y))
    }

    /// Clear all dirty flags.
    pub fn clear(&mut self) {
        self.min_x = u32::MAX;
        self.min_y = u32::MAX;
        self.max_x = 0;
        self.max_y = 0;
        self.dirty = false;
    }

    /// Check if any region is dirty.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
}

impl Default for DirtyRect {
    fn default() -> Self {
        Self::new()
    }
}

// ── LayerCompositor ─────────────────────────────────────────────────

/// Layer index for the compositor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Layer {
    /// Background layer (drawn first, behind everything).
    Background = 0,
    /// Content layer (main application content).
    Content = 1,
    /// Overlay layer (drawn last, on top of everything — notifications, HUD).
    Overlay = 2,
}

/// Number of compositing layers.
pub const NUM_LAYERS: usize = 3;

/// A simple layer-based framebuffer compositor.
///
/// Maintains separate offscreen buffers for background, content, and
/// overlay layers. On each frame, the compositor blits the dirty
/// portions of each layer into the final framebuffer in order.
///
/// # Memory Layout
///
/// Each layer gets its own buffer. For PSM8888 at 512x272, each buffer
/// is ~544 KiB. Three layers plus two display buffers require ~2.7 MiB,
/// which exceeds VRAM (2 MiB). Therefore, layer buffers are allocated in
/// main RAM and blitted to VRAM.
///
/// # Example
///
/// ```ignore
/// use psp::framebuffer::{LayerCompositor, Layer, DoubleBuffer};
/// use psp::sys::DisplayPixelFormat;
///
/// let db = DoubleBuffer::new(DisplayPixelFormat::Psm8888, true);
/// let mut comp = LayerCompositor::new(DisplayPixelFormat::Psm8888);
///
/// // Draw into layers
/// let bg = comp.layer_buffer(Layer::Background);
/// // ... draw background ...
/// comp.mark_dirty(Layer::Background, 0, 0, 480, 272);
///
/// // Composite to the draw buffer
/// comp.composite_to(db.draw_buffer());
/// ```
pub struct LayerCompositor {
    format: DisplayPixelFormat,
    dirty: [DirtyRect; NUM_LAYERS],
    /// Whether each layer is enabled.
    enabled: [bool; NUM_LAYERS],
}

impl LayerCompositor {
    /// Create a new layer compositor.
    pub fn new(format: DisplayPixelFormat) -> Self {
        Self {
            format,
            dirty: [DirtyRect::new(), DirtyRect::new(), DirtyRect::new()],
            enabled: [true, true, true],
        }
    }

    /// Mark a rectangular region of a layer as dirty.
    pub fn mark_dirty(&mut self, layer: Layer, x: u32, y: u32, w: u32, h: u32) {
        self.dirty[layer as usize].mark(x, y, w, h);
    }

    /// Mark an entire layer as dirty.
    pub fn mark_layer_dirty(&mut self, layer: Layer) {
        self.dirty[layer as usize].mark_all();
    }

    /// Enable or disable a layer.
    pub fn set_layer_enabled(&mut self, layer: Layer, enabled: bool) {
        self.enabled[layer as usize] = enabled;
    }

    /// Check if a layer is enabled.
    pub fn is_layer_enabled(&self, layer: Layer) -> bool {
        self.enabled[layer as usize]
    }

    /// Composite all dirty layer regions into the output buffer.
    ///
    /// Layers are drawn in order: Background, Content, Overlay. Only
    /// the dirty bounding rectangle of each layer is copied.
    ///
    /// # Safety
    ///
    /// - `output` must point to a valid framebuffer of the correct format.
    /// - `layer_buffers` must contain valid pointers for each enabled layer.
    pub unsafe fn composite_to(
        &mut self,
        output: *mut u8,
        layer_buffers: &[*const u8; NUM_LAYERS],
    ) {
        let bpp = bytes_per_pixel(self.format);
        let stride = BUF_WIDTH * bpp;

        for i in 0..NUM_LAYERS {
            if !self.enabled[i] {
                continue;
            }

            if let Some((x, y, w, h)) = self.dirty[i].bounds() {
                let src = layer_buffers[i];
                // Blit the dirty region row by row
                for row in y..y + h {
                    let src_offset = (row * stride + x * bpp) as usize;
                    let dst_offset = src_offset;
                    let row_bytes = (w * bpp) as usize;

                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            src.add(src_offset),
                            output.add(dst_offset),
                            row_bytes,
                        );
                    }
                }

                self.dirty[i].clear();
            }
        }
    }

    /// Clear all dirty flags for all layers.
    pub fn clear_all_dirty(&mut self) {
        for d in &mut self.dirty {
            d.clear();
        }
    }

    /// Get the pixel format.
    pub fn format(&self) -> DisplayPixelFormat {
        self.format
    }
}
