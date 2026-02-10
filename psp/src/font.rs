//! Font rendering with VRAM glyph atlas.
//!
//! Three-layer architecture:
//! - [`FontLib`]: Library instance (one per app). RAII.
//! - [`Font`]: Open PGF font handle. RAII.
//! - [`FontRenderer`]: High-level text renderer with glyph atlas caching
//!   and sprite-batched drawing via [`crate::gu_ext::SpriteBatch`].

use alloc::vec::Vec;
use core::alloc::Layout;
use core::ffi::c_void;

use crate::sys::{
    SceFontCharInfo, SceFontErrorCode, SceFontFamilyCode, SceFontGlyphImage, SceFontInfo,
    SceFontLanguageCode, SceFontNewLibParams, SceFontPixelFormatCode, SceFontStyle,
    SceFontStyleCode, sceFontClose, sceFontDoneLib, sceFontFindOptimumFont,
    sceFontGetCharGlyphImage, sceFontGetCharInfo, sceFontGetFontInfo, sceFontGetNumFontList,
    sceFontNewLib, sceFontOpen,
};

/// Error from a font operation.
pub enum FontError {
    /// SCE error code from a font syscall.
    Sce(i32),
    /// Font library error code.
    Lib(SceFontErrorCode),
    /// Font not found.
    NotFound,
    /// Font library not initialized.
    NotInitialized,
}

impl core::fmt::Debug for FontError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Sce(e) => write!(f, "FontError::Sce({e:#010x})"),
            Self::Lib(e) => write!(f, "FontError::Lib({e:?})"),
            Self::NotFound => write!(f, "FontError::NotFound"),
            Self::NotInitialized => write!(f, "FontError::NotInitialized"),
        }
    }
}

impl core::fmt::Display for FontError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Sce(e) => write!(f, "font error {e:#010x}"),
            Self::Lib(e) => write!(f, "font library error {e:?}"),
            Self::NotFound => write!(f, "font not found"),
            Self::NotInitialized => write!(f, "font library not initialized"),
        }
    }
}

// ── Alloc callbacks for SceFontNewLibParams ──────────────────────────

extern "C" fn font_alloc(_user: *mut c_void, size: usize) -> *mut c_void {
    let total = size + 16;
    let Ok(layout) = Layout::from_size_align(total, 16) else {
        return core::ptr::null_mut();
    };
    let ptr = unsafe { alloc::alloc::alloc(layout) };
    if ptr.is_null() {
        return core::ptr::null_mut();
    }
    unsafe { (ptr as *mut usize).write(total) };
    unsafe { ptr.add(16) as *mut c_void }
}

extern "C" fn font_free(_user: *mut c_void, ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }
    let real_ptr = unsafe { (ptr as *mut u8).sub(16) };
    let total = unsafe { (real_ptr as *mut usize).read() };
    if let Ok(layout) = Layout::from_size_align(total, 16) {
        unsafe { alloc::alloc::dealloc(real_ptr, layout) };
    }
}

// ── FontLib ──────────────────────────────────────────────────────────

/// Font library instance. One per application.
///
/// Calls `sceFontDoneLib` on drop.
pub struct FontLib {
    handle: u32,
}

impl FontLib {
    /// Initialize the font library.
    ///
    /// `max_fonts` is the maximum number of fonts that can be open simultaneously.
    pub fn new(max_fonts: u32) -> Result<Self, FontError> {
        let params = SceFontNewLibParams {
            user_data_addr: 0,
            num_fonts: max_fonts,
            cache_data: 0,
            alloc_func: Some(font_alloc),
            free_func: Some(font_free),
            open_func: None,
            close_func: None,
            read_func: None,
            seek_func: None,
            error_func: None,
            io_finish_func: None,
        };

        let mut error = SceFontErrorCode::Success;
        let handle = unsafe { sceFontNewLib(&params, &mut error) };

        if handle == 0 {
            return Err(FontError::Lib(error));
        }

        Ok(Self { handle })
    }

    /// Open a font by index.
    pub fn open(&self, index: u32) -> Result<Font, FontError> {
        let mut error = SceFontErrorCode::Success;
        let font_handle = unsafe { sceFontOpen(self.handle, index, 0, &mut error) };
        if font_handle == 0 {
            return Err(FontError::Lib(error));
        }
        Ok(Font {
            handle: font_handle,
            _lib_handle: self.handle,
        })
    }

    /// Find and open the best matching font.
    pub fn find_optimum(
        &self,
        family: SceFontFamilyCode,
        style: SceFontStyleCode,
        language: SceFontLanguageCode,
    ) -> Result<Font, FontError> {
        let mut font_style: SceFontStyle = unsafe { core::mem::zeroed() };
        font_style.font_family = family;
        font_style.font_style = style;
        font_style.font_language = language;
        // Default resolution.
        font_style.font_h = 0.0;
        font_style.font_v = 0.0;
        font_style.font_h_res = 128.0;
        font_style.font_v_res = 128.0;

        let mut error = SceFontErrorCode::Success;
        let index = unsafe { sceFontFindOptimumFont(self.handle, &font_style, &mut error) };
        if index < 0 {
            return Err(FontError::NotFound);
        }

        self.open(index as u32)
    }

    /// Get the number of fonts available in the library.
    pub fn font_count(&self) -> Result<i32, FontError> {
        let mut error = SceFontErrorCode::Success;
        let count = unsafe { sceFontGetNumFontList(self.handle, &mut error) };
        if count < 0 {
            Err(FontError::Sce(count))
        } else {
            Ok(count)
        }
    }
}

impl Drop for FontLib {
    fn drop(&mut self) {
        unsafe { sceFontDoneLib(self.handle) };
    }
}

// ── Font ─────────────────────────────────────────────────────────────

/// An open PGF font handle.
///
/// Calls `sceFontClose` on drop.
pub struct Font {
    handle: u32,
    _lib_handle: u32,
}

impl Font {
    /// Get character metrics without rendering.
    pub fn char_info(&self, c: char) -> Result<GlyphMetrics, FontError> {
        let mut info: SceFontCharInfo = unsafe { core::mem::zeroed() };
        let ret = unsafe { sceFontGetCharInfo(self.handle, c as u32, &mut info) };
        if ret < 0 {
            return Err(FontError::Sce(ret));
        }
        Ok(GlyphMetrics {
            width: info.bitmap_width,
            height: info.bitmap_height,
            bearing_x: sfp26_to_f32(info.sfp26_bearing_hx),
            bearing_y: sfp26_to_f32(info.sfp26_bearing_hy),
            advance_x: sfp26_to_f32(info.sfp26_advance_h),
            advance_y: sfp26_to_f32(info.sfp26_advance_v),
        })
    }

    /// Get font-level information.
    pub fn info(&self) -> Result<SceFontInfo, FontError> {
        let mut info: SceFontInfo = unsafe { core::mem::zeroed() };
        let ret = unsafe { sceFontGetFontInfo(self.handle, &mut info) };
        if ret < 0 {
            Err(FontError::Sce(ret))
        } else {
            Ok(info)
        }
    }

    /// Render a glyph into a buffer in Format8 (8-bit alpha).
    ///
    /// `buf` must be at least `buf_width * buf_height` bytes.
    /// Returns the glyph metrics on success.
    pub fn render_glyph(
        &self,
        c: char,
        buf: &mut [u8],
        buf_width: u16,
        buf_height: u16,
    ) -> Result<GlyphMetrics, FontError> {
        let metrics = self.char_info(c)?;

        if metrics.width == 0 || metrics.height == 0 {
            return Ok(metrics);
        }

        // Clear the target region.
        for b in buf
            .iter_mut()
            .take((buf_width as usize) * (buf_height as usize))
        {
            *b = 0;
        }

        let mut glyph_image = SceFontGlyphImage {
            pixel_format: SceFontPixelFormatCode::Format8,
            x_pos_64: 0,
            y_pos_64: 0,
            buf_width,
            buf_height,
            bytes_per_line: buf_width,
            pad: 0,
            buffer_ptr: buf.as_mut_ptr() as u32,
        };

        let ret = unsafe { sceFontGetCharGlyphImage(self.handle, c as u32, &mut glyph_image) };
        if ret < 0 {
            return Err(FontError::Sce(ret));
        }

        Ok(metrics)
    }

    /// Get the raw font handle for direct syscall use.
    pub fn handle(&self) -> u32 {
        self.handle
    }
}

impl Drop for Font {
    fn drop(&mut self) {
        unsafe { sceFontClose(self.handle) };
    }
}

// ── GlyphMetrics ─────────────────────────────────────────────────────

/// Metrics for a single glyph.
#[derive(Debug, Clone, Copy, Default)]
pub struct GlyphMetrics {
    pub width: u32,
    pub height: u32,
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub advance_x: f32,
    pub advance_y: f32,
}

/// Convert a 26.6 fixed-point value to f32.
fn sfp26_to_f32(v: i32) -> f32 {
    v as f32 / 64.0
}

// ── Glyph Atlas ──────────────────────────────────────────────────────

struct AtlasRow {
    y: u32,
    height: u32,
    x_cursor: u32,
    lru_stamp: u32,
}

struct CachedGlyph {
    char_code: u32,
    atlas_x: u32,
    atlas_y: u32,
    atlas_w: u32,
    atlas_h: u32,
    metrics: GlyphMetrics,
    row_idx: usize,
}

struct GlyphAtlas {
    vram_ptr: *mut u8,
    width: u32,
    height: u32,
    rows: Vec<AtlasRow>,
    cache: Vec<CachedGlyph>,
    lru_counter: u32,
    y_cursor: u32,
}

impl GlyphAtlas {
    fn new(vram_ptr: *mut u8, width: u32, height: u32) -> Self {
        Self {
            vram_ptr,
            width,
            height,
            rows: Vec::new(),
            cache: Vec::new(),
            lru_counter: 0,
            y_cursor: 0,
        }
    }

    fn find_cached(&mut self, char_code: u32) -> Option<&CachedGlyph> {
        let stamp = self.lru_counter;
        for entry in &mut self.cache {
            if entry.char_code == char_code {
                // Update LRU stamp on the row.
                if let Some(row) = self.rows.get_mut(entry.row_idx) {
                    row.lru_stamp = stamp;
                }
                // Return a reference — we need to reborrow from self.cache.
                break;
            }
        }
        // Re-search to satisfy borrow checker.
        self.cache.iter().find(|e| e.char_code == char_code)
    }

    fn insert(
        &mut self,
        char_code: u32,
        glyph_w: u32,
        glyph_h: u32,
        metrics: GlyphMetrics,
        staging: &[u8],
        staging_width: u32,
    ) -> Option<&CachedGlyph> {
        self.lru_counter += 1;
        let stamp = self.lru_counter;

        // Try to fit in an existing row.
        let mut fit_row = None;
        for (i, row) in self.rows.iter().enumerate() {
            if row.height >= glyph_h && row.x_cursor + glyph_w <= self.width {
                fit_row = Some(i);
                break;
            }
        }

        // No existing row fits — try to add a new row.
        if fit_row.is_none() {
            if self.y_cursor + glyph_h <= self.height {
                let idx = self.rows.len();
                self.rows.push(AtlasRow {
                    y: self.y_cursor,
                    height: glyph_h,
                    x_cursor: 0,
                    lru_stamp: stamp,
                });
                self.y_cursor += glyph_h;
                fit_row = Some(idx);
            }
        }

        // Still no room — evict the LRU row that can fit the glyph height.
        if fit_row.is_none() {
            if let Some((evict_idx, _)) = self
                .rows
                .iter()
                .enumerate()
                .filter(|(_, r)| r.height >= glyph_h)
                .min_by_key(|(_, r)| r.lru_stamp)
            {
                // Remove all cached glyphs in this row.
                self.cache.retain(|g| g.row_idx != evict_idx);
                let row = &mut self.rows[evict_idx];
                row.x_cursor = 0;
                // Keep the original row height to avoid overwriting adjacent rows.
                row.lru_stamp = stamp;
                fit_row = Some(evict_idx);
            }
        }

        let row_idx = fit_row?;
        let row = &mut self.rows[row_idx];
        let atlas_x = row.x_cursor;
        let atlas_y = row.y;
        row.x_cursor += glyph_w;
        row.lru_stamp = stamp;

        // Copy staging buffer to VRAM atlas.
        for sy in 0..glyph_h {
            let src_off = (sy * staging_width) as usize;
            let dst_off = ((atlas_y + sy) * self.width + atlas_x) as usize;
            let len = glyph_w as usize;
            if src_off + len <= staging.len() {
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        staging.as_ptr().add(src_off),
                        self.vram_ptr.add(dst_off),
                        len,
                    );
                }
            }
        }

        self.cache.push(CachedGlyph {
            char_code,
            atlas_x,
            atlas_y,
            atlas_w: glyph_w,
            atlas_h: glyph_h,
            metrics,
            row_idx,
        });

        self.cache.last()
    }

    fn clear(&mut self) {
        self.rows.clear();
        self.cache.clear();
        self.y_cursor = 0;
        self.lru_counter = 0;
    }
}

// ── FontRenderer ─────────────────────────────────────────────────────

/// High-level text renderer with VRAM glyph atlas and sprite batching.
///
/// Renders glyphs to a PsmT8 atlas in VRAM on cache miss, then draws
/// them as textured sprites via [`crate::gu_ext::SpriteBatch`].
pub struct FontRenderer<'a> {
    font: &'a Font,
    atlas: GlyphAtlas,
    batch: crate::gu_ext::SpriteBatch,
    font_size: f32,
    max_ascender: f32,
    staging: Vec<u8>,
}

/// CLUT for PsmT8: maps index i to RGBA(0xFF, 0xFF, 0xFF, i).
/// 256 entries × 4 bytes = 1024 bytes. Must be 16-byte aligned.
#[repr(align(16))]
struct ClutTable([u32; 256]);

static ALPHA_CLUT: ClutTable = {
    let mut table = [0u32; 256];
    let mut i = 0u32;
    while i < 256 {
        // ABGR format: 0xAABBGGRR where AA=i, BB=FF, GG=FF, RR=FF
        table[i as usize] = (i << 24) | 0x00FFFFFF;
        i += 1;
    }
    ClutTable(table)
};

const ATLAS_WIDTH: u32 = 512;
const ATLAS_HEIGHT: u32 = 512;
const MAX_STAGING_SIZE: usize = 128 * 128; // Largest single glyph staging buffer.

impl<'a> FontRenderer<'a> {
    /// Create a font renderer.
    ///
    /// `atlas_vram` must point to at least `512 * 512` bytes of VRAM
    /// (allocated via `vram_alloc`). `font_size` is used for scaling
    /// (currently informational — PSP system fonts have fixed pixel sizes).
    pub fn new(font: &'a Font, atlas_vram: *mut u8, font_size: f32) -> Self {
        let max_ascender = font
            .info()
            .map(|i| i.max_glyph_ascender_f)
            .unwrap_or(font_size * 0.8);
        Self {
            font,
            atlas: GlyphAtlas::new(atlas_vram, ATLAS_WIDTH, ATLAS_HEIGHT),
            batch: crate::gu_ext::SpriteBatch::new(256),
            font_size,
            max_ascender,
            staging: alloc::vec![0u8; MAX_STAGING_SIZE],
        }
    }

    /// Queue text for drawing at `(x, y)` with the given color (ABGR).
    ///
    /// `y` is the **top** of the text line (not the baseline). The
    /// renderer adds the font's max ascender internally to derive the
    /// baseline, so callers can position text with simple top-left
    /// coordinates.
    ///
    /// Renders glyphs to the atlas on cache miss. Characters that fail
    /// to render are silently skipped.
    pub fn draw_text(&mut self, x: f32, y: f32, color: u32, text: &str) {
        let mut cursor_x = x;
        let baseline = y + self.max_ascender;

        for c in text.chars() {
            if c == ' ' {
                // Use advance of space character or fallback.
                if let Ok(metrics) = self.font.char_info(c) {
                    cursor_x += metrics.advance_x;
                } else {
                    cursor_x += self.font_size * 0.5;
                }
                continue;
            }

            let char_code = c as u32;

            // Check cache first.
            if let Some(cached) = self.atlas.find_cached(char_code) {
                let gx = cursor_x + cached.metrics.bearing_x;
                let gy = baseline - cached.metrics.bearing_y;
                let u0 = cached.atlas_x as f32;
                let v0 = cached.atlas_y as f32;
                let u1 = (cached.atlas_x + cached.atlas_w) as f32;
                let v1 = (cached.atlas_y + cached.atlas_h) as f32;
                self.batch.draw_rect(
                    gx,
                    gy,
                    cached.atlas_w as f32,
                    cached.atlas_h as f32,
                    u0,
                    v0,
                    u1,
                    v1,
                    color,
                );
                cursor_x += cached.metrics.advance_x;
                continue;
            }

            // Cache miss — render glyph.
            let Ok(metrics) = self.font.char_info(c) else {
                continue;
            };

            if metrics.width == 0 || metrics.height == 0 {
                cursor_x += metrics.advance_x;
                continue;
            }

            let gw = metrics.width;
            let gh = metrics.height;
            let staging_size = (gw * gh) as usize;
            if staging_size > self.staging.len() {
                self.staging.resize(staging_size, 0);
            }

            // Clear staging buffer.
            for b in self.staging[..staging_size].iter_mut() {
                *b = 0;
            }

            let mut glyph_image = SceFontGlyphImage {
                pixel_format: SceFontPixelFormatCode::Format8,
                x_pos_64: 0,
                y_pos_64: 0,
                buf_width: gw as u16,
                buf_height: gh as u16,
                bytes_per_line: gw as u16,
                pad: 0,
                buffer_ptr: self.staging.as_mut_ptr() as u32,
            };

            let ret =
                unsafe { sceFontGetCharGlyphImage(self.font.handle, char_code, &mut glyph_image) };
            if ret < 0 {
                cursor_x += metrics.advance_x;
                continue;
            }

            // Insert into atlas.
            if let Some(cached) = self.atlas.insert(
                char_code,
                gw,
                gh,
                metrics,
                &self.staging[..staging_size],
                gw,
            ) {
                let gx = cursor_x + cached.metrics.bearing_x;
                let gy = baseline - cached.metrics.bearing_y;
                let u0 = cached.atlas_x as f32;
                let v0 = cached.atlas_y as f32;
                let u1 = (cached.atlas_x + cached.atlas_w) as f32;
                let v1 = (cached.atlas_y + cached.atlas_h) as f32;
                self.batch.draw_rect(
                    gx,
                    gy,
                    cached.atlas_w as f32,
                    cached.atlas_h as f32,
                    u0,
                    v0,
                    u1,
                    v1,
                    color,
                );
            }

            cursor_x += metrics.advance_x;
        }
    }

    /// Measure the width of a string in pixels without drawing.
    pub fn measure_text(&self, text: &str) -> f32 {
        let mut width = 0.0f32;
        for c in text.chars() {
            if let Ok(metrics) = self.font.char_info(c) {
                width += metrics.advance_x;
            }
        }
        width
    }

    /// Get the line height in pixels.
    pub fn line_height(&self) -> f32 {
        if let Ok(info) = self.font.info() {
            info.max_glyph_height_f
        } else {
            self.font_size
        }
    }

    /// Submit all queued glyph sprites to the GU.
    ///
    /// Sets up the CLUT and texture state for the PsmT8 atlas, then
    /// flushes the sprite batch.
    ///
    /// # Safety
    ///
    /// Must be called within an active GU display list.
    pub unsafe fn flush(&mut self) {
        if self.batch.count() == 0 {
            return;
        }

        unsafe {
            // Set up CLUT: alpha-ramp lookup table.
            crate::sys::sceGuClutMode(crate::sys::ClutPixelFormat::Psm8888, 0, 0xFF, 0);
            crate::sys::sceGuClutLoad(256 / 8, ALPHA_CLUT.0.as_ptr() as *const c_void);

            // Bind atlas texture as PsmT8.
            crate::sys::sceGuTexMode(crate::sys::TexturePixelFormat::PsmT8, 0, 0, 0);
            crate::sys::sceGuTexImage(
                crate::sys::MipmapLevel::None,
                ATLAS_WIDTH as i32,
                ATLAS_HEIGHT as i32,
                ATLAS_WIDTH as i32,
                self.atlas.vram_ptr as *const c_void,
            );

            // Modulate: vertex color * texture alpha.
            crate::sys::sceGuTexFunc(
                crate::sys::TextureEffect::Modulate,
                crate::sys::TextureColorComponent::Rgba,
            );

            self.batch.flush();
        }
    }

    /// Clear the atlas, forcing all glyphs to be re-rendered.
    pub fn clear_atlas(&mut self) {
        self.atlas.clear();
    }

    /// Change the font size (clears the atlas).
    ///
    /// Note: PSP system fonts have fixed pixel sizes. This value is used
    /// for spacing calculations when the font doesn't report metrics.
    pub fn set_size(&mut self, size: f32) {
        self.font_size = size;
        self.atlas.clear();
    }
}
