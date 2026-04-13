use crate::sync::atomic::{AtomicBool, Ordering};

unsafe extern "C" {
    fn __psp_mt19937_init(ctx: *mut u8, seed: u32);
    fn __psp_mt19937_uint(ctx: *mut u8) -> u32;
    fn __psp_get_system_time_low() -> u32;
}

// MT19937 context is a `{ u32 count, u32 state[624] }` = 2500 bytes,
// but — critically — it needs `u32` alignment. Declaring the backing
// storage as `[u8; 2504]` on PSP would leave it 1-byte-aligned, and
// the MIPS Allegrex `lw`/`sw` instructions issued inside
// `sceKernelUtilsMt19937*` trap on unaligned access on real hardware
// (PPSSPP is lenient and silently permits the misaligned load, which
// is why this bug escaped emulator testing).
//
// Back the context with a `[u32; 626]` static so the compiler
// guarantees a 4-byte-aligned .bss slot, and cast to `*mut u8` only
// at the FFI boundary where the C-ABI helpers rewrap it as a
// `SceKernelUtilsMt19937Context`.
#[repr(align(4))]
struct MtCtx([u32; 626]);
static mut MT_CTX: MtCtx = MtCtx([0u32; 626]);
static MT_INITIALIZED: AtomicBool = AtomicBool::new(false);
static MT_LOCK: AtomicBool = AtomicBool::new(false);

fn acquire_lock() {
    while MT_LOCK
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
}

fn release_lock() {
    MT_LOCK.store(false, Ordering::Release);
}

fn mt_ctx_ptr() -> *mut u8 {
    // SAFETY: `MT_CTX` is `#[repr(align(4))]`, guaranteeing the
    // returned pointer is suitably aligned for the `u32` loads that
    // `sceKernelUtilsMt19937*` issues. The `MT_LOCK` spin-lock
    // serialises concurrent access so the `&raw mut` is not racy.
    unsafe { (&raw mut MT_CTX) as *mut u8 }
}

pub fn fill_bytes(bytes: &mut [u8]) {
    acquire_lock();

    if !MT_INITIALIZED.load(Ordering::Relaxed) {
        let seed = unsafe { __psp_get_system_time_low() };
        unsafe { __psp_mt19937_init(mt_ctx_ptr(), seed) };
        MT_INITIALIZED.store(true, Ordering::Relaxed);
    }

    let mut i = 0;
    while i < bytes.len() {
        let val = unsafe { __psp_mt19937_uint(mt_ctx_ptr()) };
        let val_bytes = val.to_ne_bytes();
        let remaining = bytes.len() - i;
        let to_copy = if remaining < 4 { remaining } else { 4 };
        bytes[i..i + to_copy].copy_from_slice(&val_bytes[..to_copy]);
        i += to_copy;
    }

    release_lock();
}
