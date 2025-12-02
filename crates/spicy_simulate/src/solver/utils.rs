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