//! Save and load game data using psp::savedata.
//!
//! The savedata utility renders via the GE, so GU must be initialized
//! before calling save/load.

#![no_std]
#![no_main]

use core::ffi::c_void;
use psp::savedata::Savedata;
use psp::sys::{
    self, DisplayPixelFormat, FrontFaceDirection, GuContextType, GuState, GuSyncBehavior,
    GuSyncMode, ShadingModel,
};

psp::module!("savedata_example", 1, 1);

static mut LIST: psp::Align16<[u32; 262144]> = psp::Align16([0; 262144]);

unsafe fn setup_gu() {
    sys::sceGuInit();
    sys::sceGuStart(GuContextType::Direct, &raw mut LIST as *mut c_void);
    sys::sceGuDrawBuffer(DisplayPixelFormat::Psm8888, core::ptr::null_mut(), 512);
    sys::sceGuDispBuffer(480, 272, 0x88000 as *mut c_void, 512);
    sys::sceGuDepthBuffer(0x110000 as *mut c_void, 512);
    sys::sceGuOffset(2048 - 240, 2048 - 136);
    sys::sceGuViewport(2048, 2048, 480, 272);
    sys::sceGuScissor(0, 0, 480, 272);
    sys::sceGuEnable(GuState::ScissorTest);
    sys::sceGuFrontFace(FrontFaceDirection::Clockwise);
    sys::sceGuShadeModel(ShadingModel::Smooth);
    sys::sceGuEnable(GuState::CullFace);
    sys::sceGuFinish();
    sys::sceGuSync(GuSyncMode::Finish, GuSyncBehavior::Wait);
    sys::sceDisplayWaitVblankStart();
    sys::sceGuDisplay(true);
}

fn psp_main() {
    psp::callback::setup_exit_callback().unwrap();

    unsafe {
        setup_gu();
    }

    // Save some data.
    let save_data = b"Hello from rust-psp savedata!";
    let game_name = b"RUSTPSP00000\0";
    let save_name = b"SAVE0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";

    psp::dprintln!("Saving {} bytes...", save_data.len());
    match Savedata::new(game_name)
        .title("Rust PSP Save")
        .detail("Example save data")
        .save(save_name, save_data)
    {
        Ok(()) => psp::dprintln!("Save successful!"),
        Err(e) => {
            psp::dprintln!("Save failed: {:?}", e);
            return;
        },
    }

    // Load it back.
    psp::dprintln!("Loading...");
    match Savedata::new(game_name).load(save_name, 1024) {
        Ok(data) => {
            let text = core::str::from_utf8(&data).unwrap_or("<binary>");
            psp::dprintln!("Loaded {} bytes: {}", data.len(), text);
        },
        Err(e) => psp::dprintln!("Load failed: {:?}", e),
    }

    unsafe {
        sys::sceKernelExitGame();
    }
}
