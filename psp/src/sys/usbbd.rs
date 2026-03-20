//! PSP USB Bus Driver (sceUsbBus_driver) — kernel-mode API.
//!
//! These functions are kernel-only and are used to implement custom USB device
//! drivers. They allow registering a USB device driver with custom endpoints,
//! descriptors, and callbacks, then sending/receiving bulk data.
//!
//! # NIDs
//!
//! NIDs verified against PSPSDK `sceUsbBus_driver.S` stubs and confirmed by
//! kernel memory dump from PSP-3001 6.61 ARK-4 (NID table at 0x88190324).
//!
//! # Usage
//!
//! These functions require the `sceUsbBus_driver` library to be loaded. Use
//! `sceUsbStart(b"USBBusDriver\0", ...)` to load it before calling any of
//! these functions.
//!
//! # Important: Host claim_interface Kills Pending Recv
//!
//! When a USB host calls `claim_interface`, it resets the device's endpoints.
//! Any pending `sceUsbbdReqRecv` will be silently cancelled — the completion
//! callback never fires and the transfer chain dies permanently.
//!
//! To avoid this, do NOT queue a callback-driven recv until the host has
//! finished its USB setup. Use a blocking recv-poll for the initial handshake:
//! queue a recv, poll the request's `retcode`/`recvsize` fields from the main
//! thread, and only switch to callback-driven mode after the first message
//! is successfully received.
//!
//! # NID Swap Warning
//!
//! Some PSP documentation and older SDKs have the NIDs for `sceUsbbdClearFIFO`
//! (0x951A24CC) and `sceUsbbdStall` (0xE65441C1) swapped. The NIDs in this
//! module match PSPSDK and have been verified by testing on real hardware.
//! Calling `sceUsbbdStall` when you meant `sceUsbbdClearFIFO` will stall
//! the endpoint, preventing all subsequent transfers.
//!
//! # References
//!
//! - [PSPSDK pspusbbus.h](https://pspdev.github.io/pspsdk/pspusbbus_8h.html)
//! - [USBHostFS implementation](https://github.com/tyranid/psplinkusb/blob/master/usbhostfs/main.c)

use core::ffi::c_void;

/// USB endpoint descriptor (PSP internal representation).
///
/// Used to identify endpoints for bulk transfers. The `endpnum` field
/// corresponds to the endpoint number (0=control, 1=bulk IN, 2=bulk OUT, etc).
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct UsbEndpoint {
    /// Endpoint number (0, 1, 2, 3)
    pub endpnum: i32,
    /// Unknown field (set to 0)
    pub unk2: i32,
    /// Unknown field (set to 0)
    pub unk3: i32,
}

/// USB device request structure.
///
/// Used with [`sceUsbbdReqSend`] and [`sceUsbbdReqRecv`] to queue async
/// bulk transfers. The completion callback is invoked from USB interrupt
/// context — it must NOT perform file I/O or call blocking syscalls.
///
/// # Cache coherency
///
/// Before calling `sceUsbbdReqRecv`, invalidate the dcache for the data
/// buffer with `sceKernelDcacheInvalidateRange`. Before `sceUsbbdReqSend`,
/// writeback the dcache with `sceKernelDcacheWritebackRange`. Also flush
/// dcache for this struct itself before submitting.
///
/// # Example (recv)
///
/// ```ignore
/// // Zero the request struct
/// core::ptr::write_bytes(&raw mut req, 0, 1);
/// req.endp = &mut endpoints[2]; // EP2 = bulk OUT
/// req.data = buf.as_mut_ptr();
/// req.size = buf.len() as i32;
/// req.func = Some(my_recv_callback);
/// sceKernelDcacheInvalidateRange(buf.as_ptr(), buf.len() as u32);
/// sceKernelDcacheWritebackInvalidateAll(); // flush req struct
/// sceUsbbdReqRecv(&mut req);
/// ```
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct UsbdDeviceReq {
    /// Endpoint to queue the transfer on
    pub endp: *mut UsbEndpoint,
    /// Data buffer pointer (must be 64-byte aligned for DMA)
    pub data: *mut u8,
    /// Buffer size (send=exact data size, recv=max receive size)
    pub size: i32,
    /// Unknown field (set to 0)
    pub unkc: i32,
    /// Completion callback (called from USB interrupt context)
    pub func: Option<unsafe extern "C" fn(*mut UsbdDeviceReq, i32, i32) -> i32>,
    /// Result: actual bytes transferred (set by hardware on completion)
    pub recvsize: i32,
    /// Result: completion status (0=success, -3=cancelled)
    pub retcode: i32,
    /// Unknown field (set to 0)
    pub unk1c: i32,
    /// User argument pointer (passed to callback, set to null if unused)
    pub arg: *mut c_void,
    /// Next request in chain (kernel internal, set to null)
    pub link: *mut UsbdDeviceReq,
}

/// USB interface descriptor (PSP internal representation).
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct UsbInterface {
    /// Interface descriptor expect count
    pub expect: i32,
    /// Unknown field
    pub unk4: i32,
    /// Unknown field
    pub unk8: i32,
}

/// USB device driver registration structure.
///
/// Register this with [`sceUsbbdRegister`] to create a custom USB device
/// driver. The PSP will call the provided callbacks during USB lifecycle
/// events (start, stop, attach, detach, control requests).
///
/// # Callbacks
///
/// - `recvctl`: Called for USB control requests. Return -1 to let the bus
///   driver handle standard requests.
/// - `attach`: Called when the host connects. Receives speed as arg1
///   (2=hi-speed, 1=full-speed). Do NOT queue transfers here — set a flag
///   and let the main loop handle it (shared request structs).
/// - `detach`: Called when the host disconnects.
/// - `start_func`: Called during `sceUsbStart`. Initialize descriptors here.
/// - `stop_func`: Called during `sceUsbStop`.
#[repr(C)]
pub struct UsbDriver {
    /// Driver name (null-terminated byte string)
    pub name: *const u8,
    /// Number of endpoints (including EP0)
    pub endpoints: i32,
    /// Pointer to endpoint array
    pub endp: *mut UsbEndpoint,
    /// Pointer to interface descriptor
    pub intp: *mut UsbInterface,
    /// Device descriptor for hi-speed (USB 2.0)
    pub devp_hi: *mut u8,
    /// Configuration descriptor for hi-speed
    pub confp_hi: *mut u8,
    /// Device descriptor for full-speed (USB 1.1)
    pub devp: *mut u8,
    /// Configuration descriptor for full-speed
    pub confp: *mut u8,
    /// String descriptor
    pub str_desc: *mut u8,
    /// Control request callback
    pub recvctl: Option<unsafe extern "C" fn(i32, i32, *mut UsbdDeviceReq) -> i32>,
    /// Function 28 callback (unknown purpose, return 0)
    pub func28: Option<unsafe extern "C" fn(i32, i32, i32) -> i32>,
    /// Attach callback (host connected)
    pub attach: Option<unsafe extern "C" fn(i32, i32, i32) -> i32>,
    /// Detach callback (host disconnected)
    pub detach: Option<unsafe extern "C" fn(i32, i32, i32) -> i32>,
    /// Unknown field (set to 0)
    pub unk34: i32,
    /// Start function callback (called during sceUsbStart)
    pub start_func: Option<unsafe extern "C" fn(i32, *mut c_void) -> i32>,
    /// Stop function callback (called during sceUsbStop)
    pub stop_func: Option<unsafe extern "C" fn(i32, *mut c_void) -> i32>,
    /// Next driver in kernel linked list (set to null)
    pub link: *mut UsbDriver,
}

// SAFETY: UsbDriver contains raw pointers but is only accessed from a single
// kernel context (USB driver registration). The driver struct is static and
// its pointers reference other statics within the same kernel module.
unsafe impl Sync for UsbDriver {}
unsafe impl Sync for UsbdDeviceReq {}
unsafe impl Sync for UsbEndpoint {}
unsafe impl Sync for UsbInterface {}

psp_extern! {
    #![name = "sceUsbBus_driver"]
    #![flags = 0x4001]
    #![version = (0x00, 0x00)]

    #[psp(0xB1644BE7)]
    /// Register a USB device driver with the bus driver.
    ///
    /// # Parameters
    ///
    /// - `driver`: Pointer to a filled-in [`UsbDriver`] structure
    ///
    /// # Return Value
    ///
    /// 0 on success, < 0 on error
    pub fn sceUsbbdRegister(driver: *mut UsbDriver) -> i32;

    #[psp(0xC1E2A540)]
    /// Unregister a USB device driver.
    ///
    /// # Parameters
    ///
    /// - `driver`: Pointer to the previously registered [`UsbDriver`]
    ///
    /// # Return Value
    ///
    /// 0 on success, < 0 on error
    pub fn sceUsbbdUnregister(driver: *mut UsbDriver) -> i32;

    #[psp(0x23E51D8F)]
    /// Queue an async bulk send (device → host) on a bulk IN endpoint.
    ///
    /// The completion callback in [`UsbdDeviceReq::func`] will be called
    /// from USB interrupt context when the transfer completes.
    ///
    /// # Parameters
    ///
    /// - `req`: Pointer to a filled-in [`UsbdDeviceReq`]. The `endp` field
    ///   must point to a bulk IN endpoint. The `data` buffer must be
    ///   64-byte aligned and dcache must be flushed before calling.
    ///
    /// # Return Value
    ///
    /// 0 on success, < 0 on error
    pub fn sceUsbbdReqSend(req: *mut UsbdDeviceReq) -> i32;

    #[psp(0x913EC15D)]
    /// Queue an async bulk receive (host → device) on a bulk OUT endpoint.
    ///
    /// The completion callback in [`UsbdDeviceReq::func`] will be called
    /// from USB interrupt context when data is received.
    ///
    /// Before calling, invalidate the dcache for the data buffer with
    /// `sceKernelDcacheInvalidateRange` to ensure the CPU reads DMA-written
    /// data correctly.
    ///
    /// # Parameters
    ///
    /// - `req`: Pointer to a filled-in [`UsbdDeviceReq`]. The `endp` field
    ///   must point to a bulk OUT endpoint.
    ///
    /// # Return Value
    ///
    /// 0 on success, < 0 on error
    pub fn sceUsbbdReqRecv(req: *mut UsbdDeviceReq) -> i32;

    #[psp(0xCC57EC9D)]
    /// Cancel a pending USB transfer request.
    ///
    /// # Parameters
    ///
    /// - `req`: Pointer to the request to cancel
    ///
    /// # Return Value
    ///
    /// 0 on success, < 0 on error
    pub fn sceUsbbdReqCancel(req: *mut UsbdDeviceReq) -> i32;

    #[psp(0xC5E53685)]
    /// Cancel all pending transfers on an endpoint.
    ///
    /// # Parameters
    ///
    /// - `endp`: Pointer to the endpoint to cancel transfers on
    ///
    /// # Return Value
    ///
    /// 0 on success, < 0 on error
    pub fn sceUsbbdReqCancelAll(endp: *mut UsbEndpoint) -> i32;

    #[psp(0x951A24CC)]
    /// Clear the FIFO of an endpoint.
    ///
    /// Call this before queuing the first receive to ensure no stale data
    /// from previous sessions remains in the hardware FIFO.
    ///
    /// # Parameters
    ///
    /// - `endp`: Pointer to the endpoint to clear
    ///
    /// # Return Value
    ///
    /// 0 on success, < 0 on error
    pub fn sceUsbbdClearFIFO(endp: *mut UsbEndpoint) -> i32;

    #[psp(0xE65441C1)]
    /// Stall an endpoint (signal an error condition to the host).
    ///
    /// # Parameters
    ///
    /// - `endp`: Pointer to the endpoint to stall
    ///
    /// # Return Value
    ///
    /// 0 on success, < 0 on error
    pub fn sceUsbbdStall(endp: *mut UsbEndpoint) -> i32;
}
