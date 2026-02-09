//! Image decoding for the PSP.
//!
//! Supports hardware-accelerated JPEG decoding via `sceJpeg*` and
//! software BMP decoding for uncompressed 24/32-bit bitmaps.

use alloc::vec::Vec;
use core::ffi::c_void;

/// Pixel format of decoded image data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// 4 bytes per pixel: R, G, B, A.
    Rgba8888,
    /// 3 bytes per pixel: R, G, B.
    Rgb888,
}

/// A decoded image in memory.
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub data: Vec<u8>,
}

/// Error from an image operation.
pub enum ImageError {
    /// Could not determine image format from magic bytes.
    UnknownFormat,
    /// Hardware JPEG decode error (SCE error code).
    JpegError(i32),
    /// BMP parsing error.
    InvalidBmp(&'static str),
    /// I/O error loading from file.
    Io(crate::io::IoError),
}

impl core::fmt::Debug for ImageError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownFormat => write!(f, "ImageError::UnknownFormat"),
            Self::JpegError(e) => write!(f, "ImageError::JpegError({e:#010x})"),
            Self::InvalidBmp(msg) => write!(f, "ImageError::InvalidBmp({msg:?})"),
            Self::Io(e) => write!(f, "ImageError::Io({e:?})"),
        }
    }
}

impl core::fmt::Display for ImageError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownFormat => write!(f, "unknown image format"),
            Self::JpegError(e) => write!(f, "JPEG decode error {e:#010x}"),
            Self::InvalidBmp(msg) => write!(f, "invalid BMP: {msg}"),
            Self::Io(e) => write!(f, "image I/O error: {e}"),
        }
    }
}

impl From<crate::io::IoError> for ImageError {
    fn from(e: crate::io::IoError) -> Self {
        Self::Io(e)
    }
}

/// Auto-detect format from magic bytes and decode.
pub fn decode(data: &[u8]) -> Result<DecodedImage, ImageError> {
    if data.len() >= 2 {
        if data[0] == 0xFF && data[1] == 0xD8 {
            // JPEG: use 1024x1024 as default max size.
            return decode_jpeg(data, 1024, 1024);
        }
        if data[0] == b'B' && data[1] == b'M' {
            return decode_bmp(data);
        }
    }
    Err(ImageError::UnknownFormat)
}

/// Decode a JPEG image using PSP hardware.
///
/// `max_width` and `max_height` specify the maximum output dimensions.
/// The JPEG must fit within these bounds.
pub fn decode_jpeg(
    data: &[u8],
    max_width: i32,
    max_height: i32,
) -> Result<DecodedImage, ImageError> {
    let ret = unsafe { crate::sys::sceJpegInitMJpeg() };
    if ret < 0 {
        return Err(ImageError::JpegError(ret));
    }

    let ret = unsafe { crate::sys::sceJpegCreateMJpeg(max_width, max_height) };
    if ret < 0 {
        unsafe { crate::sys::sceJpegFinishMJpeg() };
        return Err(ImageError::JpegError(ret));
    }

    let buf_size = (max_width as usize) * (max_height as usize) * 4;
    let mut output = alloc::vec![0u8; buf_size];

    let ret = unsafe {
        crate::sys::sceJpegDecodeMJpeg(
            data.as_ptr() as *mut u8,
            data.len(),
            output.as_mut_ptr() as *mut c_void,
            0,
        )
    };

    unsafe {
        crate::sys::sceJpegDeleteMJpeg();
        crate::sys::sceJpegFinishMJpeg();
    }

    if ret < 0 {
        return Err(ImageError::JpegError(ret));
    }

    let width = ((ret >> 16) & 0xFFFF) as u32;
    let height = (ret & 0xFFFF) as u32;
    output.truncate((width * height * 4) as usize);

    Ok(DecodedImage {
        width,
        height,
        format: PixelFormat::Rgba8888,
        data: output,
    })
}

/// Decode an uncompressed 24-bit or 32-bit BMP.
pub fn decode_bmp(data: &[u8]) -> Result<DecodedImage, ImageError> {
    if data.len() < 54 {
        return Err(ImageError::InvalidBmp("file too small"));
    }
    if data[0] != b'B' || data[1] != b'M' {
        return Err(ImageError::InvalidBmp("bad magic"));
    }

    let data_offset = read_u32_le(data, 10) as usize;
    let dib_size = read_u32_le(data, 14);
    if dib_size < 40 {
        return Err(ImageError::InvalidBmp("unsupported DIB header"));
    }

    let width = read_i32_le(data, 18);
    let height_raw = read_i32_le(data, 22);
    let top_down = height_raw < 0;
    let height = if top_down { -height_raw } else { height_raw };
    if width <= 0 || height <= 0 {
        return Err(ImageError::InvalidBmp("invalid dimensions"));
    }
    let width = width as u32;
    let height = height as u32;

    let bpp = read_u16_le(data, 28);
    let compression = read_u32_le(data, 30);
    if compression != 0 {
        return Err(ImageError::InvalidBmp("compressed BMPs not supported"));
    }

    let (format, out_bpp) = match bpp {
        24 => (PixelFormat::Rgb888, 3u32),
        32 => (PixelFormat::Rgba8888, 4u32),
        _ => return Err(ImageError::InvalidBmp("only 24/32-bit supported")),
    };

    let row_stride = ((width * (bpp as u32) + 31) / 32) * 4;
    let mut output = alloc::vec![0u8; (width * height * out_bpp) as usize];

    for y in 0..height {
        let src_y = if top_down { y } else { height - 1 - y };
        let src_offset = data_offset + (src_y * row_stride) as usize;
        let dst_offset = (y * width * out_bpp) as usize;

        if src_offset + (width * (bpp as u32 / 8)) as usize > data.len() {
            return Err(ImageError::InvalidBmp("unexpected end of data"));
        }

        for x in 0..width {
            let si = src_offset + (x * (bpp as u32 / 8)) as usize;
            let di = dst_offset + (x * out_bpp) as usize;
            // BMP stores BGR; convert to RGB.
            output[di] = data[si + 2]; // R
            output[di + 1] = data[si + 1]; // G
            output[di + 2] = data[si]; // B
            if bpp == 32 {
                output[di + 3] = data[si + 3]; // A
            }
        }
    }

    Ok(DecodedImage {
        width,
        height,
        format,
        data: output,
    })
}

/// Load an image from a file path (auto-detect format).
pub fn load(path: &str) -> Result<DecodedImage, ImageError> {
    let data = crate::io::read_to_vec(path)?;
    decode(&data)
}

fn read_u16_le(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

fn read_u32_le(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

fn read_i32_le(data: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}
