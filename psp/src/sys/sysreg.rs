//! PSP System Register driver (sceSysreg_driver) — kernel-mode API.
//!
//! The System Register block at `0xBC100000` controls peripheral clocks,
//! bus gates, GPIO port enables, and USB controller configuration.
//!
//! # Register Map (TA-090v2, PSP-3001)
//!
//! | Offset | Register         | Value       | Writable | Notes                    |
//! |--------|------------------|-------------|----------|--------------------------|
//! | +0x40  | Tachyon Version  | 0x82000002  | R        | Model identifier         |
//! | +0x4C  | Bus Control      | 0x00000040  | R/W      | USB init → 0x00010020    |
//! | +0x50  | Periph Clock 1   | 0x0000DC1D  | R/W      | Bit 8=USB, bit 13=OHCI   |
//! | +0x58  | Periph Clock 2   | 0x05AD2601  | R/W      | Bit 9=USB clock          |
//! | +0x74  | USB Control      | 0x00000000  | R/W      | Bit 8 set by USB init    |
//! | +0x78  | OHCI/USB Clock   | 0x03082AFA  | R/W      | Bit 1=OHCI, bit 19=PHY   |
//! | +0x7C  | GPIO Port Enable | 0x070000D9  | R/W      | Per-pin enable           |
//! | +0xB8  | USB Host Bus Gate| 0x00000000  | R/W      | Accepts writes           |
//! | +0xC4  | USB Host Mode    | 0x00000000  | **LOCKED** | Silicon-locked         |
//!
//! # NIDs
//!
//! All 14 USB-related NIDs resolved from decrypted `usb.prx` on PSP-3001
//! 6.61. Resolved addresses verified against kernel memory dump.
//!
//! # Kernel Mode Required
//!
//! All functions require `feature = "kernel"`.

psp_extern! {
    #![name = "sceSysreg_driver"]
    #![flags = 0x4001]
    #![version = (0x00, 0x00)]

    #[psp(0xEC03F6E2)]
    /// Enable the GPIO peripheral clock.
    ///
    /// # Return Value
    ///
    /// 0 on success, < 0 on error.
    pub fn sceSysregGpioClkEnable() -> i32;

    #[psp(0x72C1CA96)]
    /// Enable GPIO I/O access.
    ///
    /// # Return Value
    ///
    /// 0 on success, < 0 on error.
    pub fn sceSysregGpioIoEnable() -> i32;

    #[psp(0x1561BCD2)]
    /// Enable the USB peripheral clock.
    ///
    /// # Return Value
    ///
    /// 0 on success, < 0 on error.
    pub fn sceSysregUsbClkEnable() -> i32;

    #[psp(0x1D233EF9)]
    /// Disable the USB peripheral clock.
    ///
    /// # Return Value
    ///
    /// 0 on success, < 0 on error.
    pub fn sceSysregUsbClkDisable() -> i32;

    #[psp(0x9306F27B)]
    /// Enable USB I/O access.
    ///
    /// # Return Value
    ///
    /// 1 if already enabled, 0 on first enable, < 0 on error.
    pub fn sceSysregUsbIoEnable() -> i32;

    #[psp(0xE2A5D1EE)]
    /// Disable USB I/O access.
    ///
    /// # Return Value
    ///
    /// Previous state value, < 0 on error.
    pub fn sceSysregUsbIoDisable() -> i32;

    #[psp(0x9A6E7BB8)]
    /// Enable the USB bus clock.
    ///
    /// # Return Value
    ///
    /// 0 on success, < 0 on error.
    pub fn sceSysregUsbBusClkEnable() -> i32;

    #[psp(0xD7AD9705)]
    /// Disable the USB bus clock.
    ///
    /// # Return Value
    ///
    /// 0 on success, < 0 on error.
    pub fn sceSysregUsbBusClkDisable() -> i32;

    #[psp(0x84A279A4)]
    /// Assert USB reset.
    ///
    /// # Return Value
    ///
    /// Reset state value, < 0 on error.
    pub fn sceSysregUsbResetEnable() -> i32;

    #[psp(0x6F3B6D7D)]
    /// Deassert USB reset.
    ///
    /// # Return Value
    ///
    /// 0 on success, < 0 on error.
    pub fn sceSysregUsbResetDisable() -> i32;

    #[psp(0x87B61303)]
    /// Get USB connection status from the system register.
    ///
    /// # Return Value
    ///
    /// 1 if USB is connected, 0 if not.
    pub fn sceSysregUsbGetConnectStatus() -> i32;

    #[psp(0x9275DD37)]
    /// Set USB connection status in the system register.
    ///
    /// # Parameters
    ///
    /// - `status`: Connection status to set
    ///
    /// # Return Value
    ///
    /// Previous status value, < 0 on error.
    pub fn sceSysregUsbSetConnectStatus(status: i32) -> i32;

    #[psp(0x30C0A141)]
    /// Query USB interrupt status.
    ///
    /// # Return Value
    ///
    /// Interrupt status flags, 0 if no pending interrupts.
    pub fn sceSysregUsbQueryIntr() -> i32;

    #[psp(0x6C0EE043)]
    /// Acquire (acknowledge) USB interrupt.
    ///
    /// # Return Value
    ///
    /// Interrupt value, < 0 on error.
    pub fn sceSysregUsbAcquireIntr() -> i32;
}
