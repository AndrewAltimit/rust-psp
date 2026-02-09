//! HTTP client for the PSP.
//!
//! Wraps `sceHttp*` syscalls into a safe, RAII-managed HTTP client with
//! template/connection/request lifecycle management.
//!
//! # Example
//!
//! ```ignore
//! use psp::http::HttpClient;
//!
//! let client = HttpClient::new().unwrap();
//! let response = client.get(b"http://example.com/\0").unwrap();
//! psp::dprintln!("Status: {}", response.status_code);
//! psp::dprintln!("Body: {} bytes", response.body.len());
//! ```

use alloc::vec::Vec;
use core::ffi::c_void;

use crate::sys;

/// Error from an HTTP operation, wrapping the raw SCE error code.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct HttpError(pub i32);

impl core::fmt::Debug for HttpError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "HttpError({:#010x})", self.0 as u32)
    }
}

impl core::fmt::Display for HttpError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "http error {:#010x}", self.0 as u32)
    }
}

/// An HTTP client with RAII resource management.
///
/// Manages the sceHttp subsystem initialization and template lifecycle.
/// All connections and requests created through this client are cleaned
/// up on drop.
pub struct HttpClient {
    template_id: i32,
}

impl HttpClient {
    /// Initialize the HTTP subsystem and create a client.
    ///
    /// Calls `sceHttpInit` and creates a default template.
    pub fn new() -> Result<Self, HttpError> {
        let ret = unsafe { sys::sceHttpInit(0x20000) };
        if ret < 0 {
            return Err(HttpError(ret));
        }

        let template_id = unsafe {
            sys::sceHttpCreateTemplate(
                b"rust-psp/1.0\0".as_ptr() as *mut u8,
                1, // HTTP/1.1
                0,
            )
        };
        if template_id < 0 {
            unsafe { sys::sceHttpEnd() };
            return Err(HttpError(template_id));
        }

        // Enable redirects by default.
        unsafe { sys::sceHttpEnableRedirect(template_id) };

        Ok(Self { template_id })
    }

    /// Perform an HTTP GET request.
    ///
    /// `url` must be a null-terminated byte string.
    pub fn get(&self, url: &[u8]) -> Result<Response, HttpError> {
        RequestBuilder::new(self, sys::HttpMethod::Get, url).send()
    }

    /// Perform an HTTP POST request.
    ///
    /// `url` must be a null-terminated byte string.
    pub fn post(&self, url: &[u8], body: &[u8]) -> Result<Response, HttpError> {
        RequestBuilder::new(self, sys::HttpMethod::Post, url)
            .body(body)
            .send()
    }

    /// Create a request builder for more control.
    pub fn request<'a>(&'a self, method: sys::HttpMethod, url: &'a [u8]) -> RequestBuilder<'a> {
        RequestBuilder::new(self, method, url)
    }

    /// Get the template ID for advanced use.
    pub fn template_id(&self) -> i32 {
        self.template_id
    }
}

impl Drop for HttpClient {
    fn drop(&mut self) {
        unsafe {
            sys::sceHttpDeleteTemplate(self.template_id);
            sys::sceHttpEnd();
        }
    }
}

/// An HTTP response.
pub struct Response {
    /// HTTP status code (e.g., 200, 404).
    pub status_code: u16,
    /// Content length if provided by the server, or `None`.
    pub content_length: Option<u64>,
    /// Response body.
    pub body: Vec<u8>,
}

/// Builder for HTTP requests.
pub struct RequestBuilder<'a> {
    client: &'a HttpClient,
    method: sys::HttpMethod,
    url: &'a [u8],
    body: Option<&'a [u8]>,
    timeout_ms: Option<u32>,
}

impl<'a> RequestBuilder<'a> {
    fn new(client: &'a HttpClient, method: sys::HttpMethod, url: &'a [u8]) -> Self {
        Self {
            client,
            method,
            url,
            body: None,
            timeout_ms: None,
        }
    }

    /// Set the request body (for POST/PUT).
    pub fn body(mut self, body: &'a [u8]) -> Self {
        self.body = Some(body);
        self
    }

    /// Set the request timeout in milliseconds.
    pub fn timeout(mut self, ms: u32) -> Self {
        self.timeout_ms = Some(ms);
        self
    }

    /// Send the request and return the response.
    pub fn send(self) -> Result<Response, HttpError> {
        // Validate null termination — the SCE HTTP syscalls expect C strings.
        if self.url.last() != Some(&0) {
            return Err(HttpError(-1));
        }

        let content_length = self.body.map(|b| b.len() as u64).unwrap_or(0);

        // Create connection + request using URL-based APIs.
        let conn_id = unsafe {
            sys::sceHttpCreateConnectionWithURL(self.client.template_id, self.url.as_ptr(), 0)
        };
        if conn_id < 0 {
            return Err(HttpError(conn_id));
        }

        let req_id = unsafe {
            sys::sceHttpCreateRequestWithURL(
                conn_id,
                self.method,
                self.url.as_ptr() as *mut u8,
                content_length,
            )
        };
        if req_id < 0 {
            unsafe { sys::sceHttpDeleteConnection(conn_id) };
            return Err(HttpError(req_id));
        }

        // Apply timeout if set.
        if let Some(ms) = self.timeout_ms {
            unsafe {
                sys::sceHttpSetConnectTimeOut(req_id, ms * 1000);
                sys::sceHttpSetRecvTimeOut(req_id, ms * 1000);
                sys::sceHttpSetSendTimeOut(req_id, ms * 1000);
            }
        }

        // Send the request.
        let (data_ptr, data_size) = match self.body {
            Some(b) => (b.as_ptr() as *mut c_void, b.len() as u32),
            None => (core::ptr::null_mut(), 0),
        };
        let ret = unsafe { sys::sceHttpSendRequest(req_id, data_ptr, data_size) };
        if ret < 0 {
            unsafe {
                sys::sceHttpDeleteRequest(req_id);
                sys::sceHttpDeleteConnection(conn_id);
            }
            return Err(HttpError(ret));
        }

        // Get status code.
        let mut status_code: i32 = 0;
        let ret = unsafe { sys::sceHttpGetStatusCode(req_id, &mut status_code) };
        if ret < 0 {
            unsafe {
                sys::sceHttpDeleteRequest(req_id);
                sys::sceHttpDeleteConnection(conn_id);
            }
            return Err(HttpError(ret));
        }

        // Get content length.
        let mut cl: u64 = 0;
        let cl_ret = unsafe { sys::sceHttpGetContentLength(req_id, &mut cl) };
        let content_length = if cl_ret >= 0 { Some(cl) } else { None };

        // Read body.
        let mut body = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let n = unsafe {
                sys::sceHttpReadData(req_id, buf.as_mut_ptr() as *mut c_void, buf.len() as u32)
            };
            if n < 0 {
                unsafe {
                    sys::sceHttpDeleteRequest(req_id);
                    sys::sceHttpDeleteConnection(conn_id);
                }
                return Err(HttpError(n));
            }
            if n == 0 {
                break;
            }
            body.extend_from_slice(&buf[..n as usize]);
        }

        // Cleanup.
        unsafe {
            sys::sceHttpDeleteRequest(req_id);
            sys::sceHttpDeleteConnection(conn_id);
        }

        Ok(Response {
            status_code: status_code as u16,
            content_length,
            body,
        })
    }
}
