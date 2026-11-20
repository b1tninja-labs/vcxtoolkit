//! Win32 calls for the VCX USB serial open.
//!
//! `vcxcomm` forbids `unsafe`. `CreateFile`, the DCB, the timeouts, the event mask,
//! and the queue sizes are FFI, so they live here.

#![cfg(windows)]

use std::io;
use std::os::windows::io::OwnedHandle;

#[allow(unsafe_code)]
mod ffi {
    use std::io;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};

    use windows_sys::Win32::Devices::Communication::{
        COMMTIMEOUTS, DCB, GetCommMask, GetCommState, GetCommTimeouts, SetCommMask, SetCommState,
        SetCommTimeouts, SetupComm,
    };
    use windows_sys::Win32::Foundation::{
        ERROR_IO_PENDING, GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileA, FILE_FLAG_OVERLAPPED, OPEN_EXISTING, ReadFile,
    };
    use windows_sys::Win32::System::IO::{GetOverlappedResult, OVERLAPPED};
    use windows_sys::Win32::System::Threading::CreateEventW;

    use crate::{Call, Error, Line, Snapshot};

    /// Share mode 0: the DLL does not pass `FILE_SHARE_READ` or `FILE_SHARE_WRITE`.
    pub(crate) const SHARE_MODE: u32 = 0;

    /// `FILE_FLAG_OVERLAPPED` alone. The DLL does not OR in `FILE_ATTRIBUTE_NORMAL`.
    pub(crate) const FILE_FLAGS: u32 = FILE_FLAG_OVERLAPPED;

    pub(crate) const ACCESS: u32 = GENERIC_READ | GENERIC_WRITE;

    pub(crate) fn create(path: &str) -> Result<OwnedHandle, Error> {
        let mut bytes = path.as_bytes().to_vec();
        bytes.push(0);
        // SAFETY: `bytes` is a NUL-terminated path and outlives this call. The security
        // descriptor and the template handle are null, which is what `SerialCommOpen` passes.
        let raw = unsafe {
            CreateFileA(
                bytes.as_ptr(),
                ACCESS,
                SHARE_MODE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAGS,
                std::ptr::null_mut(),
            )
        };
        if raw == INVALID_HANDLE_VALUE {
            return Err(Error::new(Call::CreateFile));
        }
        // SAFETY: `raw` is an open `CreateFile` handle, it is not `INVALID_HANDLE_VALUE`,
        // and this function takes exclusive ownership of it.
        Ok(unsafe { OwnedHandle::from_raw_handle(raw) })
    }

    pub(crate) fn configure(
        handle: &OwnedHandle,
        line: &Line,
        edit_flags: impl FnOnce(u32) -> u32,
    ) -> Result<(), Error> {
        let raw = handle.as_raw_handle();
        let mut dcb = std::mem::MaybeUninit::<DCB>::zeroed();
        // SAFETY: `DCB` is a C struct. An all-zero pattern is valid, and `DCBlength` is set
        // before `GetCommState`, which is what that call requires.
        let dcb = unsafe {
            let dcb = dcb.assume_init_mut();
            dcb.DCBlength = dword(std::mem::size_of::<DCB>())?;
            if GetCommState(raw, dcb) == 0 {
                return Err(Error::new(Call::GetCommState));
            }
            dcb.BaudRate = line.baud;
            dcb.ByteSize = line.byte_size;
            dcb.Parity = line.parity;
            dcb.StopBits = line.stop_bits;
            dcb._bitfield = edit_flags(dcb._bitfield);
            dcb
        };
        // SAFETY: `raw` is the open port and `dcb` is a filled `DCB` of the length we set.
        // The pointer stays valid for the call.
        let set = unsafe { SetCommState(raw, dcb) };
        if set == 0 {
            return Err(Error::new(Call::SetCommState));
        }
        let timeouts = COMMTIMEOUTS {
            ReadIntervalTimeout: line.read_interval_timeout,
            ReadTotalTimeoutMultiplier: line.read_total_timeout_multiplier,
            ReadTotalTimeoutConstant: line.read_total_timeout_constant,
            WriteTotalTimeoutMultiplier: line.write_total_timeout_multiplier,
            WriteTotalTimeoutConstant: line.write_total_timeout_constant,
        };
        // SAFETY: `raw` is the open port and `timeouts` stays valid for the call.
        let set = unsafe { SetCommTimeouts(raw, &raw const timeouts) };
        if set == 0 {
            return Err(Error::new(Call::SetCommTimeouts));
        }
        // SAFETY: `raw` is the open port.
        let set = unsafe { SetCommMask(raw, line.event_mask) };
        if set == 0 {
            return Err(Error::new(Call::SetCommMask));
        }
        // SAFETY: `raw` is the open port. The sizes are the queue lengths in bytes.
        let set = unsafe { SetupComm(raw, line.in_queue, line.out_queue) };
        if set == 0 {
            return Err(Error::new(Call::SetupComm));
        }
        Ok(())
    }

    pub(crate) fn snapshot(handle: &OwnedHandle) -> Result<Snapshot, Error> {
        let raw = handle.as_raw_handle();
        let mut dcb = std::mem::MaybeUninit::<DCB>::zeroed();
        // SAFETY: `DCB` is a C struct. An all-zero pattern is valid, and `DCBlength` is set
        // before `GetCommState`.
        let (baud, byte_size, parity, stop_bits, flags) = unsafe {
            let dcb = dcb.assume_init_mut();
            dcb.DCBlength = dword(std::mem::size_of::<DCB>())?;
            if GetCommState(raw, dcb) == 0 {
                return Err(Error::new(Call::GetCommState));
            }
            (
                dcb.BaudRate,
                dcb.ByteSize,
                dcb.Parity,
                dcb.StopBits,
                dcb._bitfield,
            )
        };
        let mut timeouts = std::mem::MaybeUninit::<COMMTIMEOUTS>::zeroed();
        // SAFETY: `COMMTIMEOUTS` is five `u32`s. An all-zero pattern is valid, and the
        // pointer stays valid for the call.
        let timeouts = unsafe {
            let timeouts = timeouts.assume_init_mut();
            if GetCommTimeouts(raw, timeouts) == 0 {
                return Err(Error::new(Call::GetCommTimeouts));
            }
            *timeouts
        };
        let mut mask = 0_u32;
        // SAFETY: `raw` is the open port and `mask` stays valid for the call.
        let got = unsafe { GetCommMask(raw, &raw mut mask) };
        if got == 0 {
            return Err(Error::new(Call::GetCommMask));
        }
        Ok(Snapshot {
            baud,
            byte_size,
            parity,
            stop_bits,
            flags,
            read_interval_timeout: timeouts.ReadIntervalTimeout,
            read_total_timeout_multiplier: timeouts.ReadTotalTimeoutMultiplier,
            read_total_timeout_constant: timeouts.ReadTotalTimeoutConstant,
            write_total_timeout_multiplier: timeouts.WriteTotalTimeoutMultiplier,
            write_total_timeout_constant: timeouts.WriteTotalTimeoutConstant,
            event_mask: mask,
        })
    }

    pub(crate) fn read(handle: &OwnedHandle, buf: &mut [u8]) -> Result<usize, Error> {
        let len = dword(buf.len()).map_err(|_| {
            Error::from_io(
                Call::Read,
                io::Error::new(io::ErrorKind::InvalidInput, "buffer longer than a DWORD"),
            )
        })?;
        let event = create_event()?;
        // SAFETY: `OVERLAPPED` is a C struct of integers and pointers. An all-zero pattern
        // is a valid overlapped block with a null event, and the event is filled in next.
        let mut overlapped = unsafe { std::mem::zeroed::<OVERLAPPED>() };
        overlapped.hEvent = event.as_raw_handle();
        let mut count = 0_u32;
        let raw = handle.as_raw_handle();
        // SAFETY: `raw` is the overlapped port, `buf` and `overlapped` and `count` stay
        // valid until `GetOverlappedResult` has returned or this call completed at once.
        // The event in `overlapped` stays open for that same span.
        let ok = unsafe {
            ReadFile(
                raw,
                buf.as_mut_ptr().cast(),
                len,
                &raw mut count,
                &raw mut overlapped,
            )
        };
        if ok != 0 {
            return Ok(widen(count));
        }
        let error = io::Error::last_os_error();
        if !is_pending(&error) {
            return Err(Error::from_io(Call::Read, error));
        }
        let mut transferred = 0_u32;
        // SAFETY: the read returned `ERROR_IO_PENDING`. `overlapped` is the same block,
        // `raw` is the same port, and `bWait` is true, so this returns only after the
        // driver has finished with `buf`.
        let ok =
            unsafe { GetOverlappedResult(raw, &raw const overlapped, &raw mut transferred, 1) };
        if ok == 0 {
            return Err(Error::new(Call::Read));
        }
        Ok(widen(transferred))
    }

    fn create_event() -> Result<OwnedHandle, Error> {
        // SAFETY: the security attributes and the name are null, so this creates an unnamed
        // auto-reset event that is not signaled. On success the handle is owned here.
        let raw = unsafe { CreateEventW(std::ptr::null(), 0, 0, std::ptr::null()) };
        if raw.is_null() {
            return Err(Error::new(Call::Event));
        }
        // SAFETY: `raw` is a live event handle and this function takes exclusive ownership.
        Ok(unsafe { OwnedHandle::from_raw_handle(raw) })
    }

    fn is_pending(error: &io::Error) -> bool {
        error.raw_os_error() == i32::try_from(ERROR_IO_PENDING).ok()
    }

    fn widen(count: u32) -> usize {
        usize::try_from(count).unwrap_or(usize::MAX)
    }

    fn dword(len: usize) -> Result<u32, Error> {
        u32::try_from(len).map_err(|_| {
            Error::from_io(
                Call::Read,
                io::Error::new(io::ErrorKind::InvalidInput, "value does not fit in a DWORD"),
            )
        })
    }
}

/// Which Win32 call failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Call {
    /// `CreateFileA` returned `INVALID_HANDLE_VALUE`.
    CreateFile,
    /// `GetCommState` failed.
    GetCommState,
    /// `SetCommState` failed.
    SetCommState,
    /// `SetCommTimeouts` failed.
    SetCommTimeouts,
    /// `GetCommTimeouts` failed.
    GetCommTimeouts,
    /// `SetCommMask` failed.
    SetCommMask,
    /// `GetCommMask` failed.
    GetCommMask,
    /// `SetupComm` failed.
    SetupComm,
    /// `ReadFile` or `GetOverlappedResult` failed.
    Read,
    /// `CreateEventW` failed.
    Event,
}

impl Call {
    const fn label(self) -> &'static str {
        match self {
            Self::CreateFile => "CreateFile",
            Self::GetCommState => "GetCommState",
            Self::SetCommState => "SetCommState",
            Self::SetCommTimeouts => "SetCommTimeouts",
            Self::GetCommTimeouts => "GetCommTimeouts",
            Self::SetCommMask => "SetCommMask",
            Self::GetCommMask => "GetCommMask",
            Self::SetupComm => "SetupComm",
            Self::Read => "ReadFile",
            Self::Event => "CreateEvent",
        }
    }
}

impl std::fmt::Display for Call {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// A failed Win32 call, with the error from `GetLastError`.
#[derive(Debug)]
pub struct Error {
    call: Call,
    source: io::Error,
}

impl Error {
    fn new(call: Call) -> Self {
        Self::from_io(call, io::Error::last_os_error())
    }

    fn from_io(call: Call, source: io::Error) -> Self {
        Self { call, source }
    }

    /// The call that failed.
    #[must_use]
    pub const fn call(&self) -> Call {
        self.call
    }

    /// The `GetLastError` value as an [`io::Error`].
    #[must_use]
    pub const fn io(&self) -> &io::Error {
        &self.source
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.call, self.source)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// The line `SetCommState`, `SetCommTimeouts`, `SetCommMask`, and `SetupComm` apply.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Line {
    /// Baud in bits per second.
    pub baud: u32,
    /// Data bits. USB is 8.
    pub byte_size: u8,
    /// `NOPARITY` is 0.
    pub parity: u8,
    /// `ONESTOPBIT` is 0.
    pub stop_bits: u8,
    /// `ReadIntervalTimeout`.
    pub read_interval_timeout: u32,
    /// `ReadTotalTimeoutMultiplier`.
    pub read_total_timeout_multiplier: u32,
    /// `ReadTotalTimeoutConstant`.
    pub read_total_timeout_constant: u32,
    /// `WriteTotalTimeoutMultiplier`.
    pub write_total_timeout_multiplier: u32,
    /// `WriteTotalTimeoutConstant`.
    pub write_total_timeout_constant: u32,
    /// The `SetCommMask` value.
    pub event_mask: u32,
    /// `SetupComm` input queue, in bytes.
    pub in_queue: u32,
    /// `SetupComm` output queue, in bytes.
    pub out_queue: u32,
}

/// What the driver reported after the open.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Snapshot {
    /// Baud the driver has stored.
    pub baud: u32,
    /// Data bits.
    pub byte_size: u8,
    /// Parity byte.
    pub parity: u8,
    /// Stop-bit byte.
    pub stop_bits: u8,
    /// The DCB flag dword.
    pub flags: u32,
    /// `ReadIntervalTimeout`.
    pub read_interval_timeout: u32,
    /// `ReadTotalTimeoutMultiplier`.
    pub read_total_timeout_multiplier: u32,
    /// `ReadTotalTimeoutConstant`.
    pub read_total_timeout_constant: u32,
    /// `WriteTotalTimeoutMultiplier`.
    pub write_total_timeout_multiplier: u32,
    /// `WriteTotalTimeoutConstant`.
    pub write_total_timeout_constant: u32,
    /// The event mask `GetCommMask` returned.
    pub event_mask: u32,
}

/// An open overlapped COM handle.
#[derive(Debug)]
pub struct Com {
    handle: OwnedHandle,
}

impl Com {
    /// Open `path` with the `SerialCommOpen` `CreateFile` arguments. One try.
    ///
    /// # Errors
    ///
    /// Returns [`Call::CreateFile`] when the port cannot be opened.
    #[must_use = "the handle is closed when this value is dropped"]
    pub fn open(path: &str) -> Result<Self, Error> {
        Ok(Self {
            handle: ffi::create(path)?,
        })
    }

    /// Apply the DCB, the timeouts, the event mask, and the queue sizes.
    ///
    /// `edit_flags` receives the flag dword `GetCommState` returned and its result is
    /// written back. This function does not write a frame.
    ///
    /// # Errors
    ///
    /// Returns the first of `GetCommState`, `SetCommState`, `SetCommTimeouts`,
    /// `SetCommMask`, or `SetupComm` that fails.
    pub fn configure(&self, line: &Line, edit_flags: impl FnOnce(u32) -> u32) -> Result<(), Error> {
        ffi::configure(&self.handle, line, edit_flags)
    }

    /// Read the line back from the driver.
    ///
    /// # Errors
    ///
    /// Returns the first query that fails.
    pub fn snapshot(&self) -> Result<Snapshot, Error> {
        ffi::snapshot(&self.handle)
    }

    /// Read whatever is already buffered.
    ///
    /// A count of 0 means the driver had no bytes. The overlapped read is waited out
    /// before this returns, so the buffer is not used after the call.
    ///
    /// # Errors
    ///
    /// Returns [`Call::Read`] when `ReadFile` or `GetOverlappedResult` fails, and
    /// [`Call::Event`] when the overlapped event cannot be created.
    pub fn read(&self, buf: &mut [u8]) -> Result<usize, Error> {
        ffi::read(&self.handle, buf)
    }
}

#[cfg(test)]
mod tests {
    use super::ffi::{ACCESS, FILE_FLAGS, SHARE_MODE};
    use windows_sys::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE};
    use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_OVERLAPPED, OPEN_EXISTING};

    #[test]
    fn create_file_matches_serial_comm_open() {
        assert_eq!(ACCESS, GENERIC_READ | GENERIC_WRITE);
        assert_eq!(ACCESS, 0xC000_0000);
        assert_eq!(SHARE_MODE, 0);
        assert_eq!(OPEN_EXISTING, 3);
        assert_eq!(FILE_FLAGS, FILE_FLAG_OVERLAPPED);
        assert_eq!(FILE_FLAGS, 0x4000_0000);
    }

    #[test]
    fn the_dcb_is_28_bytes() {
        assert_eq!(
            std::mem::size_of::<windows_sys::Win32::Devices::Communication::DCB>(),
            28
        );
        assert_eq!(
            std::mem::size_of::<windows_sys::Win32::Devices::Communication::COMMTIMEOUTS>(),
            20
        );
    }
}
