//! Synchronization primitives for the PSP.
//!
//! The PSP is a single-core MIPS R4000 processor, so these primitives use
//! atomic operations primarily to prevent re-entrant access from interrupt
//! handlers and to provide proper compiler ordering barriers.
//!
//! # Primitives
//!
//! - [`SpinMutex<T>`]: Exclusive-access spinlock (extracted from `debug.rs`)
//! - [`SpinRwLock<T>`]: Reader-writer spinlock for shared-read / exclusive-write
//! - [`SpscQueue<T, N>`]: Lock-free single-producer single-consumer ring buffer
//! - [`UncachedBox<T>`]: Heap-allocated box in uncached (ME-accessible) memory

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

// ── SpinMutex ───────────────────────────────────────────────────────

/// A simple spinlock for single-core environments (PSP MIPS R4000).
///
/// Uses `AtomicBool` with acquire/release ordering. On the single-core PSP
/// this prevents compiler reordering; on multi-core it would provide proper
/// synchronization too.
///
/// # Example
///
/// ```ignore
/// use psp::sync::SpinMutex;
///
/// static COUNTER: SpinMutex<u32> = SpinMutex::new(0);
///
/// let mut guard = COUNTER.lock();
/// *guard += 1;
/// ```
pub struct SpinMutex<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

// SAFETY: SpinMutex provides exclusive access via the atomic lock.
// PSP is single-core, so the spinlock prevents re-entrant access from
// interrupt handlers or coroutines.
unsafe impl<T: Send> Sync for SpinMutex<T> {}
unsafe impl<T: Send> Send for SpinMutex<T> {}

impl<T> SpinMutex<T> {
    /// Create a new `SpinMutex` wrapping `val`.
    pub const fn new(val: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(val),
        }
    }

    /// Acquire the lock, spinning until it becomes available.
    ///
    /// Returns a RAII guard that releases the lock on drop.
    pub fn lock(&self) -> SpinGuard<'_, T> {
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        SpinGuard { mutex: self }
    }

    /// Try to acquire the lock without spinning.
    ///
    /// Returns `None` if the lock is already held.
    pub fn try_lock(&self) -> Option<SpinGuard<'_, T>> {
        if self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(SpinGuard { mutex: self })
        } else {
            None
        }
    }
}

/// RAII guard for [`SpinMutex`]. Releases the lock when dropped.
pub struct SpinGuard<'a, T> {
    mutex: &'a SpinMutex<T>,
}

impl<T> core::ops::Deref for SpinGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: We hold the lock.
        unsafe { &*self.mutex.data.get() }
    }
}

impl<T> core::ops::DerefMut for SpinGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: We hold the lock exclusively.
        unsafe { &mut *self.mutex.data.get() }
    }
}

impl<T> Drop for SpinGuard<'_, T> {
    fn drop(&mut self) {
        self.mutex.locked.store(false, Ordering::Release);
    }
}

// ── SpinRwLock ──────────────────────────────────────────────────────

/// A reader-writer spinlock.
///
/// Allows multiple concurrent readers or one exclusive writer.
/// Useful for the "UI reads state while IO writes" pattern.
///
/// The state is encoded in a single `AtomicU32`:
/// - `0` = unlocked
/// - `WRITER_BIT` set = write-locked
/// - Otherwise, the value is the reader count
///
/// # Example
///
/// ```ignore
/// use psp::sync::SpinRwLock;
///
/// static STATE: SpinRwLock<GameState> = SpinRwLock::new(GameState::default());
///
/// // Reader (UI thread):
/// let guard = STATE.read();
/// draw_ui(&*guard);
///
/// // Writer (IO thread):
/// let mut guard = STATE.write();
/// guard.score += 10;
/// ```
pub struct SpinRwLock<T> {
    /// 0 = unlocked, WRITER_BIT = write-locked, else reader count
    state: AtomicU32,
    data: UnsafeCell<T>,
}

const WRITER_BIT: u32 = 1 << 31;

// SAFETY: SpinRwLock provides reader/writer exclusion via atomic state.
unsafe impl<T: Send> Send for SpinRwLock<T> {}
unsafe impl<T: Send + Sync> Sync for SpinRwLock<T> {}

impl<T> SpinRwLock<T> {
    /// Create a new `SpinRwLock` wrapping `val`.
    pub const fn new(val: T) -> Self {
        Self {
            state: AtomicU32::new(0),
            data: UnsafeCell::new(val),
        }
    }

    /// Acquire a read lock, spinning until no writer holds the lock.
    pub fn read(&self) -> ReadGuard<'_, T> {
        loop {
            let s = self.state.load(Ordering::Relaxed);
            // Cannot acquire read lock while a writer holds it
            if s & WRITER_BIT != 0 {
                core::hint::spin_loop();
                continue;
            }
            // Try to increment the reader count
            if self
                .state
                .compare_exchange_weak(s, s + 1, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                return ReadGuard { lock: self };
            }
            core::hint::spin_loop();
        }
    }

    /// Try to acquire a read lock without spinning.
    pub fn try_read(&self) -> Option<ReadGuard<'_, T>> {
        let s = self.state.load(Ordering::Relaxed);
        if s & WRITER_BIT != 0 {
            return None;
        }
        if self
            .state
            .compare_exchange(s, s + 1, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(ReadGuard { lock: self })
        } else {
            None
        }
    }

    /// Acquire a write lock, spinning until all readers and writers release.
    pub fn write(&self) -> WriteGuard<'_, T> {
        loop {
            if self
                .state
                .compare_exchange_weak(0, WRITER_BIT, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                return WriteGuard { lock: self };
            }
            core::hint::spin_loop();
        }
    }

    /// Try to acquire a write lock without spinning.
    pub fn try_write(&self) -> Option<WriteGuard<'_, T>> {
        if self
            .state
            .compare_exchange(0, WRITER_BIT, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(WriteGuard { lock: self })
        } else {
            None
        }
    }
}

/// RAII read guard for [`SpinRwLock`].
pub struct ReadGuard<'a, T> {
    lock: &'a SpinRwLock<T>,
}

impl<T> core::ops::Deref for ReadGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: Read lock is held; no writer can exist.
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> Drop for ReadGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.state.fetch_sub(1, Ordering::Release);
    }
}

/// RAII write guard for [`SpinRwLock`].
pub struct WriteGuard<'a, T> {
    lock: &'a SpinRwLock<T>,
}

impl<T> core::ops::Deref for WriteGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: Write lock is held exclusively.
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> core::ops::DerefMut for WriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: Write lock is held exclusively.
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T> Drop for WriteGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.state.store(0, Ordering::Release);
    }
}

// ── SPSC Ring Buffer ────────────────────────────────────────────────

/// A lock-free single-producer single-consumer (SPSC) ring buffer.
///
/// This is the fundamental primitive for ME↔CPU and IO-thread↔UI-thread
/// communication. One thread pushes, one thread pops — no locks needed.
///
/// `N` must be a power of two for efficient modular indexing.
///
/// # Example
///
/// ```ignore
/// use psp::sync::SpscQueue;
///
/// static QUEUE: SpscQueue<u32, 64> = SpscQueue::new();
///
/// // Producer thread:
/// QUEUE.push(42);
///
/// // Consumer thread:
/// if let Some(val) = QUEUE.pop() {
///     assert_eq!(val, 42);
/// }
/// ```
pub struct SpscQueue<T, const N: usize> {
    head: AtomicU32,
    tail: AtomicU32,
    buf: UnsafeCell<[MaybeUninit<T>; N]>,
}

// SAFETY: Only one producer and one consumer are expected.
// The atomic head/tail provide the necessary synchronization.
unsafe impl<T: Send, const N: usize> Send for SpscQueue<T, N> {}
unsafe impl<T: Send, const N: usize> Sync for SpscQueue<T, N> {}

impl<T, const N: usize> SpscQueue<T, N> {
    const _ASSERT_POWER_OF_TWO: () = assert!(
        N > 0 && (N & (N - 1)) == 0,
        "SpscQueue capacity must be a power of two"
    );

    /// Create a new empty `SpscQueue`.
    pub const fn new() -> Self {
        // Trigger the compile-time assertion
        #[allow(clippy::let_unit_value)]
        let _ = Self::_ASSERT_POWER_OF_TWO;

        // SAFETY: An array of MaybeUninit doesn't require initialization
        let buf = unsafe { MaybeUninit::<[MaybeUninit<T>; N]>::uninit().assume_init() };
        Self {
            head: AtomicU32::new(0),
            tail: AtomicU32::new(0),
            buf: UnsafeCell::new(buf),
        }
    }

    const MASK: u32 = (N - 1) as u32;

    /// Push a value into the queue.
    ///
    /// Returns `Err(val)` if the queue is full.
    pub fn push(&self, val: T) -> Result<(), T> {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);

        if tail.wrapping_sub(head) >= N as u32 {
            return Err(val);
        }

        let idx = (tail & Self::MASK) as usize;
        // SAFETY: We are the sole producer, and we've verified there's space.
        unsafe {
            let slot = &mut (*self.buf.get())[idx];
            slot.write(val);
        }

        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        Ok(())
    }

    /// Pop a value from the queue.
    ///
    /// Returns `None` if the queue is empty.
    pub fn pop(&self) -> Option<T> {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);

        if head == tail {
            return None;
        }

        let idx = (head & Self::MASK) as usize;
        // SAFETY: We are the sole consumer, and we've verified there's data.
        let val = unsafe {
            let slot = &(*self.buf.get())[idx];
            slot.assume_init_read()
        };

        self.head.store(head.wrapping_add(1), Ordering::Release);
        Some(val)
    }

    /// Returns `true` if the queue is empty.
    pub fn is_empty(&self) -> bool {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        head == tail
    }

    /// Returns the number of items currently in the queue.
    pub fn len(&self) -> u32 {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        tail.wrapping_sub(head)
    }

    /// Returns the total capacity of the queue.
    pub const fn capacity(&self) -> usize {
        N
    }
}

impl<T, const N: usize> Drop for SpscQueue<T, N> {
    fn drop(&mut self) {
        // Drop all remaining items in the queue.
        while self.pop().is_some() {}
    }
}

// ── UncachedBox ─────────────────────────────────────────────────────

/// A heap-allocated box in uncached (partition 3) memory, suitable for
/// sharing data with the Media Engine.
///
/// The ME cannot access cached main RAM coherently — all shared memory must
/// use uncached addresses (OR'd with `0x4000_0000`). `UncachedBox<T>`
/// allocates from ME-accessible partition 3 and returns an uncached pointer.
///
/// `UncachedBox<T>` is `Send` but not `Sync`: it enforces the "one writer"
/// model. Pass it to the ME thread or use it from one side at a time with
/// explicit synchronization.
///
/// # Kernel Mode Required
///
/// This type requires `feature = "kernel"` because partition 3 is only
/// accessible in kernel mode.
///
/// # Example
///
/// ```ignore
/// use psp::sync::UncachedBox;
///
/// let shared = UncachedBox::new(0u32).unwrap();
/// // Pass `shared` to ME task...
/// ```
#[cfg(feature = "kernel")]
pub struct UncachedBox<T> {
    ptr: *mut T,
    block_id: crate::sys::SceUid,
}

// SAFETY: UncachedBox owns its data and can be sent across threads.
// Not Sync — enforces "one writer" model for ME-shared data.
#[cfg(feature = "kernel")]
unsafe impl<T: Send> Send for UncachedBox<T> {}

#[cfg(feature = "kernel")]
impl<T> UncachedBox<T> {
    /// Allocate an `UncachedBox` in ME-accessible partition 3.
    ///
    /// The value is written to uncached memory. Returns an error with the
    /// PSP error code if allocation fails.
    pub fn new(val: T) -> Result<Self, i32> {
        let size = core::mem::size_of::<T>().max(1) as u32;
        // SAFETY: Kernel mode is required; we allocate from partition 3.
        let (ptr, block_id) = unsafe { crate::me::me_alloc(size, b"UncachedBox\0".as_ptr()) }?;
        let typed_ptr = ptr as *mut T;

        // SAFETY: ptr is valid uncached memory of sufficient size.
        unsafe {
            core::ptr::write_volatile(typed_ptr, val);
        }

        Ok(Self {
            ptr: typed_ptr,
            block_id,
        })
    }

    /// Get a raw pointer to the uncached data.
    pub fn as_ptr(&self) -> *const T {
        self.ptr
    }

    /// Get a mutable raw pointer to the uncached data.
    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.ptr
    }

    /// Read the value using volatile access (appropriate for uncached memory).
    ///
    /// # Safety
    ///
    /// The caller must ensure no concurrent writes are in progress (e.g.,
    /// the ME is not currently modifying this data).
    pub unsafe fn read_volatile(&self) -> T {
        unsafe { core::ptr::read_volatile(self.ptr) }
    }

    /// Write a value using volatile access (appropriate for uncached memory).
    ///
    /// # Safety
    ///
    /// The caller must ensure no concurrent reads/writes are in progress.
    pub unsafe fn write_volatile(&mut self, val: T) {
        unsafe { core::ptr::write_volatile(self.ptr, val) }
    }
}

#[cfg(feature = "kernel")]
impl<T> Drop for UncachedBox<T> {
    fn drop(&mut self) {
        unsafe {
            // Drop the inner value before freeing the memory.
            core::ptr::drop_in_place(self.ptr);
            crate::sys::sceKernelFreePartitionMemory(self.block_id);
        }
    }
}

#[cfg(feature = "kernel")]
impl<T: core::fmt::Debug> core::fmt::Debug for UncachedBox<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // SAFETY: Debug access — caller should ensure no concurrent ME writes
        let val = unsafe { core::ptr::read_volatile(self.ptr) };
        f.debug_struct("UncachedBox")
            .field("value", &val)
            .field("ptr", &self.ptr)
            .finish()
    }
}

// ── SyncError ───────────────────────────────────────────────────────

/// Error from a PSP synchronization operation, wrapping the raw SCE error code.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SyncError(pub i32);

impl SyncError {
    pub fn code(self) -> i32 {
        self.0
    }
}

impl core::fmt::Debug for SyncError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "SyncError({:#010x})", self.0 as u32)
    }
}

impl core::fmt::Display for SyncError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "sync error {:#010x}", self.0 as u32)
    }
}

// ── Semaphore ───────────────────────────────────────────────────────

/// A kernel semaphore with RAII cleanup.
///
/// Provides blocking, non-blocking, and timed wait operations backed by
/// `sceKernelCreateSema` / `sceKernelDeleteSema`.
///
/// # Example
///
/// ```ignore
/// use psp::sync::Semaphore;
///
/// let sem = Semaphore::new(b"MySema\0", 0, 1).unwrap();
/// // In producer: sem.signal(1);
/// // In consumer: sem.wait();
/// ```
pub struct Semaphore {
    id: crate::sys::SceUid,
}

// SAFETY: PSP kernel semaphores are designed for cross-thread use.
unsafe impl Send for Semaphore {}
unsafe impl Sync for Semaphore {}

impl Semaphore {
    /// Create a new kernel semaphore.
    ///
    /// - `name`: null-terminated name (e.g. `b"MySema\0"`)
    /// - `init_count`: initial semaphore count
    /// - `max_count`: maximum semaphore count
    pub fn new(name: &[u8], init_count: i32, max_count: i32) -> Result<Self, SyncError> {
        debug_assert!(name.last() == Some(&0), "name must be null-terminated");
        let id = unsafe {
            crate::sys::sceKernelCreateSema(
                name.as_ptr(),
                0, // default attributes
                init_count,
                max_count,
                core::ptr::null_mut(),
            )
        };
        if id.0 < 0 {
            Err(SyncError(id.0))
        } else {
            Ok(Self { id })
        }
    }

    /// Wait (block) until the semaphore count is >= 1, then decrement.
    pub fn wait(&self) -> Result<(), SyncError> {
        let ret = unsafe { crate::sys::sceKernelWaitSema(self.id, 1, core::ptr::null_mut()) };
        if ret < 0 { Err(SyncError(ret)) } else { Ok(()) }
    }

    /// Wait with a timeout in microseconds.
    ///
    /// Returns `Err` on timeout or other error.
    pub fn wait_timeout(&self, us: u32) -> Result<(), SyncError> {
        let mut timeout = us;
        let ret = unsafe { crate::sys::sceKernelWaitSema(self.id, 1, &mut timeout) };
        if ret < 0 { Err(SyncError(ret)) } else { Ok(()) }
    }

    /// Try to decrement the semaphore without blocking.
    ///
    /// Returns `Err` if the count is zero.
    pub fn try_wait(&self) -> Result<(), SyncError> {
        let ret = unsafe { crate::sys::sceKernelPollSema(self.id, 1) };
        if ret < 0 { Err(SyncError(ret)) } else { Ok(()) }
    }

    /// Increment the semaphore count by `count`.
    pub fn signal(&self, count: i32) -> Result<(), SyncError> {
        let ret = unsafe { crate::sys::sceKernelSignalSema(self.id, count) };
        if ret < 0 { Err(SyncError(ret)) } else { Ok(()) }
    }

    /// Get the kernel UID.
    pub fn id(&self) -> crate::sys::SceUid {
        self.id
    }
}

impl Drop for Semaphore {
    fn drop(&mut self) {
        unsafe {
            crate::sys::sceKernelDeleteSema(self.id);
        }
    }
}

// ── EventFlag ───────────────────────────────────────────────────────

/// A kernel event flag with RAII cleanup.
///
/// Provides a bitmask-based synchronization primitive backed by
/// `sceKernelCreateEventFlag` / `sceKernelDeleteEventFlag`.
///
/// # Example
///
/// ```ignore
/// use psp::sync::EventFlag;
/// use psp::sys::{EventFlagAttributes, EventFlagWaitTypes};
///
/// let flag = EventFlag::new(b"MyFlag\0", EventFlagAttributes::empty(), 0).unwrap();
/// // In producer: flag.set(0x01);
/// // In consumer: flag.wait(0x01, EventFlagWaitTypes::OR | EventFlagWaitTypes::CLEAR);
/// ```
pub struct EventFlag {
    id: crate::sys::SceUid,
}

// SAFETY: PSP kernel event flags are designed for cross-thread use.
unsafe impl Send for EventFlag {}
unsafe impl Sync for EventFlag {}

impl EventFlag {
    /// Create a new kernel event flag.
    ///
    /// - `name`: null-terminated name
    /// - `attr`: attributes (e.g. `EventFlagAttributes::WAIT_MULTIPLE`)
    /// - `init_pattern`: initial bit pattern
    pub fn new(
        name: &[u8],
        attr: crate::sys::EventFlagAttributes,
        init_pattern: u32,
    ) -> Result<Self, SyncError> {
        debug_assert!(name.last() == Some(&0), "name must be null-terminated");
        let id = unsafe {
            crate::sys::sceKernelCreateEventFlag(
                name.as_ptr(),
                attr,
                init_pattern as i32,
                core::ptr::null_mut(),
            )
        };
        if id.0 < 0 {
            Err(SyncError(id.0))
        } else {
            Ok(Self { id })
        }
    }

    /// Wait for bits matching `pattern` according to `wait_type`.
    ///
    /// Returns the bit pattern that was matched.
    pub fn wait(
        &self,
        pattern: u32,
        wait_type: crate::sys::EventFlagWaitTypes,
    ) -> Result<u32, SyncError> {
        let mut out_bits: u32 = 0;
        let ret = unsafe {
            crate::sys::sceKernelWaitEventFlag(
                self.id,
                pattern,
                wait_type,
                &mut out_bits,
                core::ptr::null_mut(),
            )
        };
        if ret < 0 {
            Err(SyncError(ret))
        } else {
            Ok(out_bits)
        }
    }

    /// Set bits in the event flag.
    pub fn set(&self, bits: u32) -> Result<(), SyncError> {
        let ret = unsafe { crate::sys::sceKernelSetEventFlag(self.id, bits) };
        if ret < 0 { Err(SyncError(ret)) } else { Ok(()) }
    }

    /// Clear bits in the event flag.
    ///
    /// Bits that are 1 in `bits` are *kept*; bits that are 0 are cleared.
    /// (This matches the PSP kernel semantics: the flag is AND'd with `bits`.)
    pub fn clear(&self, bits: u32) -> Result<(), SyncError> {
        let ret = unsafe { crate::sys::sceKernelClearEventFlag(self.id, bits) };
        if ret < 0 { Err(SyncError(ret)) } else { Ok(()) }
    }

    /// Poll for matching bits without blocking.
    ///
    /// Returns the matched bit pattern, or `Err` if no match.
    pub fn poll(
        &self,
        pattern: u32,
        wait_type: crate::sys::EventFlagWaitTypes,
    ) -> Result<u32, SyncError> {
        let mut out_bits: u32 = 0;
        let ret = unsafe {
            crate::sys::sceKernelPollEventFlag(self.id, pattern, wait_type, &mut out_bits)
        };
        if ret < 0 {
            Err(SyncError(ret))
        } else {
            Ok(out_bits)
        }
    }

    /// Get the kernel UID.
    pub fn id(&self) -> crate::sys::SceUid {
        self.id
    }
}

impl Drop for EventFlag {
    fn drop(&mut self) {
        unsafe {
            crate::sys::sceKernelDeleteEventFlag(self.id);
        }
    }
}
