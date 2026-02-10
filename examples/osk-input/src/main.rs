//! On-screen keyboard text input using psp::osk.
//!
//! Demonstrates both the convenience function and the builder pattern.
//! The OSK renders via the GE, so GU must be initialized first.

#![no_std]
#![no_main]

use core::ffi::c_void;
use psp::osk::{self, OskBuilder};
use psp::sys::{
    self, DisplayPixelFormat, FrontFaceDirection, GuContextType, GuState, GuSyncBehavior,
    GuSyncMode, ShadingModel,
};

psp::module!("osk_input_example", 1, 1);

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

    // Simple convenience function: prompt + max chars.
    psp::dprintln!("Opening simple text input...");
    match osk::text_input("Enter your name:", 32) {
        Ok(Some(text)) => psp::dprintln!("Hello, {}!", text),
        Ok(None) => psp::dprintln!("Input cancelled."),
        Err(e) => psp::dprintln!("OSK error: {:?}", e),
    }

    // Builder pattern for more control.
    psp::dprintln!("Opening builder-based input...");
    match OskBuilder::new("What is your favorite color?")
        .max_chars(24)
        .initial_text("blue")
        .show()
    {
        Ok(Some(text)) => psp::dprintln!("Favorite color: {}", text),
        Ok(None) => psp::dprintln!("Input cancelled."),
        Err(e) => psp::dprintln!("OSK error: {:?}", e),
    }

    unsafe {
        sys::sceKernelExitGame();
    }
}
