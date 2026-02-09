use crate::cell::UnsafeCell;
use crate::sync::atomic::{AtomicI32, Ordering};

unsafe extern "C" {
    fn __psp_lwmutex_create(work: *mut u8, name: *const u8, attr: u32, count: i32) -> i32;
    fn __psp_lwmutex_lock(work: *mut u8, count: i32, timeout: *mut u32) -> i32;
    fn __psp_lwmutex_unlock(work: *mut u8, count: i32) -> i32;
    fn __psp_lwmutex_trylock(work: *mut u8, count: i32) -> i32;
    fn __psp_lwmutex_delete(work: *mut u8) -> i32;
}

/// SceKernelLwMutexWork is 32 bytes (8 x u32).
/// We use lazy initialization: first lock initializes the mutex.
const LWMUTEX_WORK_SIZE: usize = 32;

pub struct Mutex {
    // SceKernelLwMutexWork storage
    work: UnsafeCell<[u8; LWMUTEX_WORK_SIZE]>,
    // Lazy initialization state: 0 = uninitialized, 1 = initialized, -1 = poisoned
    state: AtomicI32,
}

unsafe impl Send for Mutex {}
unsafe impl Sync for Mutex {}

impl Mutex {
    pub const fn new() -> Mutex {
        Mutex {
            work: UnsafeCell::new([0u8; LWMUTEX_WORK_SIZE]),
            state: AtomicI32::new(0),
        }
    }

    fn ensure_init(&self) {
        let s = self.state.load(Ordering::Acquire);
        if s == 1 {
            return;
        }
        if s == 0 {
            // Try to claim initialization
            if self.state.compare_exchange(0, -1, Ordering::AcqRel, Ordering::Acquire).is_ok() {
                let name = b"std_mtx\0";
                let ret = unsafe {
                    __psp_lwmutex_create(
                        self.work.get() as *mut u8,
                        name.as_ptr(),
                        0, // Attr: no recursion
                        0, // Initial lock count
                    )
                };
                if ret >= 0 {
                    self.state.store(1, Ordering::Release);
                } else {
                    // Reset to uninitialized so future attempts can retry,
                    // then abort. Using panic!() here could recurse during
                    // global initialization (panic handler may need a mutex).
                    self.state.store(0, Ordering::Release);
                    unsafe extern "C" {
                        fn __psp_abort() -> !;
                    }
                    unsafe { __psp_abort() };
                }
                return;
            }
        }
        // Another thread is initializing -- spin until done
        while self.state.load(Ordering::Acquire) == -1 {
            core::hint::spin_loop();
        }
    }

    pub fn lock(&self) {
        self.ensure_init();
        unsafe {
            __psp_lwmutex_lock(self.work.get() as *mut u8, 1, core::ptr::null_mut());
        }
    }

    pub unsafe fn unlock(&self) {
        unsafe {
            __psp_lwmutex_unlock(self.work.get() as *mut u8, 1);
        }
    }

    pub fn try_lock(&self) -> bool {
        self.ensure_init();
        let ret = unsafe { __psp_lwmutex_trylock(self.work.get() as *mut u8, 1) };
        ret >= 0
    }
}

impl Drop for Mutex {
    fn drop(&mut self) {
        if *self.state.get_mut() == 1 {
            unsafe {
                __psp_lwmutex_delete(self.work.get() as *mut u8);
            }
        }
    }
}
