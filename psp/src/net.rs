//! Network sockets and WiFi access for the PSP.
//!
//! Provides RAII wrappers around the PSP's networking stack: access
//! point connection, DNS resolution, and TCP/UDP sockets.
//!
//! # Initialization
//!
//! Before using any networking, call [`init`] to set up the network
//! subsystem. Call [`term`] when done. Connect to a WiFi access point
//! with [`connect_ap`].
//!
//! # Example
//!
//! ```ignore
//! use psp::net;
//!
//! net::init(0x20000).unwrap();
//! net::connect_ap(1).unwrap();
//!
//! let ip = net::get_ip_address().unwrap();
//! psp::dprintln!("IP: {}", core::str::from_utf8(&ip).unwrap_or("?"));
//!
//! let mut stream = net::TcpStream::connect(net::Ipv4Addr([93, 184, 216, 34]), 80).unwrap();
//! stream.write(b"GET / HTTP/1.0\r\nHost: example.com\r\n\r\n").unwrap();
//!
//! let mut buf = [0u8; 1024];
//! let n = stream.read(&mut buf).unwrap();
//! ```

use core::ffi::c_void;
use core::marker::PhantomData;

use crate::sys;

/// Error from a network operation, wrapping the raw SCE error code.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct NetError(pub i32);

impl core::fmt::Debug for NetError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "NetError({:#010x})", self.0 as u32)
    }
}

impl core::fmt::Display for NetError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "net error {:#010x}", self.0 as u32)
    }
}

/// An IPv4 address in network byte order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ipv4Addr(pub [u8; 4]);

impl Ipv4Addr {
    /// Convert to a `u32` in network byte order (big-endian).
    pub fn to_u32_be(self) -> u32 {
        u32::from_be_bytes(self.0)
    }
}

/// Initialize the PSP network subsystem.
///
/// `pool_size` is the memory pool size for the networking stack.
/// A typical value is `0x20000` (128 KiB).
pub fn init(pool_size: u32) -> Result<(), NetError> {
    let ret = unsafe { sys::sceNetInit(pool_size as i32, 0x20, 0x1000, 0x20, 0x1000) };
    if ret < 0 {
        return Err(NetError(ret));
    }

    let ret = unsafe { sys::sceNetInetInit() };
    if ret < 0 {
        unsafe { sys::sceNetTerm() };
        return Err(NetError(ret));
    }

    let ret = unsafe { sys::sceNetResolverInit() };
    if ret < 0 {
        unsafe {
            sys::sceNetInetTerm();
            sys::sceNetTerm();
        }
        return Err(NetError(ret));
    }

    let ret = unsafe { sys::sceNetApctlInit(0x1600, 42) };
    if ret < 0 {
        unsafe {
            sys::sceNetResolverTerm();
            sys::sceNetInetTerm();
            sys::sceNetTerm();
        }
        return Err(NetError(ret));
    }

    Ok(())
}

/// Terminate the network subsystem.
///
/// Call when networking is no longer needed.
pub fn term() {
    unsafe {
        sys::sceNetApctlTerm();
        sys::sceNetResolverTerm();
        sys::sceNetInetTerm();
        sys::sceNetTerm();
    }
}

/// Connect to a WiFi access point using a stored PSP network config slot.
///
/// `config_index` is 1-based (matches the PSP's Network Settings list).
/// Blocks until the connection is established or fails.
/// Uses a default timeout of 30 seconds.
pub fn connect_ap(config_index: i32) -> Result<(), NetError> {
    connect_ap_timeout(config_index, 30_000)
}

/// Connect to a WiFi access point with a custom timeout.
///
/// `config_index` is 1-based (matches the PSP's Network Settings list).
/// `timeout_ms` is the maximum time to wait in milliseconds.
pub fn connect_ap_timeout(config_index: i32, timeout_ms: u32) -> Result<(), NetError> {
    let ret = unsafe { sys::sceNetApctlConnect(config_index) };
    if ret < 0 {
        return Err(NetError(ret));
    }

    // Poll until we get an IP, hit an error, or time out.
    let max_iterations = timeout_ms / 50;
    for _ in 0..max_iterations {
        let mut state = sys::ApctlState::Disconnected;
        let ret = unsafe { sys::sceNetApctlGetState(&mut state) };
        if ret < 0 {
            return Err(NetError(ret));
        }
        match state {
            sys::ApctlState::GotIp => return Ok(()),
            sys::ApctlState::Disconnected => return Err(NetError(-1)),
            _ => {},
        }
        crate::thread::sleep_ms(50);
    }

    // Timed out — disconnect and return error.
    let _ = unsafe { sys::sceNetApctlDisconnect() };
    Err(NetError(-1))
}

/// Disconnect from the current access point.
pub fn disconnect_ap() -> Result<(), NetError> {
    let ret = unsafe { sys::sceNetApctlDisconnect() };
    if ret < 0 { Err(NetError(ret)) } else { Ok(()) }
}

/// Get the IP address assigned to the WLAN interface.
///
/// Returns a null-terminated string in a 16-byte buffer (e.g. `"192.168.1.42\0"`).
pub fn get_ip_address() -> Result<[u8; 16], NetError> {
    let mut info: sys::SceNetApctlInfo = unsafe { core::mem::zeroed() };
    let ret = unsafe { sys::sceNetApctlGetInfo(sys::ApctlInfo::Ip, &mut info) };
    if ret < 0 {
        return Err(NetError(ret));
    }
    // IP is stored as a string in the `name` field of the union
    let mut out = [0u8; 16];
    let src = unsafe { &info.name[..16] };
    out.copy_from_slice(src);
    Ok(out)
}

/// Resolve a hostname to an IPv4 address.
///
/// `hostname` must be a null-terminated byte string.
pub fn resolve_hostname(hostname: &[u8]) -> Result<Ipv4Addr, NetError> {
    let mut rid: i32 = 0;
    let mut buf = [0u8; 1024];

    let ret = unsafe {
        sys::sceNetResolverCreate(&mut rid, buf.as_mut_ptr() as *mut c_void, buf.len() as u32)
    };
    if ret < 0 {
        return Err(NetError(ret));
    }

    let mut addr = sys::in_addr(0);
    let ret = unsafe { sys::sceNetResolverStartNtoA(rid, hostname.as_ptr(), &mut addr, 5, 3) };
    unsafe { sys::sceNetResolverDelete(rid) };

    if ret < 0 {
        return Err(NetError(ret));
    }

    Ok(Ipv4Addr(addr.0.to_be_bytes()))
}

fn make_sockaddr_in(addr: Ipv4Addr, port: u16) -> sys::sockaddr {
    let mut sa = sys::sockaddr {
        sa_len: 16,
        sa_family: 2, // AF_INET
        sa_data: [0u8; 14],
    };
    // sockaddr_in layout: family(2) + port(2, big-endian) + addr(4, big-endian) + pad(8)
    let port_be = port.to_be_bytes();
    sa.sa_data[0] = port_be[0];
    sa.sa_data[1] = port_be[1];
    sa.sa_data[2] = addr.0[0];
    sa.sa_data[3] = addr.0[1];
    sa.sa_data[4] = addr.0[2];
    sa.sa_data[5] = addr.0[3];
    sa
}

// ── TcpStream ──────────────────────────────────────────────────────

/// A TCP stream with RAII socket management.
pub struct TcpStream {
    fd: i32,
    _marker: PhantomData<*const ()>, // !Send + !Sync
}

impl TcpStream {
    /// Connect to a remote TCP endpoint.
    pub fn connect(addr: Ipv4Addr, port: u16) -> Result<Self, NetError> {
        // AF_INET=2, SOCK_STREAM=1, protocol=0
        let fd = unsafe { sys::sceNetInetSocket(2, 1, 0) };
        if fd < 0 {
            return Err(NetError(unsafe { sys::sceNetInetGetErrno() }));
        }

        let sa = make_sockaddr_in(addr, port);
        let ret = unsafe {
            sys::sceNetInetConnect(fd, &sa, core::mem::size_of::<sys::sockaddr>() as u32)
        };
        if ret < 0 {
            let errno = unsafe { sys::sceNetInetGetErrno() };
            unsafe { sys::sceNetInetClose(fd) };
            return Err(NetError(errno));
        }

        Ok(Self {
            fd,
            _marker: PhantomData,
        })
    }

    /// Read data from the stream.
    ///
    /// Returns the number of bytes read. Returns 0 at EOF.
    pub fn read(&self, buf: &mut [u8]) -> Result<usize, NetError> {
        let ret =
            unsafe { sys::sceNetInetRecv(self.fd, buf.as_mut_ptr() as *mut c_void, buf.len(), 0) };
        if ret < 0 {
            Err(NetError(unsafe { sys::sceNetInetGetErrno() }))
        } else {
            Ok(ret as usize)
        }
    }

    /// Write data to the stream.
    ///
    /// Returns the number of bytes written.
    pub fn write(&self, buf: &[u8]) -> Result<usize, NetError> {
        let ret =
            unsafe { sys::sceNetInetSend(self.fd, buf.as_ptr() as *const c_void, buf.len(), 0) };
        if ret < 0 {
            Err(NetError(unsafe { sys::sceNetInetGetErrno() }))
        } else {
            Ok(ret as usize)
        }
    }
}

impl Drop for TcpStream {
    fn drop(&mut self) {
        unsafe {
            sys::sceNetInetClose(self.fd);
        }
    }
}

// ── UdpSocket ──────────────────────────────────────────────────────

/// A UDP socket with RAII management.
pub struct UdpSocket {
    fd: i32,
    _marker: PhantomData<*const ()>, // !Send + !Sync
}

impl UdpSocket {
    /// Create a UDP socket bound to the given port.
    ///
    /// Pass `0` to let the OS choose an ephemeral port.
    pub fn bind(port: u16) -> Result<Self, NetError> {
        // AF_INET=2, SOCK_DGRAM=2, protocol=0
        let fd = unsafe { sys::sceNetInetSocket(2, 2, 0) };
        if fd < 0 {
            return Err(NetError(unsafe { sys::sceNetInetGetErrno() }));
        }

        let sa = make_sockaddr_in(Ipv4Addr([0, 0, 0, 0]), port);
        let ret =
            unsafe { sys::sceNetInetBind(fd, &sa, core::mem::size_of::<sys::sockaddr>() as u32) };
        if ret < 0 {
            let errno = unsafe { sys::sceNetInetGetErrno() };
            unsafe { sys::sceNetInetClose(fd) };
            return Err(NetError(errno));
        }

        Ok(Self {
            fd,
            _marker: PhantomData,
        })
    }

    /// Send data to a remote UDP endpoint.
    pub fn send_to(&self, buf: &[u8], addr: Ipv4Addr, port: u16) -> Result<usize, NetError> {
        let sa = make_sockaddr_in(addr, port);
        let ret = unsafe {
            sys::sceNetInetSendto(
                self.fd,
                buf.as_ptr() as *const c_void,
                buf.len(),
                0,
                &sa,
                core::mem::size_of::<sys::sockaddr>() as u32,
            )
        };
        if ret < 0 {
            Err(NetError(unsafe { sys::sceNetInetGetErrno() }))
        } else {
            Ok(ret as usize)
        }
    }

    /// Receive data from any remote endpoint.
    ///
    /// Returns `(bytes_read, sender_addr, sender_port)`.
    pub fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, Ipv4Addr, u16), NetError> {
        let mut sa = sys::sockaddr {
            sa_len: 16,
            sa_family: 2,
            sa_data: [0u8; 14],
        };
        let mut sa_len = core::mem::size_of::<sys::sockaddr>() as u32;

        let ret = unsafe {
            sys::sceNetInetRecvfrom(
                self.fd,
                buf.as_mut_ptr() as *mut c_void,
                buf.len(),
                0,
                &mut sa,
                &mut sa_len,
            )
        };
        if ret < 0 {
            return Err(NetError(unsafe { sys::sceNetInetGetErrno() }));
        }

        let port = u16::from_be_bytes([sa.sa_data[0], sa.sa_data[1]]);
        let addr = Ipv4Addr([sa.sa_data[2], sa.sa_data[3], sa.sa_data[4], sa.sa_data[5]]);
        Ok((ret as usize, addr, port))
    }
}

impl Drop for UdpSocket {
    fn drop(&mut self) {
        unsafe {
            sys::sceNetInetClose(self.fd);
        }
    }
}
