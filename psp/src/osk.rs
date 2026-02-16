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

/// Small display list for utility dialog GU frames (16KB, 16-byte aligned).
#[repr(C, align(16))]
struct Align16<T>(T);
static mut DIALOG_LIST: Align16<[u8; 0x4000]> = Align16([0u8; 0x4000]);

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

        // Close the caller's open GU display list so the utility dialog
        // can render into the framebuffer.
        // SAFETY: sceGuFinish/sceGuSync are GU FFI calls. The caller's
        // display list was opened by sceGuStart in swap_buffers or init.
        unsafe {
            crate::sys::sceGuFinish();
            crate::sys::sceGuSync(
                crate::sys::GuSyncMode::Finish,
                crate::sys::GuSyncBehavior::Wait,
            );
        }

        for _ in 0..MAX_OSK_ITERATIONS {
            let status = unsafe { crate::sys::sceUtilityOskGetStatus() };
            if status == 0 || status < 0 {
                break;
            }

            // Provide a GU frame with a cleared screen as the dialog
            // background, then close the frame before updating the
            // utility dialog.  PSPSDK convention: the dialog update
            // must be called **outside** any open GU display list.
            // SAFETY: DIALOG_LIST is only used by utility dialog loops
            // which run on the main thread and never overlap.
            unsafe {
                crate::sys::sceGuStart(
                    crate::sys::GuContextType::Direct,
                    &raw mut DIALOG_LIST as *mut core::ffi::c_void,
                );
                crate::sys::sceGuClearColor(0xff00_0000); // opaque black
                crate::sys::sceGuClear(crate::sys::ClearBuffer::COLOR_BUFFER_BIT);
                crate::sys::sceGuFinish();
                crate::sys::sceGuSync(
                    crate::sys::GuSyncMode::Finish,
                    crate::sys::GuSyncBehavior::Wait,
                );
            }

            // Update the utility dialog outside the GU frame.
            match status {
                2 => unsafe {
                    crate::sys::sceUtilityOskUpdate(1);
                },
                3 => unsafe {
                    crate::sys::sceUtilityOskShutdownStart();
                },
                _ => {},
            }

            // SAFETY: Present the frame.
            unsafe {
                crate::sys::sceDisplayWaitVblankStart();
                crate::sys::sceGuSwapBuffers();
            }
        }

        // If the dialog is still active after the polling loop (timeout or
        // error), force-shutdown so subsequent utility calls don't fail
        // with UTILITY_INVALID_STATUS.
        let final_status = unsafe { crate::sys::sceUtilityOskGetStatus() };
        if final_status > 0 {
            unsafe {
                crate::sys::sceUtilityOskShutdownStart();
            }
            // Drain the shutdown state machine.
            for _ in 0..120 {
                let s = unsafe { crate::sys::sceUtilityOskGetStatus() };
                if s == 0 || s < 0 {
                    break;
                }
                unsafe {
                    crate::sys::sceDisplayWaitVblankStart();
                }
            }
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
