//! Kernel mode example demonstrating privileged PSP APIs.
//!
//! This example requires custom firmware (ARK-4, PRO, ME CFW, etc.)
//! to run on real hardware. In PPSSPP, it will load but kernel-only
//! syscalls may return errors.

#![no_std]
#![no_main]

psp::module_kernel!("KernelDemo", 1, 0);

fn psp_main() {
    psp::callback::setup_exit_callback().unwrap();

    // Demonstrate kernel-only features
    unsafe {
        // 1. ME clock frequency
        let me_freq = psp::sys::scePowerGetMeClockFrequency();
        psp::dprintln!("ME clock: {}MHz", me_freq);

        // 2. Volatile memory (extra 4MB RAM on PSP-2000+)
        let mut addr: *mut u8 = core::ptr::null_mut();
        let mut size: i32 = 0;
        let ret = psp::sys::sceKernelVolatileMemLock(
            0,
            &mut addr as *mut _ as *mut *mut core::ffi::c_void,
            &mut size,
        );
        if ret == 0 {
            psp::dprintln!("Volatile mem: {:p}, {} bytes", addr, size);
            psp::sys::sceKernelVolatileMemUnlock(0);
        } else {
            psp::dprintln!("Volatile mem lock failed: {}", ret);
        }

        // 3. NAND flash info
        let page_size = psp::sys::sceNandGetPageSize();
        let pages_per_block = psp::sys::sceNandGetPagesPerBlock();
        let total_blocks = psp::sys::sceNandGetTotalBlocks();
        psp::dprintln!(
            "NAND: page={}B, ppb={}, blocks={}",
            page_size,
            pages_per_block,
            total_blocks
        );

        // 4. Hardware register read (GPIO port)
        let gpio_val = psp::hw::hw_read32(psp::hw::GPIO_PORT_READ);
        psp::dprintln!("GPIO port: 0x{:08X}", gpio_val);

        // 5. Type-safe register access
        let gpio_reg = psp::hw::Register::<u32>::new(psp::hw::GPIO_PORT_READ);
        let gpio_val2 = gpio_reg.read();
        psp::dprintln!("GPIO port (via Register): 0x{:08X}", gpio_val2);
    }
}
