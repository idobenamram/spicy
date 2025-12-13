use std::{mem, slice};

pub const EMPTY: isize = -1;

/// negation about -1, used to mark an integer i that is normally non-negative.
pub fn flip(x: isize) -> isize {
    -(x) - 2
}

pub fn is_flipped(x: isize) -> bool {
    x < -1
}

// TODO: shouldn't this return a usize
pub fn unflip(x: isize) -> isize {
    if is_flipped(x) { flip(x) } else { x }
}

/// Reinterpret a mutable slice of `f64` as a mutable slice of `isize`.
///
/// # Safety
/// - `f64` and `isize` must have the same size and alignment (true on all Rust platforms).
/// - While the returned slice is alive, the original `isize` slice must not be accessed.
pub(crate) unsafe fn f64_as_isize_slice_mut(s: &mut [f64]) -> &mut [isize] {
    debug_assert_eq!(mem::size_of::<f64>(), mem::size_of::<isize>());
    debug_assert_eq!(mem::align_of::<f64>(), mem::align_of::<isize>());
    let len = s.len();
    let ptr = s.as_mut_ptr() as *mut isize;
    // SAFETY: caller upholds that the memory really contains `isize`-compatible values
    unsafe { slice::from_raw_parts_mut(ptr, len) }
}

/// Reinterpret a mutable slice of `f64` as a mutable slice of `usize`.
///
/// # Safety
/// - `f64` and `usize` must have the same size and alignment (true on all Rust platforms).
/// - While the returned slice is alive, the original `usize` slice must not be accessed.
pub(crate) unsafe fn f64_as_usize_slice_mut(s: &mut [f64]) -> &mut [usize] {
    debug_assert_eq!(mem::size_of::<f64>(), mem::size_of::<usize>());
    debug_assert_eq!(mem::align_of::<f64>(), mem::align_of::<usize>());
    let len = s.len();
    let ptr = s.as_mut_ptr() as *mut usize;
    // SAFETY: caller upholds that the memory really contains `isize`-compatible values
    unsafe { slice::from_raw_parts_mut(ptr, len) }
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

/// Reinterpret a slice of `f64` as a slice of `usize`.
///
/// # Safety
/// - `f64` and `usize` must have the same size and alignment (true on all Rust platforms).
pub(crate) unsafe fn f64_as_usize_slice(s: &[f64]) -> &[usize] {
    debug_assert_eq!(mem::size_of::<f64>(), mem::size_of::<usize>());
    debug_assert_eq!(mem::align_of::<f64>(), mem::align_of::<usize>());
    let len = s.len();
    let ptr = s.as_ptr() as *const usize;
    // SAFETY: caller upholds that the memory really contains `usize`-compatible values
    unsafe { slice::from_raw_parts(ptr, len) }
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

/// Rust equivalent of the KLU `DUNITS` macro.
///
/// In the C implementation (`klu_version.h`):
/// `DUNITS(type,n) = ceil (sizeof(type) * (double)n / sizeof(Unit))`
///
/// Here `Unit` is `f64` (the numeric storage type for LU data).  This helper
/// returns how many `Unit`-sized slots are required to store `n` values of
/// type `T`, rounding up, and checks for overflow.
pub(crate) fn dunits<T>(n: usize) -> Result<usize, String> {
    let type_bytes = mem::size_of::<T>();
    let unit_bytes = mem::size_of::<f64>();

    // bytes = sizeof(T) * n  (checked to avoid overflow)
    let bytes = type_bytes
        .checked_mul(n)
        .ok_or_else(|| "overflow computing DUNITS byte count".to_string())?;

    // ceil(bytes / unit_bytes)
    let units = bytes
        .checked_add(unit_bytes - 1)
        .ok_or_else(|| "overflow computing DUNITS units".to_string())?
        / unit_bytes;

    Ok(units)
}

pub(crate) fn inverse_permutation(n: usize, permutation: &[isize], inverse: &mut [isize]) {

    #[cfg(debug_assertions)]
    {
        for k in 0..n {
            inverse[k] = EMPTY;
        }
    }

    for k in 0..n {
        debug_assert!(permutation[k] >= 0 && permutation[k] < n as isize);
        inverse[permutation[k] as usize] = k as isize;
    }

    #[cfg(debug_assertions)]
    {
        for k in 0..n {
            debug_assert!(inverse[k] != EMPTY);
        }
    }
}