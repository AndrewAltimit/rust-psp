//! Hardware register access for kernel-mode PSP applications.
//!
//! The PSP's peripherals are controlled via memory-mapped I/O registers
//! at fixed physical addresses. These functions provide volatile read/write
//! access with proper memory ordering.
//!
//! # Kernel Mode Required
//!
//! All functions in this module require `feature = "kernel"` and the module
//! must be declared with `psp::module_kernel!()`.
//!
//! # Safety
//!
//! Hardware register access is inherently unsafe. Incorrect register writes
//! can hang the system, corrupt firmware state, or damage hardware.

// ── Register Address Constants ──────────────────────────────────────

/// System control register base address.
pub const SYS_CTRL_BASE: u32 = 0xBC10_0000;

/// GPIO register base address.
pub const GPIO_BASE: u32 = 0xBE24_0000;
/// GPIO port read register.
pub const GPIO_PORT_READ: u32 = GPIO_BASE + 0x004;
/// GPIO port set register (write 1 to set bits).
pub const GPIO_PORT_SET: u32 = GPIO_BASE + 0x008;
/// GPIO port clear register (write 1 to clear bits).
pub const GPIO_PORT_CLEAR: u32 = GPIO_BASE + 0x00C;

/// Display engine register base address.
pub const DISPLAY_BASE: u32 = 0xBE14_0000;

/// Audio hardware register base address.
pub const AUDIO_BASE: u32 = 0xBE00_0000;

/// DMA controller register base address.
pub const DMAC_BASE: u32 = 0xBC90_0000;

/// Memory Stick Pro interface register base address.
pub const MSPRO_BASE: u32 = 0xBD20_0000;

// ── Primitive Read/Write Functions ──────────────────────────────────

/// Read a 32-bit hardware register.
///
/// # Safety
///
/// `addr` must be a valid memory-mapped I/O register address.
/// Caller must be in kernel mode.
#[inline(always)]
pub unsafe fn hw_read32(addr: u32) -> u32 {
    let ptr = addr as *const u32;
    unsafe { core::ptr::read_volatile(ptr) }
}

/// Write a 32-bit hardware register.
///
/// # Safety
///
/// `addr` must be a valid memory-mapped I/O register address.
/// Caller must be in kernel mode.
#[inline(always)]
pub unsafe fn hw_write32(addr: u32, value: u32) {
    let ptr = addr as *mut u32;
    unsafe { core::ptr::write_volatile(ptr, value) };
}

/// Read a 16-bit hardware register.
///
/// # Safety
///
/// `addr` must be a valid, 2-byte-aligned memory-mapped I/O register address.
/// Caller must be in kernel mode.
#[inline(always)]
pub unsafe fn hw_read16(addr: u32) -> u16 {
    let ptr = addr as *const u16;
    unsafe { core::ptr::read_volatile(ptr) }
}

/// Write a 16-bit hardware register.
///
/// # Safety
///
/// `addr` must be a valid, 2-byte-aligned memory-mapped I/O register address.
/// Caller must be in kernel mode.
#[inline(always)]
pub unsafe fn hw_write16(addr: u32, value: u16) {
    let ptr = addr as *mut u16;
    unsafe { core::ptr::write_volatile(ptr, value) };
}

/// Read an 8-bit hardware register.
///
/// # Safety
///
/// `addr` must be a valid memory-mapped I/O register address.
/// Caller must be in kernel mode.
#[inline(always)]
pub unsafe fn hw_read8(addr: u32) -> u8 {
    let ptr = addr as *const u8;
    unsafe { core::ptr::read_volatile(ptr) }
}

/// Write an 8-bit hardware register.
///
/// # Safety
///
/// `addr` must be a valid memory-mapped I/O register address.
/// Caller must be in kernel mode.
#[inline(always)]
pub unsafe fn hw_write8(addr: u32, value: u8) {
    let ptr = addr as *mut u8;
    unsafe { core::ptr::write_volatile(ptr, value) };
}

// ── Type-Safe Register Wrapper ──────────────────────────────────────

/// A memory-mapped I/O register at a fixed address.
///
/// Provides type-safe volatile read/write access to hardware registers.
///
/// # Example
///
/// ```ignore
/// use psp::hw::Register;
///
/// const GPIO_READ: Register<u32> = Register::new(0xBE24_0004);
///
/// let value = unsafe { GPIO_READ.read() };
/// ```
pub struct Register<T: Copy> {
    addr: u32,
    _phantom: core::marker::PhantomData<T>,
}

impl<T: Copy> Register<T> {
    /// Create a new register reference at the given address.
    pub const fn new(addr: u32) -> Self {
        Self {
            addr,
            _phantom: core::marker::PhantomData,
        }
    }

    /// Get the raw address of this register.
    pub const fn addr(&self) -> u32 {
        self.addr
    }
}

impl Register<u32> {
    /// Read a 32-bit value from this register.
    ///
    /// # Safety
    ///
    /// The register address must be valid and the caller must be in
    /// kernel mode.
    #[inline(always)]
    pub unsafe fn read(&self) -> u32 {
        unsafe { core::ptr::read_volatile(self.addr as *const u32) }
    }

    /// Write a 32-bit value to this register.
    ///
    /// # Safety
    ///
    /// The register address must be valid and the caller must be in
    /// kernel mode.
    #[inline(always)]
    pub unsafe fn write(&self, value: u32) {
        unsafe { core::ptr::write_volatile(self.addr as *mut u32, value) };
    }
}

impl Register<u16> {
    /// Read a 16-bit value from this register.
    ///
    /// # Safety
    ///
    /// The register address must be valid and 2-byte aligned, and the
    /// caller must be in kernel mode.
    #[inline(always)]
    pub unsafe fn read(&self) -> u16 {
        unsafe { core::ptr::read_volatile(self.addr as *const u16) }
    }

    /// Write a 16-bit value to this register.
    ///
    /// # Safety
    ///
    /// The register address must be valid and 2-byte aligned, and the
    /// caller must be in kernel mode.
    #[inline(always)]
    pub unsafe fn write(&self, value: u16) {
        unsafe { core::ptr::write_volatile(self.addr as *mut u16, value) };
    }
}

impl Register<u8> {
    /// Read an 8-bit value from this register.
    ///
    /// # Safety
    ///
    /// The register address must be valid and the caller must be in
    /// kernel mode.
    #[inline(always)]
    pub unsafe fn read(&self) -> u8 {
        unsafe { core::ptr::read_volatile(self.addr as *const u8) }
    }

    /// Write an 8-bit value to this register.
    ///
    /// # Safety
    ///
    /// The register address must be valid and the caller must be in
    /// kernel mode.
    #[inline(always)]
    pub unsafe fn write(&self, value: u8) {
        unsafe { core::ptr::write_volatile(self.addr as *mut u8, value) };
    }
}
