//! One HTTP GET through `WinHTTP`.
//!
//! The license service and the firmware catalog are GETs. Certificate checks stay
//! on. The URL is not part of the error.

#![allow(unsafe_code)]

use std::ptr;

use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::Networking::WinHttp::{
    WinHttpAddRequestHeaders, WinHttpCloseHandle, WinHttpConnect, WinHttpOpen,
    WinHttpOpenRequest, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse,
    WinHttpSendRequest, WinHttpSetTimeouts, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
    WINHTTP_ADDREQ_FLAG_ADD, WINHTTP_FLAG_SECURE, WINHTTP_QUERY_FLAG_NUMBER,
    WINHTTP_QUERY_STATUS_CODE,
};

const TIMEOUT_MS: i32 = 30_000;
const HTTP_PORT: u16 = 80;
const HTTPS_PORT: u16 = 443;

/// Why a GET did not return a body.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HttpError {
    /// The URL is not `http` or `https`, or it has no host.
    Url,
    /// `WinHTTP` failed. `os` is `GetLastError`.
    Transport {
        /// The Win32 error from the call that failed.
        os: i32,
    },
    /// The status line was not 200. The body is not returned.
    Status(u32),
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Url => f.write_str("URL is not an http or https request"),
            Self::Transport { os } => write!(f, "WinHTTP failed ({os})"),
            Self::Status(status) => write!(f, "HTTP status {status}"),
        }
    }
}

impl std::error::Error for HttpError {}

struct Target {
    https: bool,
    host: String,
    port: u16,
    path: String,
}

struct Internet(*mut core::ffi::c_void);

impl Drop for Internet {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: `self.0` is a WinHTTP handle this value owns.
            unsafe {
                WinHttpCloseHandle(self.0);
            }
        }
    }
}

/// GET `url`. `header` is one extra request header, including the trailing CRLF.
///
/// # Errors
///
/// Returns [`HttpError::Url`] when `url` is not an `http` or `https` URL with a
/// host, [`HttpError::Transport`] when `WinHTTP` fails, and [`HttpError::Status`]
/// when the status is not 200.
pub fn http_get(url: &str, header: Option<&str>) -> Result<Vec<u8>, HttpError> {
    let target = split_url(url).ok_or(HttpError::Url)?;
    let session = open_session()?;
    let connection = connect(&session, &target)?;
    let request = open_request(&connection, &target)?;
    if let Some(header) = header {
        add_header(&request, header)?;
    }
    send_and_receive(&request)?;
    let status = status_code(&request)?;
    if status != 200 {
        return Err(HttpError::Status(status));
    }
    read_body(&request)
}

fn split_url(url: &str) -> Option<Target> {
    let (https, rest) = if let Some(rest) = url.strip_prefix("https://") {
        (true, rest)
    } else {
        (false, url.strip_prefix("http://")?)
    };
    if rest.is_empty() || rest.contains('@') {
        return None;
    }
    let (hostport, path) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, "/"),
    };
    if hostport.is_empty() {
        return None;
    }
    let (host, port) = if let Some((host, port)) = hostport.rsplit_once(':') {
        (host, port.parse::<u16>().ok()?)
    } else {
        (hostport, if https { HTTPS_PORT } else { HTTP_PORT })
    };
    if host.is_empty() || host.starts_with('[') {
        return None;
    }
    Some(Target {
        https,
        host: host.to_owned(),
        port,
        path: path.to_owned(),
    })
}

fn wide(text: &str) -> Vec<u16> {
    let mut units: Vec<u16> = text.encode_utf16().collect();
    units.push(0);
    units
}

fn transport() -> HttpError {
    // SAFETY: `GetLastError` reads this thread's last error and takes no pointer.
    let os = i32::try_from(unsafe { GetLastError() }).unwrap_or(i32::MAX);
    HttpError::Transport { os }
}

fn open_session() -> Result<Internet, HttpError> {
    let agent = wide("vcxtoolkit");
    // SAFETY: `agent` is a NUL-terminated wide string and outlives the call. The
    // proxy name and bypass are null, which selects the automatic proxy.
    let raw = unsafe {
        WinHttpOpen(
            agent.as_ptr(),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            ptr::null(),
            ptr::null(),
            0,
        )
    };
    if raw.is_null() {
        return Err(transport());
    }
    let session = Internet(raw);
    // SAFETY: `session.0` is the open session and stays valid for the call.
    let set = unsafe { WinHttpSetTimeouts(session.0, TIMEOUT_MS, TIMEOUT_MS, TIMEOUT_MS, TIMEOUT_MS) };
    if set == 0 {
        return Err(transport());
    }
    Ok(session)
}

fn connect(session: &Internet, target: &Target) -> Result<Internet, HttpError> {
    let host = wide(&target.host);
    // SAFETY: `session.0` is the open session. `host` is NUL-terminated and
    // outlives the call. The reserved argument is 0.
    let raw = unsafe { WinHttpConnect(session.0, host.as_ptr(), target.port, 0) };
    if raw.is_null() {
        return Err(transport());
    }
    Ok(Internet(raw))
}

fn open_request(connection: &Internet, target: &Target) -> Result<Internet, HttpError> {
    let verb = wide("GET");
    let path = wide(&target.path);
    let flags = if target.https { WINHTTP_FLAG_SECURE } else { 0 };
    // SAFETY: `connection.0` is the open connection. `verb` and `path` are
    // NUL-terminated and outlive the call. The version, referrer, and accept
    // list are null, which is the default GET.
    let raw = unsafe {
        WinHttpOpenRequest(
            connection.0,
            verb.as_ptr(),
            path.as_ptr(),
            ptr::null(),
            ptr::null(),
            ptr::null_mut(),
            flags,
        )
    };
    if raw.is_null() {
        return Err(transport());
    }
    Ok(Internet(raw))
}

fn add_header(request: &Internet, header: &str) -> Result<(), HttpError> {
    let wide_header = wide(header);
    let len = u32::try_from(header.encode_utf16().count()).map_err(|_| transport())?;
    // SAFETY: `request.0` is the open request. `wide_header` is NUL-terminated
    // and outlives the call. `len` is the character count without the NUL.
    let added = unsafe {
        WinHttpAddRequestHeaders(request.0, wide_header.as_ptr(), len, WINHTTP_ADDREQ_FLAG_ADD)
    };
    if added == 0 {
        return Err(transport());
    }
    Ok(())
}

fn send_and_receive(request: &Internet) -> Result<(), HttpError> {
    // SAFETY: `request.0` is the open request. There is no extra header and no
    // body. The context is 0.
    let sent = unsafe {
        WinHttpSendRequest(
            request.0,
            ptr::null(),
            0,
            ptr::null(),
            0,
            0,
            0,
        )
    };
    if sent == 0 {
        return Err(transport());
    }
    // SAFETY: `request.0` is the request that was just sent. The reserved pointer is null.
    let received = unsafe { WinHttpReceiveResponse(request.0, ptr::null_mut()) };
    if received == 0 {
        return Err(transport());
    }
    Ok(())
}

fn status_code(request: &Internet) -> Result<u32, HttpError> {
    let mut status = 0_u32;
    let mut len = u32::try_from(size_of::<u32>()).map_err(|_| transport())?;
    // SAFETY: `request.0` is the completed request. `status` and `len` are
    // writable for the call. The name and the index are null, which is what a
    // numeric status query uses.
    let ok = unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            ptr::null(),
            (&raw mut status).cast(),
            &raw mut len,
            ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(transport());
    }
    Ok(status)
}

fn read_body(request: &Internet) -> Result<Vec<u8>, HttpError> {
    let mut body = Vec::new();
    let mut buf = [0_u8; 8192];
    loop {
        let mut read = 0_u32;
        // SAFETY: `request.0` is the completed request. `buf` is writable and
        // outlives the call. `read` receives the count.
        let ok = unsafe {
            WinHttpReadData(
                request.0,
                buf.as_mut_ptr().cast(),
                u32::try_from(buf.len()).map_err(|_| transport())?,
                &raw mut read,
            )
        };
        if ok == 0 {
            return Err(transport());
        }
        if read == 0 {
            return Ok(body);
        }
        let count = usize::try_from(read).map_err(|_| transport())?;
        body.extend_from_slice(&buf[..count]);
    }
}
