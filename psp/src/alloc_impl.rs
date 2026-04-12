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

/// Userspace heap arena reserved at first allocation. 14 MB on user
/// builds — sized to fit a `vec![None; ~2500]` of the OASIS browser's
/// 1240-byte `ComputedStyle` struct (~3 MB) on top of a 5 MB Wikipedia
/// HTML token + DOM working set. Leaves ~6 MB of the 24 MB user
/// partition for textures, GU command buffer, video decode buffers,
/// audio, and the EBOOT code itself.
#[cfg(not(feature = "kernel"))]
const HEAP_SIZE: usize = 14 * 1024 * 1024;

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

/// Per-allocation header. Lives immediately before the alignment
/// padding. Stores the total size given to the underlying heap
/// (header + padding + user data) so `dealloc` can recover the
/// original allocation without needing the layout from the caller.
#[repr(C)]
struct AllocHeader {
    total_size: u32,
}

const HEADER_SIZE: usize = mem::size_of::<AllocHeader>();
const HEADER_OVERHEAD: usize = HEADER_SIZE + MAX_ALIGN;

/// Heap arena. Lazily initialised on first allocation.
static HEAP: Mutex<Heap> = Mutex::new(Heap::empty());

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
    HEAP.lock().free()
}

/// Total heap arena size in bytes (the original `HEAP_SIZE`).
pub fn heap_total() -> usize {
    let h = HEAP.lock();
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
        let heap_layout = match Layout::from_size_align(total, mem::align_of::<AllocHeader>()) {
            Ok(l) => l,
            Err(_) => return ptr::null_mut(),
        };

        let raw = {
            let mut heap = HEAP.lock();
            if !ensure_heap_init(&mut heap) {
                return ptr::null_mut();
            }
            match heap.allocate_first_fit(heap_layout) {
                Ok(nn) => nn.as_ptr(),
                Err(_) => return ptr::null_mut(),
            }
        };

        // Layout: [AllocHeader][padding 1..=MAX_ALIGN][user data...]
        // The pad-length byte is stored immediately before the user
        // pointer so dealloc can recover the start of the block.
        let after_header = raw.add(HEADER_SIZE);
        let offset = after_header.add(1).align_offset(user_align);
        if offset == usize::MAX {
            // Pathological alignment for this address — give the
            // memory back to the heap and bail.
            HEAP.lock()
                .deallocate(core::ptr::NonNull::new_unchecked(raw), heap_layout);
            return ptr::null_mut();
        }
        let pad_len = 1 + offset;
        debug_assert!(pad_len <= MAX_ALIGN);

        ptr::write(
            raw.cast::<AllocHeader>(),
            AllocHeader {
                total_size: total as u32,
            },
        );
        let user_ptr = after_header.add(pad_len);
        *user_ptr.sub(1) = pad_len as u8;
        user_ptr
    }

    #[inline(never)]
    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        if ptr.is_null() {
            return;
        }
        let pad_len = *ptr.sub(1) as usize;
        let header_ptr = ptr.sub(pad_len).sub(HEADER_SIZE);
        let header = ptr::read(header_ptr.cast::<AllocHeader>());
        let heap_layout = Layout::from_size_align_unchecked(
            header.total_size as usize,
            mem::align_of::<AllocHeader>(),
        );
        HEAP.lock()
            .deallocate(core::ptr::NonNull::new_unchecked(header_ptr), heap_layout);
    }
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
