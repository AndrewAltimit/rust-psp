#![no_std]
#![no_main]

use core::ffi::c_void;

use psp::sys::{self, DisplayPixelFormat, GuState, IoOpenFlags, IoPermissions, TexturePixelFormat};
use psp::vram_alloc::get_vram_allocator;
use psp::{BUF_WIDTH, SCREEN_HEIGHT, SCREEN_WIDTH};

psp::module!("screenshot_example", 1, 1);

static mut LIST: psp::Align16<[u32; 0x40000]> = psp::Align16([0; 0x40000]);

fn psp_main() {
    psp::callback::setup_exit_callback().unwrap();

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

    unsafe {
        sys::sceGuInit();
        sys::sceGuStart(sys::GuContextType::Direct, &raw mut LIST as *mut c_void);
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
        sys::sceGuScissor(0, 0, SCREEN_WIDTH as i32, SCREEN_HEIGHT as i32);
        sys::sceGuEnable(GuState::ScissorTest);
        sys::sceGuFinish();
        sys::sceGuSync(sys::GuSyncMode::Finish, sys::GuSyncBehavior::Wait);
        sys::sceDisplayWaitVblankStart();
        sys::sceGuDisplay(true);

        // Draw a colored background (teal).
        sys::sceGuStart(sys::GuContextType::Direct, &raw mut LIST as *mut c_void);
        sys::sceGuClearColor(0xff_80_40_20); // ABGR: opaque teal-ish
        sys::sceGuClear(sys::ClearBuffer::COLOR_BUFFER_BIT);
        sys::sceGuFinish();
        sys::sceGuSync(sys::GuSyncMode::Finish, sys::GuSyncBehavior::Wait);
        sys::sceDisplayWaitVblankStart();
        sys::sceGuSwapBuffers();

        // Wait one more frame so the display buffer is stable.
        sys::sceDisplayWaitVblankStart();

        // Capture the framebuffer to a BMP.
        let bmp_data = psp::screenshot_bmp();
        psp::dprintln!("Screenshot captured: {} bytes", bmp_data.len());

        // Write the BMP to a file.
        let path = b"host0:/screenshot.bmp\0";
        let fd = sys::sceIoOpen(
            path.as_ptr(),
            IoOpenFlags::WR_ONLY | IoOpenFlags::CREAT | IoOpenFlags::TRUNC,
            0o644 as IoPermissions,
        );

        if fd.0 >= 0 {
            sys::sceIoWrite(fd, bmp_data.as_ptr() as *const c_void, bmp_data.len());
            sys::sceIoClose(fd);
            psp::dprintln!("Screenshot saved to host0:/screenshot.bmp");
        } else {
            psp::dprintln!("Failed to save screenshot: {}", fd.0);
        }
    }
}
