//! PSP system memory allocator and C runtime memory functions.
//!
//! # Allocator design
//!
//! The naive approach (one `sceKernelAllocPartitionMemory` per Rust
//! allocation) hits a hard PSP firmware limit on the number of
//! active kernel memory blocks: at roughly 1500–2000 outstanding
//! blocks the next `sceKernelAllocPartitionMemory` call hangs
//! indefinitely. The limit is independent of free RAM — confirmed
//! by repro on PPSSPP and bisection on the OASIS_OS browser
//! tokenizer (60 KB of HTML produces ~1500 small `String`
//! allocations and works; 70 KB produces ~1800 and the next
//! `Vec::push` hangs while 17 MB of heap remains free).
//!
//! The fix is to reserve **one** large kernel block at startup and
//! run [`linked_list_allocator::Heap`] on top of it. Now every Rust
//! allocation consumes one *userspace* heap node, not one PSP
//! kernel block. The kernel block count stays at one for the whole
//! program no matter how many `String`s and `Vec`s the program
//! creates.
//!
//! `HEAP_SIZE` is sized to leave headroom for VRAM textures, GU
//! command buffer, video decode buffers, and the OS. Kernel-mode
//! PRX modules use a smaller arena (`KERNEL_HEAP_SIZE`) since they
//! share kernel partition memory with the rest of the firmware.

#![allow(unsafe_op_in_unsafe_fn)]

use crate::sys::{self, SceSysMemBlockTypes, SceSysMemPartitionId, SceUid};
use alloc::alloc::{GlobalAlloc, Layout};
use core::{mem, ptr};
use linked_list_allocator::Heap;
use spin::Mutex;

/// Userspace heap arena reserved at first allocation. 8 MB on user
/// builds — sized for the small-allocation churn of typical Rust
/// programs (HTML tokens, DOM nodes, String/Vec growth) without
/// monopolising the user partition. Allocations at or above
/// `LARGE_ALLOC_THRESHOLD` bypass the arena and go directly to
/// `sceKernelAllocPartitionMemory`, so genuinely big buffers like
/// the OASIS browser's 3 MB `Vec<Option<ComputedStyle>>` or video
/// frame buffers don't need to fit inside the arena.
///
/// Sizing math (24 MB user partition):
///
///   EBOOT code (.text + .data): ~5 MB
///   Rust arena (this constant):  8 MB
///   sceMpeg ME workspace:        6 MB (allocated at TV Guide tune)
///   Large-alloc bypass blocks:   ~3 MB (browser ComputedStyle + buffers)
///   Audio + GU + scratch:       ~1 MB
///   ──────────────────────────────────
///   Total:                     ~23 MB
///
/// Bumping the arena above ~9 MB starves `AvcDecoder::new`'s 6 MB
/// allocation and crashes TV Guide channel-tune. Don't do that.
#[cfg(not(feature = "kernel"))]
const HEAP_SIZE: usize = 8 * 1024 * 1024;

/// Kernel-mode arena. Kernel partition is much smaller (~512 KB
/// shared with the firmware) so reserve correspondingly less. The
/// kernel PRX use case is overlay UI + audio routing — small
/// allocation footprint compared to the user-mode browser.
#[cfg(feature = "kernel")]
const HEAP_SIZE: usize = 256 * 1024;

/// Maximum supported alignment (must be a power of 2). We store the
/// alignment padding offset in a single `u8` byte before the user
/// pointer.
const MAX_ALIGN: usize = 128;

/// Allocations at or above this size bypass the userspace arena and
/// go directly to `sceKernelAllocPartitionMemory`. They cost one
/// kernel block apiece, but only a handful of such blocks ever
/// exist at once (video frame buffers, MP4 download buffers, image
/// decode scratch). Smaller allocations stay in the arena where the
/// kernel block count problem matters.
///
/// 256 KB is below the OASIS_OS PSP video decoder's 544 KB frame
/// buffer (one allocation per double-buffered frame slot) and below
/// the streaming MP4 download buffer (~1-3 MB), so the regression
/// where TV Guide channel-tune fragments the arena out from under
/// the cascade is fixed. It's well above typical String/Vec growth
/// sizes (≤64 KB) so the arena keeps absorbing the small-alloc
/// churn that motivated the arena in the first place.
const LARGE_ALLOC_THRESHOLD: usize = 256 * 1024;

/// Magic word stored in `AllocHeader.tag` for arena-allocated blocks.
/// `dealloc` reads this to decide whether to free into the arena or
/// back to the kernel partition.
const TAG_ARENA: u32 = 0xA1E0_5FF1;
/// Magic word for kernel-block-allocated blocks.
const TAG_KERNEL: u32 = 0xC0DE_BABE;

/// Per-allocation header. Lives immediately before the alignment
/// padding. Stores the routing tag plus the size or kernel block id
/// `dealloc` needs to free the block.
///
/// Layout:
///
/// ```text
/// [AllocHeader: tag + size_or_id][padding 1..=MAX_ALIGN][user data...]
///                                                        ^-- returned ptr
/// ```
///
/// For arena blocks, `size_or_id` is the total bytes allocated from
/// the linked-list heap (header + padding + user data). For kernel
/// blocks it's the `SceUid` cast to `u32` so we can call
/// `sceKernelFreePartitionMemory`.
#[repr(C)]
struct AllocHeader {
    tag: u32,
    size_or_id: u32,
}

const HEADER_SIZE: usize = mem::size_of::<AllocHeader>();
const HEADER_OVERHEAD: usize = HEADER_SIZE + MAX_ALIGN;

/// Heap arena. Lazily initialised on first allocation.
static HEAP: Mutex<Heap> = Mutex::new(Heap::empty());

/// Acquire the heap lock, yielding to the PSP scheduler between retries.
///
/// A plain `HEAP.lock()` spin-loop deadlocks the TV Guide pipeline:
/// the audio thread runs at priority 16 and the I/O thread at 32
/// (lower priority). If the I/O thread holds the heap mutex and the
/// audio thread starts spinning, the scheduler keeps the higher-prio
/// audio thread running, starves the I/O thread, and the system
/// wedges. `sceKernelDelayThread` forces the scheduler to pick a
/// different runnable thread for the duration, letting the holder
/// make progress.
fn lock_heap() -> spin::MutexGuard<'static, Heap> {
    loop {
        if let Some(guard) = HEAP.try_lock() {
            return guard;
        }
        // 100us is short enough to be invisible to audio latency and
        // long enough to let the lower-priority holder complete.
        unsafe { sys::sceKernelDelayThread(100) };
    }
}

/// Reserve the underlying kernel block on first allocation.
/// Idempotent — subsequent calls are a no-op once the heap has been
/// initialised.
fn ensure_heap_init(heap: &mut Heap) -> bool {
    if heap.size() > 0 {
        return true;
    }
    // Use the kernel partition for kernel-mode PRX modules, user
    // partition otherwise.
    #[cfg(feature = "kernel")]
    let partition = SceSysMemPartitionId::SceKernelPrimaryKernelPartition;
    #[cfg(not(feature = "kernel"))]
    let partition = SceSysMemPartitionId::SceKernelPrimaryUserPartition;
    // SAFETY: requesting a private block from the relevant partition.
    // The block is intentionally leaked for the lifetime of the
    // process — we want the heap to live as long as the program.
    let id = unsafe {
        sys::sceKernelAllocPartitionMemory(
            partition,
            &b"rust_heap\0"[0],
            SceSysMemBlockTypes::Low,
            HEAP_SIZE as u32,
            ptr::null_mut(),
        )
    };
    if id.0 < 0 {
        return false;
    }
    // SAFETY: `sceKernelGetBlockHeadAddr` returns the start of the
    // block we just allocated. The region is HEAP_SIZE bytes and
    // exclusively owned by the Rust heap.
    unsafe {
        let arena_start = sys::sceKernelGetBlockHeadAddr(id) as *mut u8;
        heap.init(arena_start, HEAP_SIZE);
    }
    let _ = id; // Block ID retained implicitly via the leaked allocation.
    true
}

/// Free heap memory in bytes (current capacity minus used). Useful
/// for diagnostics — embedders can call this to check arena pressure.
pub fn heap_free() -> usize {
    lock_heap().free()
}

/// Total heap arena size in bytes (the original `HEAP_SIZE`).
pub fn heap_total() -> usize {
    let h = lock_heap();
    if h.size() == 0 {
        HEAP_SIZE
    } else {
        h.size()
    }
}

struct SystemAlloc;

unsafe impl GlobalAlloc for SystemAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let user_size = layout.size();
        let user_align = layout.align();
        if user_align > MAX_ALIGN || !user_align.is_power_of_two() {
            return ptr::null_mut();
        }
        let Some(total) = user_size.checked_add(HEADER_OVERHEAD) else {
            return ptr::null_mut();
        };

        // Large-allocation fallback: bypass the arena and request a
        // dedicated kernel block. Avoids fragmenting the arena with
        // multi-MB allocations like video frame buffers and the MP4
        // streaming download buffer.
        if total >= LARGE_ALLOC_THRESHOLD {
            return alloc_kernel_path(total, user_align);
        }

        let heap_layout = match Layout::from_size_align(total, mem::align_of::<AllocHeader>()) {
            Ok(l) => l,
            Err(_) => return ptr::null_mut(),
        };

        let raw = {
            let mut heap = lock_heap();
            if !ensure_heap_init(&mut heap) {
                return ptr::null_mut();
            }
            match heap.allocate_first_fit(heap_layout) {
                Ok(nn) => nn.as_ptr(),
                Err(_) => {
                    // Arena is full or fragmented enough to fail this
                    // size. Fall back to a kernel block — slower
                    // (one syscall, one block-table entry) but lets
                    // the program keep running.
                    drop(heap);
                    return alloc_kernel_path(total, user_align);
                },
            }
        };

        write_header_and_align(raw, total, user_align, TAG_ARENA, total as u32)
    }

    #[inline(never)]
    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        if ptr.is_null() {
            return;
        }
        let pad_len = *ptr.sub(1) as usize;
        let header_ptr = ptr.sub(pad_len).sub(HEADER_SIZE);
        let header = ptr::read(header_ptr.cast::<AllocHeader>());
        match header.tag {
            TAG_ARENA => {
                let heap_layout = Layout::from_size_align_unchecked(
                    header.size_or_id as usize,
                    mem::align_of::<AllocHeader>(),
                );
                lock_heap()
                    .deallocate(core::ptr::NonNull::new_unchecked(header_ptr), heap_layout);
            },
            TAG_KERNEL => {
                let id = SceUid(header.size_or_id as i32);
                sys::sceKernelFreePartitionMemory(id);
            },
            _ => {
                // Header corruption — leak rather than crash.
            },
        }
    }
}

/// Allocate a kernel block, write the [`AllocHeader`] with
/// `TAG_KERNEL` + the SceUid, align the user pointer, and return it.
/// Used both by the large-allocation fast path and by the arena
/// fallback when the heap can't satisfy a request.
unsafe fn alloc_kernel_path(total: usize, user_align: usize) -> *mut u8 {
    #[cfg(feature = "kernel")]
    let partition = SceSysMemPartitionId::SceKernelPrimaryKernelPartition;
    #[cfg(not(feature = "kernel"))]
    let partition = SceSysMemPartitionId::SceKernelPrimaryUserPartition;
    let id = sys::sceKernelAllocPartitionMemory(
        partition,
        &b"big_block\0"[0],
        SceSysMemBlockTypes::Low,
        total as u32,
        ptr::null_mut(),
    );
    if id.0 < 0 {
        return ptr::null_mut();
    }
    let raw = sys::sceKernelGetBlockHeadAddr(id) as *mut u8;
    write_header_and_align(raw, total, user_align, TAG_KERNEL, id.0 as u32)
}

/// Write the per-allocation header at `raw`, then compute and stamp
/// the alignment padding so the returned pointer satisfies
/// `user_align`. Returns the user pointer or null on pathological
/// alignment.
unsafe fn write_header_and_align(
    raw: *mut u8,
    total: usize,
    user_align: usize,
    tag: u32,
    size_or_id: u32,
) -> *mut u8 {
    let after_header = raw.add(HEADER_SIZE);
    let offset = after_header.add(1).align_offset(user_align);
    if offset == usize::MAX {
        // Free the just-allocated block before bailing so we don't
        // leak a kernel block id.
        if tag == TAG_KERNEL {
            sys::sceKernelFreePartitionMemory(SceUid(size_or_id as i32));
        } else {
            let heap_layout = Layout::from_size_align_unchecked(
                total,
                mem::align_of::<AllocHeader>(),
            );
            lock_heap()
                .deallocate(core::ptr::NonNull::new_unchecked(raw), heap_layout);
        }
        return ptr::null_mut();
    }
    let pad_len = 1 + offset;
    debug_assert!(pad_len <= MAX_ALIGN);
    ptr::write(raw.cast::<AllocHeader>(), AllocHeader { tag, size_or_id });
    let user_ptr = after_header.add(pad_len);
    *user_ptr.sub(1) = pad_len as u8;
    user_ptr
}

#[global_allocator]
static ALLOC: SystemAlloc = SystemAlloc;

#[cfg(not(feature = "std"))]
#[alloc_error_handler]
fn aeh(_: Layout) -> ! {
    dprintln!("out of memory");
    loop {
        core::hint::spin_loop()
    }
}

// NOTE: These C runtime functions MUST use manual byte loops, not
// `core::ptr::write_bytes` / `copy_nonoverlapping` / `copy`. Those
// intrinsics lower to calls to memset/memcpy/memmove respectively,
// creating infinite recursion (which on MIPS manifests as a jump to
// an invalid trampoline address).

#[unsafe(no_mangle)]
#[cfg(not(feature = "stub-only"))]
unsafe extern "C" fn memset(ptr: *mut u8, value: u32, num: usize) -> *mut u8 {
    let mut i = 0;
    while i < num {
        *ptr.add(i) = value as u8;
        i += 1;
    }
    ptr
}

#[unsafe(no_mangle)]
#[cfg(not(feature = "stub-only"))]
unsafe extern "C" fn memcpy(dst: *mut u8, src: *const u8, num: isize) -> *mut u8 {
    let mut i = 0isize;
    while i < num {
        *dst.offset(i) = *src.offset(i);
        i += 1;
    }
    dst
}

#[unsafe(no_mangle)]
#[cfg(not(feature = "stub-only"))]
unsafe extern "C" fn memcmp(ptr1: *mut u8, ptr2: *mut u8, num: usize) -> i32 {
    let mut i = 0;
    while i < num {
        let diff = *ptr1.add(i) as i32 - *ptr2.add(i) as i32;
        if diff != 0 {
            return diff;
        }
        i += 1;
    }
    0
}

#[unsafe(no_mangle)]
#[cfg(not(feature = "stub-only"))]
unsafe extern "C" fn memmove(dst: *mut u8, src: *mut u8, num: isize) -> *mut u8 {
    if (dst as usize) < (src as usize) {
        let mut i = 0isize;
        while i < num {
            *dst.offset(i) = *src.offset(i);
            i += 1;
        }
    } else {
        let mut i = num;
        while i > 0 {
            i -= 1;
            *dst.offset(i) = *src.offset(i);
        }
    }
    dst
}

#[unsafe(no_mangle)]
#[cfg(not(feature = "stub-only"))]
unsafe extern "C" fn strlen(s: *mut u8) -> usize {
    let mut len = 0;
    while *s.add(len) != 0 {
        len += 1;
    }
    len
}
