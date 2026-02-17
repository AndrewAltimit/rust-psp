//! System dialog wrappers for the PSP.
//!
//! Provides simple blocking functions for the PSP's built-in message
//! dialogs, hiding the Init→Update→GetStatus→Shutdown state machine.
//!
//! # Example
//!
//! ```ignore
//! use psp::dialog;
//!
//! let result = dialog::message_dialog("Hello from Rust!").unwrap();
//! if result == dialog::DialogResult::Confirm {
//!     // User pressed OK
//! }
//! ```

use crate::sys::{
    SystemParamLanguage, UtilityDialogButtonAccept, UtilityDialogCommon, UtilityMsgDialogMode,
    UtilityMsgDialogOption, UtilityMsgDialogParams, UtilityMsgDialogPressed,
};

/// Result of a dialog interaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogResult {
    /// User confirmed (pressed OK / Yes).
    Confirm,
    /// User cancelled (pressed No).
    Cancel,
    /// User closed the dialog (pressed Back).
    Closed,
}

/// Error from a dialog operation, wrapping the raw SCE error code.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DialogError(pub i32);

impl core::fmt::Debug for DialogError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "DialogError({:#010x})", self.0 as u32)
    }
}

impl core::fmt::Display for DialogError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "dialog error {:#010x}", self.0 as u32)
    }
}

/// Standard thread priorities for utility dialogs (from PSPSDK convention).
const GRAPHICS_THREAD: i32 = 0x11;
const ACCESS_THREAD: i32 = 0x13;
const FONT_THREAD: i32 = 0x12;
const SOUND_THREAD: i32 = 0x10;

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

fn make_message_buf(message: &str) -> [u8; 512] {
    let mut msg = [0u8; 512];
    let len = message.len().min(511);
    msg[..len].copy_from_slice(&message.as_bytes()[..len]);
    msg
}

/// Maximum iterations for dialog polling (~10 seconds at 60 fps).
const MAX_DIALOG_ITERATIONS: u32 = 600;

/// Small display list for utility dialog GU frames (16KB, 16-byte aligned).
#[repr(C, align(16))]
pub(crate) struct Align16<T>(pub T);
/// Shared display list for all utility dialog loops. Only accessed from
/// the main thread, never concurrently.
pub(crate) static mut DIALOG_LIST: Align16<[u8; 0x4000]> = Align16([0u8; 0x4000]);

/// Create a `UtilityDialogCommon` for the netconf dialog.
pub(crate) fn make_netconf_common(size: u32) -> UtilityDialogCommon {
    make_common(size)
}

fn run_dialog(params: &mut UtilityMsgDialogParams) -> Result<DialogResult, DialogError> {
    let ret =
        unsafe { crate::sys::sceUtilityMsgDialogInitStart(params as *mut UtilityMsgDialogParams) };
    if ret < 0 {
        return Err(DialogError(ret));
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

    for _ in 0..MAX_DIALOG_ITERATIONS {
        let status = unsafe { crate::sys::sceUtilityMsgDialogGetStatus() };
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
                crate::sys::sceUtilityMsgDialogUpdate(1);
            },
            3 => unsafe {
                crate::sys::sceUtilityMsgDialogShutdownStart();
            },
            _ => {},
        }

        // SAFETY: Present the frame.
        unsafe {
            crate::sys::sceDisplayWaitVblankStart();
            crate::sys::sceGuSwapBuffers();
        }
    }

    // If the dialog reached QUIT (3) or FINISHED (4) but the polling
    // loop exited before it drained to NONE (0), finish the shutdown.
    for _ in 0..120 {
        let s = unsafe { crate::sys::sceUtilityMsgDialogGetStatus() };
        match s {
            3 => unsafe {
                crate::sys::sceUtilityMsgDialogShutdownStart();
                crate::sys::sceDisplayWaitVblankStart();
            },
            4 => unsafe {
                crate::sys::sceDisplayWaitVblankStart();
            },
            _ => break,
        }
    }

    Ok(match params.button_pressed {
        UtilityMsgDialogPressed::Yes => DialogResult::Confirm,
        UtilityMsgDialogPressed::No => DialogResult::Cancel,
        UtilityMsgDialogPressed::Back => DialogResult::Closed,
        UtilityMsgDialogPressed::Unknown1 => DialogResult::Confirm,
    })
}

/// Show a blocking message dialog with an OK button.
pub fn message_dialog(message: &str) -> Result<DialogResult, DialogError> {
    let mut params = UtilityMsgDialogParams {
        base: make_common(core::mem::size_of::<UtilityMsgDialogParams>() as u32),
        unknown: 0,
        mode: UtilityMsgDialogMode::Text,
        error_value: 0,
        message: make_message_buf(message),
        options: UtilityMsgDialogOption::TEXT,
        button_pressed: UtilityMsgDialogPressed::Unknown1,
    };
    run_dialog(&mut params)
}

/// Show a blocking Yes/No confirmation dialog.
pub fn confirm_dialog(message: &str) -> Result<DialogResult, DialogError> {
    let mut params = UtilityMsgDialogParams {
        base: make_common(core::mem::size_of::<UtilityMsgDialogParams>() as u32),
        unknown: 0,
        mode: UtilityMsgDialogMode::Text,
        error_value: 0,
        message: make_message_buf(message),
        options: UtilityMsgDialogOption::TEXT | UtilityMsgDialogOption::YES_NO_BUTTONS,
        button_pressed: UtilityMsgDialogPressed::Unknown1,
    };
    run_dialog(&mut params)
}

/// Show a blocking error code dialog.
pub fn error_dialog(error_code: u32) -> Result<DialogResult, DialogError> {
    let mut params = UtilityMsgDialogParams {
        base: make_common(core::mem::size_of::<UtilityMsgDialogParams>() as u32),
        unknown: 0,
        mode: UtilityMsgDialogMode::Error,
        error_value: error_code,
        message: [0u8; 512],
        options: UtilityMsgDialogOption::ERROR,
        button_pressed: UtilityMsgDialogPressed::Unknown1,
    };
    run_dialog(&mut params)
}

/// Builder for customized message dialogs.
pub struct MessageDialogBuilder {
    message: [u8; 512],
    mode: UtilityMsgDialogMode,
    options: UtilityMsgDialogOption,
    language: SystemParamLanguage,
    error_value: u32,
}

impl MessageDialogBuilder {
    /// Create a new builder for a text message dialog.
    pub fn new(message: &str) -> Self {
        Self {
            message: make_message_buf(message),
            mode: UtilityMsgDialogMode::Text,
            options: UtilityMsgDialogOption::TEXT,
            language: SystemParamLanguage::English,
            error_value: 0,
        }
    }

    /// Set the dialog language.
    pub fn language(mut self, lang: SystemParamLanguage) -> Self {
        self.language = lang;
        self
    }

    /// Enable Yes/No buttons instead of just OK.
    pub fn yes_no(mut self) -> Self {
        self.options |= UtilityMsgDialogOption::YES_NO_BUTTONS;
        self
    }

    /// Set dialog to error mode with the given error code.
    pub fn error_mode(mut self, code: u32) -> Self {
        self.mode = UtilityMsgDialogMode::Error;
        self.options = UtilityMsgDialogOption::ERROR;
        self.error_value = code;
        self
    }

    /// Show the dialog and block until the user responds.
    pub fn show(self) -> Result<DialogResult, DialogError> {
        let mut base = make_common(core::mem::size_of::<UtilityMsgDialogParams>() as u32);
        base.language = self.language;

        let mut params = UtilityMsgDialogParams {
            base,
            unknown: 0,
            mode: self.mode,
            error_value: self.error_value,
            message: self.message,
            options: self.options,
            button_pressed: UtilityMsgDialogPressed::Unknown1,
        };
        run_dialog(&mut params)
    }
}
