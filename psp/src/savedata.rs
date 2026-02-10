//! Savedata utility for the PSP.
//!
//! Wraps `sceUtilitySavedata*` to provide a safe, builder-pattern API
//! for saving and loading game data via the PSP's standard save dialog.
//!
//! # Example
//!
//! ```ignore
//! use psp::savedata::Savedata;
//!
//! // Save
//! let data = b"hello world";
//! Savedata::new(b"MYAPP00000\0\0\0")
//!     .title("My Save")
//!     .save(b"SAVE0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0", data)
//!     .unwrap();
//!
//! // Load
//! let loaded = Savedata::new(b"MYAPP00000\0\0\0")
//!     .load(b"SAVE0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0", 1024)
//!     .unwrap();
//! ```

use alloc::vec::Vec;
use core::ffi::c_void;

use crate::sys::{
    SceUtilitySavedataParam, SystemParamLanguage, UtilityDialogButtonAccept, UtilityDialogCommon,
    UtilitySavedataFocus, UtilitySavedataMode, UtilitySavedataSFOParam,
};

/// Error from a savedata operation.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SavedataError(pub i32);

impl core::fmt::Debug for SavedataError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "SavedataError({:#010x})", self.0 as u32)
    }
}

impl core::fmt::Display for SavedataError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "savedata error {:#010x}", self.0 as u32)
    }
}

/// Standard thread priorities for utility dialogs.
const GRAPHICS_THREAD: i32 = 0x11;
const ACCESS_THREAD: i32 = 0x13;
const FONT_THREAD: i32 = 0x12;
const SOUND_THREAD: i32 = 0x10;

/// Maximum iterations for savedata polling (~30 seconds at 60 fps).
const MAX_SAVEDATA_ITERATIONS: u32 = 1800;

fn make_common() -> UtilityDialogCommon {
    UtilityDialogCommon {
        size: core::mem::size_of::<SceUtilitySavedataParam>() as u32,
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

/// Builder for savedata operations.
pub struct Savedata {
    game_name: [u8; 13],
    title: [u8; 128],
    detail: [u8; 1024],
}

impl Savedata {
    /// Create a new savedata builder.
    ///
    /// `game_name` must be exactly 13 bytes (e.g., `b"MYAPP00000\0\0\0"`),
    /// matching the game's product code registered with SCE.
    pub fn new(game_name: &[u8; 13]) -> Self {
        Self {
            game_name: *game_name,
            title: [0u8; 128],
            detail: [0u8; 1024],
        }
    }

    /// Set the save title (shown in the save dialog).
    pub fn title(mut self, title: &str) -> Self {
        let len = title.len().min(127);
        self.title[..len].copy_from_slice(&title.as_bytes()[..len]);
        self
    }

    /// Set the save detail text (shown in the save dialog).
    pub fn detail(mut self, detail: &str) -> Self {
        let len = detail.len().min(1023);
        self.detail[..len].copy_from_slice(&detail.as_bytes()[..len]);
        self
    }

    /// Save data to the specified save slot.
    ///
    /// `save_name` must be exactly 20 bytes (null-padded).
    /// `data` is the raw bytes to save.
    pub fn save(&self, save_name: &[u8; 20], data: &[u8]) -> Result<(), SavedataError> {
        let mut data_buf = Vec::from(data);

        let mut sfo = UtilitySavedataSFOParam {
            title: self.title,
            savedata_title: [0u8; 128],
            detail: self.detail,
            parental_level: 0,
            unknown: [0u8; 3],
        };

        let mut params: SceUtilitySavedataParam = unsafe { core::mem::zeroed() };
        params.base = make_common();
        params.mode = UtilitySavedataMode::AutoSave;
        params.game_name = self.game_name;
        params.save_name = *save_name;
        params.file_name = *b"DATA.BIN\0\0\0\0\0";
        params.data_buf = data_buf.as_mut_ptr() as *mut c_void;
        params.data_buf_size = data_buf.len();
        params.data_size = data_buf.len();
        params.sfo_param = sfo;
        params.focus = UtilitySavedataFocus::Latest;

        self.run_savedata(&mut params)
    }

    /// Load data from the specified save slot.
    ///
    /// `save_name` must be exactly 20 bytes (null-padded).
    /// `max_size` is the maximum expected data size.
    pub fn load(&self, save_name: &[u8; 20], max_size: usize) -> Result<Vec<u8>, SavedataError> {
        let mut data_buf = alloc::vec![0u8; max_size];

        let mut params: SceUtilitySavedataParam = unsafe { core::mem::zeroed() };
        params.base = make_common();
        params.mode = UtilitySavedataMode::AutoLoad;
        params.game_name = self.game_name;
        params.save_name = *save_name;
        params.file_name = *b"DATA.BIN\0\0\0\0\0";
        params.data_buf = data_buf.as_mut_ptr() as *mut c_void;
        params.data_buf_size = data_buf.len();
        params.data_size = 0;
        params.focus = UtilitySavedataFocus::Latest;

        self.run_savedata(&mut params)?;

        let actual_size = params.data_size.min(max_size);
        data_buf.truncate(actual_size);
        Ok(data_buf)
    }

    fn run_savedata(&self, params: &mut SceUtilitySavedataParam) -> Result<(), SavedataError> {
        let ret = unsafe {
            crate::sys::sceUtilitySavedataInitStart(params as *mut SceUtilitySavedataParam)
        };
        if ret < 0 {
            return Err(SavedataError(ret));
        }

        for _ in 0..MAX_SAVEDATA_ITERATIONS {
            let status = unsafe { crate::sys::sceUtilitySavedataGetStatus() };
            match status {
                2 => {
                    unsafe { crate::sys::sceUtilitySavedataUpdate(1) };
                },
                3 => {
                    unsafe { crate::sys::sceUtilitySavedataShutdownStart() };
                },
                0 => break,
                _ => {},
            }
            unsafe { crate::sys::sceDisplayWaitVblankStart() };
        }

        if params.base.result < 0 {
            return Err(SavedataError(params.base.result));
        }

        Ok(())
    }
}
