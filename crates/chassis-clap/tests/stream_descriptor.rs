#![allow(unsafe_code)]
//! Foreign state stream descriptors are read-only, even for mutable Rust I/O.

use clack_plugin::stream::{InputStream, OutputStream, StreamError};
use clap_sys::stream::{clap_istream, clap_ostream};
use std::{
    cell::Cell,
    ffi::c_void,
    io::{Read, Write},
    ptr,
};

struct Context {
    descriptor: Cell<*const c_void>,
    bytes: Cell<[u8; 4]>,
    position: Cell<usize>,
    calls: Cell<usize>,
    fail: Cell<bool>,
    wrong_descriptor: Cell<bool>,
}
impl Context {
    fn new(bytes: [u8; 4]) -> Self {
        Self {
            descriptor: Cell::new(ptr::null()),
            bytes: Cell::new(bytes),
            position: Cell::new(0),
            calls: Cell::new(0),
            fail: Cell::new(false),
            wrong_descriptor: Cell::new(false),
        }
    }
    fn accepts(&self, descriptor: *const c_void) -> bool {
        self.calls.set(self.calls.get() + 1);
        if descriptor == self.descriptor.get() {
            !self.fail.get()
        } else {
            self.wrong_descriptor.set(true);
            false
        }
    }
}

unsafe extern "C" fn read(stream: *const clap_istream, buffer: *mut c_void, size: u64) -> i64 {
    // SAFETY: the tests loan this immutable descriptor and live Cell-owned context.
    let context = unsafe { &*((*stream).ctx.cast::<Context>()) };
    if !context.accepts(stream.cast()) {
        return -1;
    }
    let position = context.position.get();
    let count = usize::try_from(size).unwrap().min(2).min(4 - position);
    let bytes = context.bytes.get();
    if count != 0 {
        // SAFETY: Read loans size writable bytes; count fits both disjoint buffers.
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr().add(position), buffer.cast(), count);
        }
    }
    context.position.set(position + count);
    i64::try_from(count).unwrap()
}

unsafe extern "C" fn write(stream: *const clap_ostream, buffer: *const c_void, size: u64) -> i64 {
    // SAFETY: the tests loan this immutable descriptor and live Cell-owned context.
    let context = unsafe { &*((*stream).ctx.cast::<Context>()) };
    if !context.accepts(stream.cast()) {
        return -1;
    }
    let position = context.position.get();
    let count = usize::try_from(size).unwrap().min(2).min(4 - position);
    let mut bytes = context.bytes.get();
    if count != 0 {
        // SAFETY: Write loans size readable bytes; count fits both disjoint buffers.
        unsafe {
            ptr::copy_nonoverlapping(buffer.cast(), bytes.as_mut_ptr().add(position), count);
        }
    }
    context.bytes.set(bytes);
    context.position.set(position + count);
    i64::try_from(count).unwrap()
}

#[test]
fn input_preserves_original_descriptor_partial_reads_errors_and_safe_replacement() {
    let context = Context::new(*b"data");
    let raw = clap_istream {
        ctx: ptr::from_ref(&context).cast_mut().cast(),
        read: Some(read),
    };
    context.descriptor.set(ptr::from_ref(&raw).cast());
    // Empty is zero-sized: this supplies a static replacement loan without
    // retaining an allocated payload or extending the foreign context lifetime.
    let replacement = Box::leak(Box::new(std::io::empty()));
    // SAFETY: raw/context and its serialized callback remain live for this scope.
    unsafe {
        InputStream::with_raw(&raw, |stream| {
            let mut bytes = [0; 4];
            stream.read_exact(&mut bytes).unwrap();
            assert_eq!(&bytes, b"data");
            assert_eq!(
                context.calls.get(),
                2,
                "partial reads must repeat the original callback"
            );
            assert_eq!(stream.read(&mut bytes).unwrap(), 0);
            context.fail.set(true);
            let error = stream.read(&mut bytes).unwrap_err();
            assert_eq!(
                error
                    .get_ref()
                    .unwrap()
                    .downcast_ref::<StreamError>()
                    .unwrap()
                    .code(),
                -1
            );
            *stream = InputStream::from_reader(replacement);
            let calls = context.calls.get();
            assert_eq!(stream.read(&mut bytes).unwrap(), 0);
            assert_eq!(context.calls.get(), calls);
        });
    }
    assert!(!context.wrong_descriptor.get());
    assert_eq!(raw.ctx, ptr::from_ref(&context).cast_mut().cast());
    // Function addresses are not identity tokens. Exercise the original table
    // after replacement to prove its callback/context still perform real I/O.
    context.fail.set(false);
    context.position.set(0);
    let calls = context.calls.get();
    let mut bytes = [0; 4];
    // SAFETY: original descriptor/context are live, and bytes loans four bytes.
    assert_eq!(
        unsafe { raw.read.unwrap()(ptr::from_ref(&raw), bytes.as_mut_ptr().cast(), 4) },
        2
    );
    assert_eq!(&bytes[..2], b"da");
    assert_eq!(context.calls.get(), calls + 1);
}

#[test]
fn output_preserves_original_descriptor_partial_writes_errors_and_safe_replacement() {
    let context = Context::new([0; 4]);
    let raw = clap_ostream {
        ctx: ptr::from_ref(&context).cast_mut().cast(),
        write: Some(write),
    };
    context.descriptor.set(ptr::from_ref(&raw).cast());
    // Sink is likewise zero-sized; no allocated data survives this test.
    let replacement = Box::leak(Box::new(std::io::sink()));
    // SAFETY: raw/context and its serialized callback remain live for this scope.
    unsafe {
        OutputStream::with_raw(&raw, |stream| {
            stream.write_all(b"data").unwrap();
            assert_eq!(context.bytes.get(), *b"data");
            assert_eq!(
                context.calls.get(),
                2,
                "partial writes must repeat the original callback"
            );
            context.fail.set(true);
            let error = stream.write(b"x").unwrap_err();
            assert_eq!(
                error
                    .get_ref()
                    .unwrap()
                    .downcast_ref::<StreamError>()
                    .unwrap()
                    .code(),
                -1
            );
            *stream = OutputStream::from_writer(replacement);
            let calls = context.calls.get();
            stream.write_all(b"ignored").unwrap();
            assert_eq!(context.calls.get(), calls);
        });
    }
    assert!(!context.wrong_descriptor.get());
    assert_eq!(raw.ctx, ptr::from_ref(&context).cast_mut().cast());
    context.fail.set(false);
    context.position.set(0);
    let calls = context.calls.get();
    // SAFETY: original descriptor/context are live; the literal loans four bytes.
    assert_eq!(
        unsafe { raw.write.unwrap()(ptr::from_ref(&raw), b"next".as_ptr().cast(), 4) },
        2
    );
    assert_eq!(&context.bytes.get()[..2], b"ne");
    assert_eq!(context.calls.get(), calls + 1);
}

#[test]
fn absent_callbacks_are_empty_without_dereferencing_null_contexts() {
    let input = clap_istream {
        ctx: ptr::null_mut(),
        read: None,
    };
    let output = clap_ostream {
        ctx: ptr::null_mut(),
        write: None,
    };
    // SAFETY: descriptors are live; absent callbacks never access their contexts.
    unsafe {
        InputStream::with_raw(&input, |stream| {
            assert_eq!(stream.read(&mut [0; 4]).unwrap(), 0);
        });
        OutputStream::with_raw(&output, |stream| {
            assert_eq!(stream.write(b"data").unwrap(), 0);
        });
    }
}
