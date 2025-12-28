use std::ops::{Deref, DerefMut};

/// A thin, **zero-cost** wrapper around a slice that allows us to implement
/// unchecked indexing (`get_unchecked`) behind the standard `[]` operators.
///
/// This is a dynamically-sized type (DST), so you generally work with it behind
/// references: `&SpicySlice<T>` / `&mut SpicySlice<T>`.
#[repr(transparent)]
pub struct SpicySlice<T>(pub [T]);

impl<T> SpicySlice<T> {
    /// View an immutable slice as an immutable `SpicySlice`.
    #[inline(always)]
    pub fn from_slice(slice: &[T]) -> &Self {
        // SAFETY: `SpicySlice<T>` is `repr(transparent)` over `[T]`,
        // so the pointer + metadata (len) are identical.
        unsafe { &*(slice as *const [T] as *const Self) }
    }

    /// View a mutable slice as a mutable `SpicySlice`.
    #[inline(always)]
    pub fn from_mut_slice(slice: &mut [T]) -> &mut Self {
        // SAFETY: `SpicySlice<T>` is `repr(transparent)` over `[T]`,
        // so the pointer + metadata (len) are identical.
        unsafe { &mut *(slice as *mut [T] as *mut Self) }
    }
}


// impl<T> Deref for SpicySlice<T> {
//     type Target = [T];

//     #[inline(always)]
//     fn deref(&self) -> &Self::Target {
//         &self.0
//     }
// }

// impl<T> DerefMut for SpicySlice<T> {
//     #[inline(always)]
//     fn deref_mut(&mut self) -> &mut Self::Target {
//         &mut self.0
//     }
// }

impl<T> std::ops::Index<usize> for SpicySlice<T> {
    type Output = T;

    #[inline(always)]
    fn index(&self, index: usize) -> &Self::Output {
        // SAFETY: NEED FOR SPEED
        unsafe { self.0.get_unchecked(index) }
    }
}

impl<T> std::ops::IndexMut<usize> for SpicySlice<T> {

    #[inline(always)]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        // SAFETY: NEED FOR SPEED
        unsafe { self.0.get_unchecked_mut(index) }
    }
}