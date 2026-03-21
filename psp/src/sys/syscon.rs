//! PSP Syscon driver (sceSyscon_driver) — kernel-mode API.
//!
//! The Syscon (System Controller) is a secondary microcontroller on the PSP
//! motherboard that manages power, battery, temperature, USB power state,
//! and other low-level hardware functions. The main CPU communicates with
//! Syscon via an SPI bus.
//!
//! # Command Map (Baryon 0x00040600, PSP-3001)
//!
//! ## USB-Related Commands
//!
//! | Cmd  | Type | Response             | Purpose                       |
//! |------|------|----------------------|-------------------------------|
//! | 0x46 | GET  | `0A 06 82 5D B7 02` | USB power state               |
//! | 0x47 | SET  | `0A 03 82 70`       | USB power control (0-4)       |
//! | 0x0C | GET  | `01`                 | USB status                    |
//! | 0x0E | GET  | `41 08`              | USB status (changes w/ OHCI)  |
//!
//! ## Dangerous Commands
//!
//! | Cmd  | Effect                                    |
//! |------|-------------------------------------------|
//! | 0x34 | Hard crash                                |
//! | 0x45 | Shutdown/reboot (screen black, MS LED on) |
//!
//! # NID Resolution
//!
//! NIDs resolved from kernel memory dump on PSP-3001 6.61 ARK-4. Note that
//! `sceSysconCtrlUsbPower` (NID 0xC8D97773) resolves to a getter stub region
//! on this firmware — the real function is at a different address not
//! reachable via standard NID resolution.
//!
//! # Kernel Mode Required
//!
//! All functions require `feature = "kernel"`.

psp_extern! {
    #![name = "sceSyscon_driver"]
    #![flags = 0x4001]
    #![version = (0x00, 0x00)]

    #[psp(0xE7E87741)]
    /// Read the Baryon (Syscon) hardware version.
    ///
    /// # Return Value
    ///
    /// Baryon version as a 32-bit value (e.g., `0x00040600` for PSP-3001).
    pub fn sceSysconGetBaryonVersion() -> i32;

    #[psp(0x8CBC8B50)]
    /// Read the power supply status.
    ///
    /// # Parameters
    ///
    /// - `status`: Output pointer for power status value
    ///
    /// # Return Value
    ///
    /// 0 on success, < 0 on error.
    pub fn sceSysconGetPowerStatus(status: *mut i32) -> i32;

    #[psp(0x3B657A27)]
    /// Read the battery remaining capacity (as a percentage 0-100).
    ///
    /// # Parameters
    ///
    /// - `percent`: Output pointer for battery percentage
    ///
    /// # Return Value
    ///
    /// 0 on success, < 0 on error.
    pub fn sceSysconGetBatteryRemain(percent: *mut i32) -> i32;

    #[psp(0x71135D7D)]
    /// Read the battery voltage in millivolts.
    ///
    /// # Parameters
    ///
    /// - `voltage`: Output pointer for voltage (mV)
    ///
    /// # Return Value
    ///
    /// 0 on success, < 0 on error.
    pub fn sceSysconGetBatteryVolt(voltage: *mut i32) -> i32;

    #[psp(0x4C539345)]
    /// Read the battery temperature in degrees Celsius.
    ///
    /// # Parameters
    ///
    /// - `temp`: Output pointer for temperature
    ///
    /// # Return Value
    ///
    /// 0 on success, < 0 on error.
    pub fn sceSysconGetBatteryTemp(temp: *mut i32) -> i32;

    #[psp(0xE0DDFE18)]
    /// Check if the AC adapter is connected.
    ///
    /// # Return Value
    ///
    /// 1 if AC adapter is connected, 0 if running on battery.
    pub fn sceSysconIsAcSupplied() -> i32;

    #[psp(0xC8D97773)]
    /// Control USB power state.
    ///
    /// # Warning
    ///
    /// On PSP-3001 6.61, this NID resolves to a **getter stub region**, not
    /// the real function. The getter returns a cached value but does not
    /// actually control USB power. Use direct Syscon SPI commands (0x47)
    /// via [`sceSysconCommonWrite`] instead if you need actual control.
    ///
    /// # Parameters
    ///
    /// - `enable`: 1 to enable USB power, 0 to disable
    ///
    /// # Return Value
    ///
    /// 0 on success, < 0 on error.
    pub fn sceSysconCtrlUsbPower(enable: i32) -> i32;

    #[psp(0x7EC5A957)]
    /// Send a raw Syscon SPI command (write/SET).
    ///
    /// This provides direct access to the Syscon command interface. Commands
    /// are sent as SPI transactions with a command byte and optional data.
    ///
    /// # Parameters
    ///
    /// - `cmd`: Syscon command byte (e.g., 0x47 for USB power SET)
    /// - `data`: Pointer to command data buffer
    /// - `len`: Length of data buffer in bytes
    ///
    /// # Return Value
    ///
    /// 0 on success, < 0 on error.
    ///
    /// # Dangerous Commands
    ///
    /// - Command 0x34: **Hard crash** — do not send
    /// - Command 0x45: **Shutdown/reboot** — causes immediate power off
    pub fn sceSysconCommonWrite(cmd: i32, data: *const u8, len: i32) -> i32;

    #[psp(0x3AC3D2A4)]
    /// Read a raw Syscon SPI response (read/GET).
    ///
    /// # Parameters
    ///
    /// - `cmd`: Syscon command byte (e.g., 0x46 for USB power GET)
    /// - `data`: Output buffer for response data
    /// - `len`: Maximum length of output buffer
    ///
    /// # Return Value
    ///
    /// Number of bytes read on success, < 0 on error.
    pub fn sceSysconCommonRead(cmd: i32, data: *mut u8, len: i32) -> i32;
}
