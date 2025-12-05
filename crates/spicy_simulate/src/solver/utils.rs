use std::{mem, slice};

pub const EMPTY: isize = -1;

/// negation about -1, used to mark an integer i that is normally non-negative.
pub fn flip(x: isize) -> isize {
    -(x) - 2
}

pub fn is_flipped(x: isize) -> bool {
    x < -1
}

pub fn unflip(x: isize) -> isize {
    if is_flipped(x) { flip(x) } else { x }
}

/// Reinterpret a mutable slice of `isize` as a mutable slice of `usize`.
///
/// # Safety
/// - `isize` and `usize` must have the same size and alignment (true on all Rust platforms).
/// - While the returned slice is alive, the original `isize` slice must not be accessed.
pub(crate) unsafe fn as_usize_slice_mut(s: &mut [isize]) -> &mut [usize] {
    debug_assert_eq!(mem::size_of::<isize>(), mem::size_of::<usize>());
    debug_assert_eq!(mem::align_of::<isize>(), mem::align_of::<usize>());
    let len = s.len();
    let ptr = s.as_mut_ptr() as *mut usize;
    // SAFETY: caller upholds that the memory really contains `usize`-compatible values
    unsafe { slice::from_raw_parts_mut(ptr, len) }
}

/// Reinterpret a slice of `isize` as a slice of `usize`.
///
/// # Safety
/// - `isize` and `usize` must have the same size and alignment (true on all Rust platforms).
pub(crate) unsafe fn as_usize_slice(s: &[isize]) -> &[usize] {
    debug_assert_eq!(mem::size_of::<isize>(), mem::size_of::<usize>());
    debug_assert_eq!(mem::align_of::<isize>(), mem::align_of::<usize>());
    let len = s.len();
    let ptr = s.as_ptr() as *const usize;
    // SAFETY: caller upholds that the memory really contains `usize`-compatible values
    unsafe { slice::from_raw_parts(ptr, len) }
}