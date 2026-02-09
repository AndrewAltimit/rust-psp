//! WiFi hardware status for the PSP.
//!
//! Provides a simple API to query WLAN chip state and MAC address.
//! This module does **not** provide networking — see [`crate::net`] for
//! TCP/UDP sockets and access point connections.

/// WLAN hardware status.
pub struct WlanStatus {
    /// Whether the WLAN chip is powered on.
    pub power_on: bool,
    /// Whether the physical WLAN switch is in the ON position.
    pub switch_on: bool,
    /// The 6-byte Ethernet (MAC) address of the WLAN interface.
    pub mac_address: [u8; 6],
}

/// Query the current WLAN hardware status.
///
/// Returns power state, switch state, and MAC address in one call.
pub fn status() -> WlanStatus {
    let power_on = unsafe { crate::sys::sceWlanDevIsPowerOn() } == 1;
    let switch_on = unsafe { crate::sys::sceWlanGetSwitchState() } == 1;
    let mut buf = [0u8; 8];
    unsafe { crate::sys::sceWlanGetEtherAddr(buf.as_mut_ptr()) };
    let mut mac_address = [0u8; 6];
    mac_address.copy_from_slice(&buf[..6]);
    WlanStatus {
        power_on,
        switch_on,
        mac_address,
    }
}

/// Check if WLAN is available (powered on and switch enabled).
pub fn is_available() -> bool {
    let s = status();
    s.power_on && s.switch_on
}
