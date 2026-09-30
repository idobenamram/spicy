//! Typed indices: one `u32` newtype per kind of thing (a block, a net, a device), so an
//! index into one table can't be used for another. Shared by every crate that numbers
//! things (`spicy_model`, `spicy_circuit`), as rustc's `rustc_index` gives every
//! compiler crate its `newtype_index!`.

/// Defines a `u32` index newtype: `id!(/// A net. NetId);` gives `NetId::new(i)` and
/// `id.index()`.
#[macro_export]
macro_rules! id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(u32);

        impl $name {
            /// Panics past `u32::MAX`, which no design reaches.
            pub fn new(index: usize) -> Self {
                Self(u32::try_from(index).expect(concat!(stringify!($name), " out of range")))
            }

            pub fn index(self) -> usize {
                self.0 as usize
            }
        }
    };
}

#[cfg(test)]
mod tests {
    id!(
        /// A test index.
        TestId
    );

    #[test]
    fn an_index_round_trips() {
        assert_eq!(TestId::new(7).index(), 7);
        assert!(TestId::new(1) < TestId::new(2));
    }

    #[test]
    #[should_panic(expected = "TestId out of range")]
    fn past_u32_it_panics() {
        TestId::new(u32::MAX as usize + 1);
    }
}
