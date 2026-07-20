//! Compat shims for a handful of libc runtime functions.
//!
//! The c2rust transpile redeclares `memcpy`/`memset`/`memcmp`/`memmove`/
//! `strlen`/`read`/`realloc`/`malloc` in a local `extern "C" { ... }` block
//! in every file that uses them, with size/length parameters typed as
//! `libc::c_ulong` (following C's `size_t`) rather than `libc::size_t`'s
//! actual Rust representation, `usize`. `c_ulong` and `usize` are the same
//! width on every target this project ships for, so there's no real ABI
//! bug — but each of those ~250 local declarations is technically a
//! mismatched redeclaration of a well-known runtime symbol, which is
//! exactly what rustc's `suspicious_runtime_symbol_definitions` lint is
//! for (~385 of the ~433 total build warnings come from this one pattern).
//!
//! The actual `libc` crate's declarations are correct (`usize`-typed), but
//! repointing every call site at `libc::memcpy` directly would mean
//! recasting the argument expression at every one of those ~1000+ call
//! sites from `libc::c_ulong` to `usize`. Instead: these are ordinary Rust
//! functions (not `extern "C"`), so they don't redeclare the runtime
//! symbol at all — rustc has no reason to warn about them — and they keep
//! the exact `c_ulong`-based signature every call site already uses, so no
//! call site needs its argument types touched, only its import.

use libc::{c_char, c_int, c_ulong, c_void, ssize_t};

/// # Safety
/// Same preconditions as `libc::memcpy`: `dst` and `src` must be valid for
/// `n` bytes and must not overlap.
pub unsafe fn memcpy(dst: *mut c_void, src: *const c_void, n: c_ulong) -> *mut c_void {
  libc::memcpy(dst, src, n as usize)
}

/// # Safety
/// Same preconditions as `libc::memset`: `dst` must be valid for `n` bytes.
pub unsafe fn memset(dst: *mut c_void, val: c_int, n: c_ulong) -> *mut c_void {
  libc::memset(dst, val, n as usize)
}

/// # Safety
/// Same preconditions as `libc::memcmp`: both pointers must be valid for
/// `n` bytes.
pub unsafe fn memcmp(a: *const c_void, b: *const c_void, n: c_ulong) -> c_int {
  libc::memcmp(a, b, n as usize)
}

/// # Safety
/// Same preconditions as `libc::memmove`: `dst` and `src` must be valid for
/// `n` bytes (unlike memcpy, overlap is allowed).
pub unsafe fn memmove(dst: *mut c_void, src: *const c_void, n: c_ulong) -> *mut c_void {
  libc::memmove(dst, src, n as usize)
}

/// # Safety
/// `s` must point to a valid NUL-terminated C string.
pub unsafe fn strlen(s: *const c_char) -> c_ulong {
  libc::strlen(s) as c_ulong
}

/// # Safety
/// Same preconditions as `libc::read`: `buf` must be valid for `nbytes`
/// bytes and `fd` must be a valid file descriptor.
pub unsafe fn read(fd: c_int, buf: *mut c_void, nbytes: c_ulong) -> ssize_t {
  libc::read(fd, buf, nbytes as usize)
}

/// # Safety
/// Same preconditions as `libc::realloc`: `ptr` must be null or a pointer
/// previously returned by malloc/calloc/realloc and not yet freed.
pub unsafe fn realloc(ptr: *mut c_void, size: c_ulong) -> *mut c_void {
  libc::realloc(ptr, size as usize)
}

/// # Safety
/// No special preconditions beyond what `libc::malloc` requires.
pub unsafe fn malloc(size: c_ulong) -> *mut c_void {
  libc::malloc(size as usize)
}
