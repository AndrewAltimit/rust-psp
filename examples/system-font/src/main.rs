//! Render text using PSP system fonts via the FontRenderer API.

#![no_std]
#![no_main]

use core::ffi::c_void;

use psp::font::{FontLib, FontRenderer};
use psp::sys::font::{SceFontFamilyCode, SceFontLanguageCode, SceFontStyleCode};
use psp::sys::{
    self, ClearBuffer, DisplayPixelFormat, GuContextType, GuState, GuSyncBehavior, GuSyncMode,
    TexturePixelFormat,
};
use psp::vram_alloc::get_vram_allocator;
use psp::{BUF_WIDTH, SCREEN_HEIGHT, SCREEN_WIDTH};

psp::module!("system_font_example", 1, 1);

static mut LIST: psp::Align16<[u32; 0x40000]> = psp::Align16([0; 0x40000]);

fn psp_main() {
    psp::callback::setup_exit_callback().unwrap();

    // Allocate VRAM for framebuffers and depth.
    let allocator = get_vram_allocator().unwrap();
    let fbp0 = allocator
        .alloc_texture_pixels(BUF_WIDTH, SCREEN_HEIGHT, TexturePixelFormat::Psm8888)
        .unwrap()
        .as_mut_ptr_from_zero();
    let fbp1 = allocator
        .alloc_texture_pixels(BUF_WIDTH, SCREEN_HEIGHT, TexturePixelFormat::Psm8888)
        .unwrap()
        .as_mut_ptr_from_zero();
    let zbp = allocator
        .alloc_texture_pixels(BUF_WIDTH, SCREEN_HEIGHT, TexturePixelFormat::Psm4444)
        .unwrap()
        .as_mut_ptr_from_zero();

    // Allocate 512x512 T8 atlas in VRAM for font glyphs.
    let atlas_vram = allocator
        .alloc_texture_pixels(512, 512, TexturePixelFormat::PsmT8)
        .unwrap()
        .as_mut_ptr_direct();

    // Initialize GU.
    unsafe {
        sys::sceGuInit();
        sys::sceGuStart(GuContextType::Direct, &raw mut LIST as *mut c_void);
        sys::sceGuDrawBuffer(DisplayPixelFormat::Psm8888, fbp0 as _, BUF_WIDTH as i32);
        sys::sceGuDispBuffer(
            SCREEN_WIDTH as i32,
            SCREEN_HEIGHT as i32,
            fbp1 as _,
            BUF_WIDTH as i32,
        );
        sys::sceGuDepthBuffer(zbp as _, BUF_WIDTH as i32);
        sys::sceGuOffset(2048 - (SCREEN_WIDTH / 2), 2048 - (SCREEN_HEIGHT / 2));
        sys::sceGuViewport(2048, 2048, SCREEN_WIDTH as i32, SCREEN_HEIGHT as i32);
        sys::sceGuDepthRange(65535, 0);
        sys::sceGuScissor(0, 0, SCREEN_WIDTH as i32, SCREEN_HEIGHT as i32);
        sys::sceGuEnable(GuState::ScissorTest);
        sys::sceGuFinish();
        sys::sceGuSync(GuSyncMode::Finish, GuSyncBehavior::Wait);
        sys::sceDisplayWaitVblankStart();
        sys::sceGuDisplay(true);
    }

    // Open a system font.
    let fontlib = match FontLib::new(4) {
        Ok(fl) => fl,
        Err(e) => {
            psp::dprintln!("FontLib::new failed: {:?}", e);
            return;
        },
    };

    let font = match fontlib.find_optimum(
        SceFontFamilyCode::SansSerif,
        SceFontStyleCode::Regular,
        SceFontLanguageCode::Latin,
    ) {
        Ok(f) => f,
        Err(e) => {
            psp::dprintln!("find_optimum failed: {:?}", e);
            return;
        },
    };

    let mut renderer = FontRenderer::new(&font, atlas_vram, 16.0);

    // Render loop.
    unsafe {
        loop {
            sys::sceGuStart(GuContextType::Direct, &raw mut LIST as *mut c_void);
            sys::sceGuClearColor(0xff442200);
            sys::sceGuClear(ClearBuffer::COLOR_BUFFER_BIT);

            psp::gu_ext::setup_2d();

            renderer.draw_text(20.0, 30.0, 0xffffffff, "Hello from system fonts!");
            renderer.draw_text(20.0, 60.0, 0xff00ffff, "rust-psp FontRenderer");
            renderer.draw_text(20.0, 90.0, 0xff88ff88, "ABCDEFGHIJKLMNOPQRSTUVWXYZ");
            renderer.draw_text(20.0, 120.0, 0xffff8888, "0123456789 !@#$%^&*()");
            renderer.flush();

            sys::sceGuFinish();
            sys::sceGuSync(GuSyncMode::Finish, GuSyncBehavior::Wait);
            sys::sceDisplayWaitVblankStart();
            sys::sceGuSwapBuffers();
        }
    }
}
