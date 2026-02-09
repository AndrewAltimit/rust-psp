use crate::sync::atomic::{AtomicBool, Ordering};

unsafe extern "C" {
    fn __psp_mt19937_init(ctx: *mut u8, seed: u32);
    fn __psp_mt19937_uint(ctx: *mut u8) -> u32;
    fn __psp_get_system_time_low() -> u32;
}

// MT19937 context is 2504 bytes (624 u32s + index)
static mut MT_CTX: [u8; 2504] = [0u8; 2504];
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

pub fn fill_bytes(bytes: &mut [u8]) {
    acquire_lock();

    if !MT_INITIALIZED.load(Ordering::Relaxed) {
        let seed = unsafe { __psp_get_system_time_low() };
        unsafe { __psp_mt19937_init(MT_CTX.as_mut_ptr(), seed) };
        MT_INITIALIZED.store(true, Ordering::Relaxed);
    }

    let mut i = 0;
    while i < bytes.len() {
        let val = unsafe { __psp_mt19937_uint(MT_CTX.as_mut_ptr()) };
        let val_bytes = val.to_ne_bytes();
        let remaining = bytes.len() - i;
        let to_copy = if remaining < 4 { remaining } else { 4 };
        bytes[i..i + to_copy].copy_from_slice(&val_bytes[..to_copy]);
        i += to_copy;
    }

    release_lock();
}
