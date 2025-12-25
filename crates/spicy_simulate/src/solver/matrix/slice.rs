

pub struct SpicySlice<T>([T]);

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