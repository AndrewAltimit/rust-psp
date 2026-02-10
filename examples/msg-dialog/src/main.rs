#![no_std]
#![no_main]

use psp::sys::{
    self, DepthFunc, DisplayPixelFormat, FrontFaceDirection, GuContextType, GuState,
    GuSyncBehavior, GuSyncMode, ShadingModel,
};

use core::ffi::c_void;

psp::module!("sample_module", 1, 1);

static mut LIST: psp::Align16<[u32; 262144]> = psp::Align16([0; 262144]);
const SCR_WIDTH: i32 = 480;
const SCR_HEIGHT: i32 = 272;
const BUF_WIDTH: i32 = 512;

unsafe fn setup_gu() {
    sys::sceGuInit();
    sys::sceGuStart(GuContextType::Direct, &raw mut LIST as *mut c_void);
    sys::sceGuDrawBuffer(
        DisplayPixelFormat::Psm8888,
        core::ptr::null_mut(),
        BUF_WIDTH,
    );
    sys::sceGuDispBuffer(SCR_WIDTH, SCR_HEIGHT, 0x88000 as *mut c_void, BUF_WIDTH);
    sys::sceGuDepthBuffer(0x110000 as *mut c_void, BUF_WIDTH);
    sys::sceGuOffset(
        2048 - (SCR_WIDTH as u32 / 2),
        2048 - (SCR_HEIGHT as u32 / 2),
    );
    sys::sceGuViewport(2048, 2048, SCR_WIDTH, SCR_HEIGHT);
    sys::sceGuDepthRange(0xc350, 0x2710);
    sys::sceGuScissor(0, 0, SCR_WIDTH, SCR_HEIGHT);
    sys::sceGuEnable(GuState::ScissorTest);
    sys::sceGuDepthFunc(DepthFunc::GreaterOrEqual);
    sys::sceGuEnable(GuState::DepthTest);
    sys::sceGuFrontFace(FrontFaceDirection::Clockwise);
    sys::sceGuShadeModel(ShadingModel::Smooth);
    sys::sceGuEnable(GuState::CullFace);
    sys::sceGuEnable(GuState::ClipPlanes);
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

    match psp::dialog::message_dialog("Hello from a Rust-created PSP Msg Dialog") {
        Ok(result) => psp::dprintln!("Dialog result: {:?}", result),
        Err(e) => psp::dprintln!("Dialog error: {:?}", e),
    }

    unsafe {
        sys::sceKernelExitGame();
    }
}
