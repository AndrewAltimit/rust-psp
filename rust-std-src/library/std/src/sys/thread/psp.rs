use crate::ffi::CStr;
use crate::io;
use crate::num::NonZero;
use crate::time::Duration;

unsafe extern "C" {
    fn __psp_create_thread(
        name: *const u8,
        entry: unsafe extern "C" fn(usize, *mut u8) -> i32,
        priority: i32,
        stack_size: i32,
        attr: u32,
        opt: *mut u8,
    ) -> i32;
    fn __psp_start_thread(id: i32, arg_len: u32, argp: *mut u8) -> i32;
    fn __psp_wait_thread_end(id: i32, timeout: *mut u32) -> i32;
    fn __psp_delete_thread(id: i32) -> i32;
    fn __psp_delay_thread(us: u32) -> i32;
    fn __psp_get_thread_id() -> i32;
}

pub const DEFAULT_MIN_STACK_SIZE: usize = 64 * 1024; // 64 KiB

pub struct Thread {
    id: i32,
}

unsafe impl Send for Thread {}
unsafe impl Sync for Thread {}

impl Thread {
    // Unsafe because the caller must ensure the entry point is valid and
    // the spawned thread doesn't outlive borrowed data.
    pub unsafe fn new(
        stack: usize,
        p: Box<crate::thread::ThreadInit>,
    ) -> io::Result<Thread> {
        let p = Box::into_raw(p);

        let name = b"rust_thread\0";
        let stack_size = if stack < DEFAULT_MIN_STACK_SIZE {
            DEFAULT_MIN_STACK_SIZE
        } else {
            stack
        };

        let id = unsafe {
            __psp_create_thread(
                name.as_ptr(),
                thread_entry,
                0x20, // Default priority (32)
                stack_size as i32,
                0x8000_0000, // PSP_THREAD_ATTR_USER
                core::ptr::null_mut(),
            )
        };

        if id < 0 {
            // Clean up the box if thread creation failed
            drop(unsafe { Box::from_raw(p) });
            return Err(io::Error::from_raw_os_error(-id));
        }

        // Pass the boxed ThreadInit pointer as the thread argument
        let ret = unsafe { __psp_start_thread(id, 4, p as *mut u8) };

        if ret < 0 {
            drop(unsafe { Box::from_raw(p) });
            unsafe { __psp_delete_thread(id) };
            return Err(io::Error::from_raw_os_error(-ret));
        }

        Ok(Thread { id })
    }

    pub fn yield_now() {
        // Delay of 0 yields the current timeslice
        unsafe { __psp_delay_thread(0) };
    }

    pub fn set_name(_name: &CStr) {
        // PSP doesn't support renaming threads after creation
    }

    pub fn sleep(dur: Duration) {
        let us = dur.as_micros();
        // Clamp to u32::MAX microseconds (~71 minutes)
        let us = if us > u32::MAX as u128 { u32::MAX } else { us as u32 };
        unsafe { __psp_delay_thread(us) };
    }

    pub fn join(self) {
        unsafe {
            __psp_wait_thread_end(self.id, core::ptr::null_mut());
            __psp_delete_thread(self.id);
        }
    }
}

/// Thread entry trampoline. Called by the PSP OS.
///
/// `arg_len` is unused (always 4), `argp` is a pointer to the ThreadInit.
unsafe extern "C" fn thread_entry(_arg_len: usize, argp: *mut u8) -> i32 {
    let init: Box<crate::thread::ThreadInit> =
        unsafe { Box::from_raw(argp as *mut crate::thread::ThreadInit) };
    let main = init.init();
    main();
    0
}

pub fn available_parallelism() -> io::Result<NonZero<usize>> {
    // PSP is single-core MIPS R4000
    Ok(unsafe { NonZero::new_unchecked(1) })
}

pub fn current_os_id() -> Option<u64> {
    let id = unsafe { __psp_get_thread_id() };
    if id >= 0 { Some(id as u64) } else { None }
}

pub fn sleep(dur: Duration) {
    Thread::sleep(dur);
}

pub fn yield_now() {
    Thread::yield_now();
}
