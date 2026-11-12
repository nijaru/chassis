//! CLAP I/O stream utilities.
//!
//! Provides `InputStream` and `OutputStream` wrappers that implement
//! `std::io::Read` and `std::io::Write`, making CLAP streams integrate
//! cleanly with Rust’s I/O traits.
//!
//! # Notes
//! Hosts may restrict how many bytes can be read or written at once.
//! These wrappers handle that by looping until all data is transferred,
//! so you don’t need to worry about partial reads or writes.

use crate::utils::{slice_from_external_parts, slice_from_external_parts_mut};
use clap_sys::stream::{clap_istream, clap_ostream};
use std::error::Error;
use std::ffi::c_void;
use std::fmt::{Display, Formatter};
use std::io::{ErrorKind, Read, Write};
use std::marker::PhantomData;

/// An error code that can be raised by CLAP stream methods.
#[derive(Copy, Clone, Debug)]
pub struct StreamError {
    code: i64,
}

impl StreamError {
    /// Returns the underlying error code returned by the CLAP stream.
    pub fn code(&self) -> i64 {
        self.code
    }
}

impl Display for StreamError {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "CLAP stream error (code: {})", self.code)
    }
}

impl Error for StreamError {}

/// A CLAP data stream that can be read from.
///
/// This helper struct is designed to work with the standard [`Read`] trait.
#[repr(C)]
pub struct InputStream<'a>(clap_istream, PhantomData<(&'a mut clap_istream, *const ())>);

impl<'a> InputStream<'a> {
    /// Creates a new input stream for an existing [reader](Read) implementation.
    pub fn from_reader<R: Read + Sized + 'a>(reader: &'a mut R) -> Self {
        Self(
            clap_istream {
                ctx: reader as *mut R as *mut _,
                read: Some(read::<R>),
            },
            PhantomData,
        )
    }

    /// Invokes a callback with a locally owned view of a read-only CLAP stream.
    ///
    /// Reads preserve the original descriptor address, context and return value.
    /// Replacing the mutable view cannot overwrite the host's descriptor, and
    /// its borrowed context cannot escape this scope through safe Rust.
    ///
    /// ```compile_fail
    /// use clack_common::stream::InputStream;
    /// fn escape(reader: &mut impl std::io::Read) {
    ///     let mut owner = InputStream::from_reader(reader);
    ///     let _escaped = unsafe { InputStream::with_raw(owner.as_raw_mut(), |stream| {
    ///         std::mem::replace(stream, InputStream::from_reader(Box::leak(Box::new(std::io::empty()))))
    ///     }) };
    /// }
    /// ```
    ///
    /// # Safety
    /// The original descriptor and its callback/context must remain valid for
    /// serialized reads throughout the callback, as required by CLAP.
    pub unsafe fn with_raw<R>(
        raw: &clap_istream,
        callback: impl for<'scope> FnOnce(&'scope mut InputStream<'scope>) -> R,
    ) -> R {
        unsafe extern "C" fn forward(
            stream: *const clap_istream,
            buffer: *mut c_void,
            size: u64,
        ) -> i64 {
            // SAFETY: with_raw owns this forwarding descriptor; its context is
            // the original live, read-only table guaranteed by the outer caller.
            let original = unsafe { &*((*stream).ctx.cast::<clap_istream>()) };
            match original.read {
                // SAFETY: preserve the original stream and caller's buffer loan.
                Some(read) => unsafe { read(original, buffer, size) },
                None => 0,
            }
        }
        let mut input = InputStream(
            clap_istream {
                ctx: (raw as *const clap_istream).cast_mut().cast(),
                read: Some(forward),
            },
            PhantomData,
        );
        callback(&mut input)
    }

    /// Creates a new input stream for a C FFI-compatible pointer.
    ///
    /// # Safety
    ///
    /// The caller must ensure the given `clap_istream` instance is valid.
    #[inline]
    pub unsafe fn from_raw_mut(raw: &mut clap_istream) -> &mut Self {
        &mut *(raw as *mut _ as *mut _)
    }

    /// Returns this input stream as a C FFI-compatible mutable descriptor.
    ///
    /// # Safety
    /// Any changes must preserve callback/context validity and lifetime. Invalid
    /// fields can make subsequent safe reads undefined behavior.
    ///
    /// ```compile_fail,E0133
    /// use clack_common::stream::InputStream;
    /// fn invalidate(stream: &mut InputStream<'_>) {
    ///     stream.as_raw_mut().ctx = std::ptr::null_mut();
    /// }
    /// ```
    #[inline]
    pub unsafe fn as_raw_mut(&mut self) -> &mut clap_istream {
        &mut self.0
    }
}

impl Read for InputStream<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let ret = if let Some(read) = self.0.read {
            // SAFETY: this function pointer is guaranteed to be valid by from_raw_mut and from_reader
            unsafe { read(&self.0, buf.as_mut_ptr().cast(), buf.len() as u64) }
        } else {
            return Ok(0);
        };
        match ret {
            i if i >= 0 => Ok(usize::try_from(i).map_err(std::io::Error::other)?),
            code => Err(std::io::Error::other(StreamError { code })),
        }
    }
}

/// A CLAP data stream that can be written to.
///
/// This helper struct is designed to work with the standard [`Write`] trait.
#[repr(C)]
pub struct OutputStream<'a>(clap_ostream, PhantomData<(&'a mut clap_ostream, *const ())>);

impl<'a> OutputStream<'a> {
    /// Creates a new output stream for an existing [write](Write) implementation.
    pub fn from_writer<W: Write + Sized + 'a>(writer: &'a mut W) -> Self {
        Self(
            clap_ostream {
                ctx: writer as *mut W as *mut _,
                write: Some(write::<W>),
            },
            PhantomData,
        )
    }

    /// Invokes a callback with a locally owned view of a read-only CLAP stream.
    ///
    /// Writes preserve the original descriptor address, context and return value.
    /// Replacing the mutable view cannot overwrite the host's descriptor, and
    /// its borrowed context cannot escape this scope through safe Rust.
    ///
    /// ```compile_fail
    /// use clack_common::stream::OutputStream;
    /// fn escape(writer: &mut impl std::io::Write) {
    ///     let mut owner = OutputStream::from_writer(writer);
    ///     let _escaped = unsafe { OutputStream::with_raw(owner.as_raw_mut(), |stream| {
    ///         std::mem::replace(stream, OutputStream::from_writer(Box::leak(Box::new(std::io::sink()))))
    ///     }) };
    /// }
    /// ```
    ///
    /// # Safety
    /// The original descriptor and its callback/context must remain valid for
    /// serialized writes throughout the callback, as required by CLAP.
    pub unsafe fn with_raw<R>(
        raw: &clap_ostream,
        callback: impl for<'scope> FnOnce(&'scope mut OutputStream<'scope>) -> R,
    ) -> R {
        unsafe extern "C" fn forward(
            stream: *const clap_ostream,
            buffer: *const c_void,
            size: u64,
        ) -> i64 {
            // SAFETY: with_raw owns this forwarding descriptor; its context is
            // the original live, read-only table guaranteed by the outer caller.
            let original = unsafe { &*((*stream).ctx.cast::<clap_ostream>()) };
            match original.write {
                // SAFETY: preserve the original stream and caller's buffer loan.
                Some(write) => unsafe { write(original, buffer, size) },
                None => 0,
            }
        }
        let mut output = OutputStream(
            clap_ostream {
                ctx: (raw as *const clap_ostream).cast_mut().cast(),
                write: Some(forward),
            },
            PhantomData,
        );
        callback(&mut output)
    }

    /// Creates a new output stream for a C FFI-compatible pointer.
    ///
    /// # Safety
    ///
    /// The caller must ensure the given `clap_ostream` instance is valid.
    #[inline]
    pub unsafe fn from_raw_mut(raw: &mut clap_ostream) -> &mut Self {
        &mut *(raw as *mut _ as *mut _)
    }

    /// Returns this output stream as a C FFI-compatible mutable descriptor.
    ///
    /// # Safety
    /// Any changes must preserve callback/context validity and lifetime. Invalid
    /// fields can make subsequent safe writes undefined behavior.
    ///
    /// ```compile_fail,E0133
    /// use clack_common::stream::OutputStream;
    /// fn invalidate(stream: &mut OutputStream<'_>) {
    ///     stream.as_raw_mut().ctx = std::ptr::null_mut();
    /// }
    /// ```
    #[inline]
    pub unsafe fn as_raw_mut(&mut self) -> &mut clap_ostream {
        &mut self.0
    }
}

impl Write for OutputStream<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let ret = if let Some(write) = self.0.write {
            // SAFETY: this function pointer is guaranteed to be valid by from_raw_mut and from_reader
            unsafe { write(&self.0, buf.as_ptr().cast(), buf.len() as u64) }
        } else {
            return Ok(0);
        };

        match ret {
            i if i >= 0 => Ok(usize::try_from(i).map_err(std::io::Error::other)?),
            code => Err(std::io::Error::other(StreamError { code })),
        }
    }

    #[inline]
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

// Guaranteed to not be able to overflow either i64 or usize
fn saturating_from_u64(val: u64) -> usize {
    // Cap size to i64 first, so that return value range is coherent
    let val = i64::try_from(val).unwrap_or(i64::MAX);

    // Can only happen on <= 32bit platforms
    usize::try_from(val).unwrap_or(usize::MAX)
}

#[allow(clippy::missing_safety_doc)]
unsafe extern "C" fn read<R: Read + Sized>(
    istream: *const clap_istream,
    buffer: *mut c_void,
    size: u64,
) -> i64 {
    let reader = &mut *((*istream).ctx as *mut R);
    let size = saturating_from_u64(size);

    let buffer = slice_from_external_parts_mut(buffer as *mut u8, size);

    match handle_interrupted(|| reader.read(buffer)) {
        #[allow(clippy::cast_possible_wrap)]
        Ok(read) => {
            // Read implementation is bad. We return the size of the buffer instead.
            if read > size {
                // We know size fits in i64
                size as i64
            } else {
                // Since read <= size and size fits in i64, then read also fits in i64
                read as i64
            }
        }
        Err(_) => -1,
    }
}

#[allow(clippy::missing_safety_doc)]
unsafe extern "C" fn write<W: Write + Sized>(
    ostream: *const clap_ostream,
    buffer: *const c_void,
    size: u64,
) -> i64 {
    let writer = &mut *((*ostream).ctx as *mut W);
    let size = saturating_from_u64(size);

    let buffer = slice_from_external_parts(buffer as *const u8, size);

    match handle_interrupted(|| writer.write(buffer)) {
        #[allow(clippy::cast_possible_wrap)]
        Ok(written) => {
            // Read implementation is bad. We return the size of the buffer instead.
            if written > size {
                // We know size fits in i64
                size as i64
            } else {
                // Since written <= size and size fits in i64, then written also firs in i64
                written as i64
            }
        }
        Err(_) => -1,
    }
}

fn handle_interrupted<F: FnMut() -> std::io::Result<usize>>(
    mut handler: F,
) -> std::io::Result<usize> {
    const MAX_ATTEMPTS: u8 = 5;
    let mut attempts = 0u8;

    loop {
        match handler() {
            Err(e) if e.kind() == ErrorKind::Interrupted && attempts < MAX_ATTEMPTS => {
                attempts += 1
            }
            res => return res,
        }
    }
}

#[cfg(test)]
mod test {
    extern crate static_assertions as sa;
    use super::*;
    use std::io::Cursor;

    sa::assert_not_impl_any!(InputStream: Send, Sync);
    sa::assert_not_impl_any!(OutputStream: Send, Sync);

    #[test]
    fn input_streams_work() {
        let src = b"Hello";
        let mut buf = vec![0; 5];
        let mut cursor = Cursor::new(src);

        let mut stream = InputStream::from_reader(&mut cursor);
        let res = stream.read(&mut buf).unwrap();
        assert_eq!(res, 5);
        assert_eq!(&buf, b"Hello");
    }

    #[test]
    fn output_streams_work() {
        let mut buf = vec![];

        let mut stream = OutputStream::from_writer(&mut buf);
        let res = stream.write(b"Hello").unwrap();

        assert_eq!(res, 5);
        assert_eq!(&buf, b"Hello");
    }
}
