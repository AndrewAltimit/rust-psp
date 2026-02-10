//! On-Screen Keyboard (OSK) for text input on the PSP.
//!
//! Wraps `sceUtilityOsk*` to provide a safe API for displaying the
//! system keyboard and capturing user text input.
//!
//! # Example
//!
//! ```ignore
//! use psp::osk;
//!
//! if let Ok(Some(text)) = osk::text_input("Enter your name:", 32) {
//!     psp::dprintln!("Hello, {}!", text);
//! }
//! ```

use alloc::string::String;
use alloc::vec::Vec;

use crate::sys::{
    SceUtilityOskData, SceUtilityOskInputLanguage, SceUtilityOskInputType, SceUtilityOskParams,
    SceUtilityOskResult, SystemParamLanguage, UtilityDialogButtonAccept, UtilityDialogCommon,
};

/// Error from an OSK operation, wrapping the raw SCE error code.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct OskError(pub i32);

impl core::fmt::Debug for OskError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "OskError({:#010x})", self.0 as u32)
    }
}

impl core::fmt::Display for OskError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "osk error {:#010x}", self.0 as u32)
    }
}

/// Standard thread priorities for utility dialogs.
const GRAPHICS_THREAD: i32 = 0x11;
const ACCESS_THREAD: i32 = 0x13;
const FONT_THREAD: i32 = 0x12;
const SOUND_THREAD: i32 = 0x10;

/// Maximum iterations for OSK polling (~30 seconds at 60 fps).
const MAX_OSK_ITERATIONS: u32 = 1800;

fn make_common(size: u32) -> UtilityDialogCommon {
    UtilityDialogCommon {
        size,
        language: SystemParamLanguage::English,
        button_accept: UtilityDialogButtonAccept::Cross,
        graphics_thread: GRAPHICS_THREAD,
        access_thread: ACCESS_THREAD,
        font_thread: FONT_THREAD,
        sound_thread: SOUND_THREAD,
        result: 0,
        reserved: [0i32; 4],
    }
}

/// Show a simple text input dialog and return the entered text.
///
/// Returns `Ok(Some(text))` if the user confirmed, `Ok(None)` if cancelled,
/// or `Err` on failure.
pub fn text_input(prompt: &str, max_chars: usize) -> Result<Option<String>, OskError> {
    OskBuilder::new(prompt).max_chars(max_chars).show()
}

/// Builder for customized OSK dialogs.
pub struct OskBuilder {
    prompt_utf16: Vec<u16>,
    initial_utf16: Vec<u16>,
    max_chars: usize,
    input_type: SceUtilityOskInputType,
    language: SceUtilityOskInputLanguage,
}

impl OskBuilder {
    /// Create a new OSK builder with the given prompt text.
    pub fn new(prompt: &str) -> Self {
        Self {
            prompt_utf16: str_to_utf16(prompt),
            initial_utf16: alloc::vec![0u16],
            max_chars: 128,
            input_type: SceUtilityOskInputType::All,
            language: SceUtilityOskInputLanguage::Default,
        }
    }

    /// Set the maximum number of characters the user can enter.
    pub fn max_chars(mut self, max: usize) -> Self {
        self.max_chars = max;
        self
    }

    /// Set initial text in the input field.
    pub fn initial_text(mut self, text: &str) -> Self {
        self.initial_utf16 = str_to_utf16(text);
        self
    }

    /// Set the input language.
    pub fn language(mut self, lang: SceUtilityOskInputLanguage) -> Self {
        self.language = lang;
        self
    }

    /// Set the input type (filter what characters are allowed).
    pub fn input_type(mut self, input_type: SceUtilityOskInputType) -> Self {
        self.input_type = input_type;
        self
    }

    /// Show the OSK dialog and block until the user responds.
    ///
    /// Returns `Ok(Some(text))` if the user confirmed input,
    /// `Ok(None)` if cancelled, or `Err` on failure.
    pub fn show(mut self) -> Result<Option<String>, OskError> {
        let mut output_buf = alloc::vec![0u16; self.max_chars + 1];

        let mut osk_data = SceUtilityOskData {
            unk_00: 0,
            unk_04: 0,
            language: self.language,
            unk_12: 0,
            inputtype: self.input_type,
            lines: 1,
            unk_24: 0,
            desc: self.prompt_utf16.as_mut_ptr(),
            intext: self.initial_utf16.as_mut_ptr(),
            outtextlength: output_buf.len() as i32,
            outtext: output_buf.as_mut_ptr(),
            result: SceUtilityOskResult::Unchanged,
            outtextlimit: self.max_chars as i32,
        };

        let mut params = SceUtilityOskParams {
            base: make_common(core::mem::size_of::<SceUtilityOskParams>() as u32),
            datacount: 1,
            data: &mut osk_data,
            state: crate::sys::PspUtilityDialogState::None,
            unk_60: 0,
        };

        let ret =
            unsafe { crate::sys::sceUtilityOskInitStart(&mut params as *mut SceUtilityOskParams) };
        if ret < 0 {
            return Err(OskError(ret));
        }

        for _ in 0..MAX_OSK_ITERATIONS {
            let status = unsafe { crate::sys::sceUtilityOskGetStatus() };
            match status {
                2 => {
                    unsafe { crate::sys::sceUtilityOskUpdate(1) };
                },
                3 => {
                    unsafe { crate::sys::sceUtilityOskShutdownStart() };
                },
                0 => break,
                _ => {},
            }
            unsafe { crate::sys::sceDisplayWaitVblankStart() };
        }

        match osk_data.result {
            SceUtilityOskResult::Changed => {
                let text = utf16_to_string(&output_buf);
                Ok(Some(text))
            },
            _ => Ok(None),
        }
    }
}

/// Convert a &str to a null-terminated UTF-16 Vec.
fn str_to_utf16(s: &str) -> Vec<u16> {
    let mut buf: Vec<u16> = s.encode_utf16().collect();
    buf.push(0);
    buf
}

/// Convert a null-terminated UTF-16 buffer to a String.
fn utf16_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}
