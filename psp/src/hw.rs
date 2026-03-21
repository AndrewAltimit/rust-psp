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

/// USB PHY serial controller base address.
///
/// Controls clock, mode, and features of the USB physical layer.
/// PHY mode register at +0x30 reads `0x301` (host mode) at boot.
pub const USB_PHY_BASE: u32 = 0xBE4C_0000;

/// OHCI host controller base address.
///
/// Gated by `BC100050` bit 13. Accessible after clock enable, but `usb.prx`
/// does NOT use this controller — it uses MUSB at `0xBD80xxxx` instead.
pub const OHCI_BASE: u32 = 0xBD10_0000;

/// MUSB (Mentor USB OTG) controller base address.
///
/// The actual USB controller used by `usb.prx` (143 references). Bus-faults
/// on TA-090v2 without the correct bus gate enabled. The bus gate register
/// at `BC1000C4` is silicon-locked on this hardware revision.
pub const MUSB_BASE: u32 = 0xBD80_0000;

// ── GPIO Register Offsets ─────────────────────────────────────────

/// GPIO port 0 read register (pin state readback).
pub const GPIO_PORT0_READ: u32 = GPIO_BASE;
/// GPIO port 1 read register.
pub const GPIO_PORT1_READ: u32 = GPIO_BASE + 0x004;
/// GPIO port 1 set register.
pub const GPIO_PORT1_SET: u32 = GPIO_BASE + 0x008;
/// GPIO port 1 clear register.
pub const GPIO_PORT1_CLEAR: u32 = GPIO_BASE + 0x00C;
/// GPIO port 0 direction register (0=input, 1=output).
pub const GPIO_PORT0_DIR: u32 = GPIO_BASE + 0x010;
/// GPIO port 0 set register (write 1 to set output bits).
pub const GPIO_PORT0_SET: u32 = GPIO_BASE + 0x014;
/// GPIO port 0 clear register (write 1 to clear output bits).
pub const GPIO_PORT0_CLEAR: u32 = GPIO_BASE + 0x018;
/// GPIO port 1 direction register.
pub const GPIO_PORT1_DIR: u32 = GPIO_BASE + 0x01C;
/// GPIO interrupt status register.
pub const GPIO_INT_STATUS: u32 = GPIO_BASE + 0x020;
/// GPIO output enable register. **Silicon-locked on TA-090v2.**
pub const GPIO_OUTPUT_EN: u32 = GPIO_BASE + 0x024;
/// GPIO port 0 alternate function register. **Silicon-locked on TA-090v2.**
pub const GPIO_PORT0_ALTFUNC: u32 = GPIO_BASE + 0x040;
/// GPIO port 1 alternate function register (busy flag in bits 0-1).
pub const GPIO_PORT1_ALTFUNC: u32 = GPIO_BASE + 0x048;

// ── System Register Offsets ───────────────────────────────────────

/// Tachyon version register (model identifier, read-only).
pub const SYSREG_TACHYON_VER: u32 = SYS_CTRL_BASE + 0x040;
/// Bus control register.
pub const SYSREG_BUS_CTRL: u32 = SYS_CTRL_BASE + 0x04C;
/// Peripheral clock 1 (bit 8=USB, bit 13=OHCI gate).
pub const SYSREG_PERIPH_CLK1: u32 = SYS_CTRL_BASE + 0x050;
/// Peripheral clock 2 (bit 9=USB clock).
pub const SYSREG_PERIPH_CLK2: u32 = SYS_CTRL_BASE + 0x058;
/// USB control register (bit 8 set during USB init).
pub const SYSREG_USB_CTRL: u32 = SYS_CTRL_BASE + 0x074;
/// OHCI/USB clock register (bit 1=OHCI, bit 19=USB PHY).
pub const SYSREG_USB_CLK: u32 = SYS_CTRL_BASE + 0x078;
/// GPIO port enable register (per-pin enable, writable).
pub const SYSREG_GPIO_PORT_EN: u32 = SYS_CTRL_BASE + 0x07C;
/// USB host interrupt status.
pub const SYSREG_USB_HOST_INTR: u32 = SYS_CTRL_BASE + 0x0B0;
/// USB host bus gate (writable).
pub const SYSREG_USB_HOST_GATE: u32 = SYS_CTRL_BASE + 0x0B8;
/// USB host mode register. **Silicon-locked on TA-090v2.**
pub const SYSREG_USB_HOST_MODE: u32 = SYS_CTRL_BASE + 0x0C4;

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
