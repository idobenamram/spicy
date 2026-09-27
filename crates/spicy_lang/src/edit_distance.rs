//! The distance behind every "did you mean" suggestion.

/// Levenshtein distance, in characters: how many single-character insertions,
/// deletions and substitutions turn `a` into `b`. Used by every "did you mean" (unit
/// suffixes in the lexer, names in resolve). Resolve compares one name against every
/// name in scope, so this does one allocation per call: a common prefix and suffix are
/// skipped (they never change the distance; `net_1997q` against `net_1998` is `7q`
/// against `8`), and the table is one reused row.
pub(crate) fn edit_distance(a: &str, b: &str) -> usize {
    let pre: usize = a
        .chars()
        .zip(b.chars())
        .take_while(|(x, y)| x == y)
        .map(|(x, _)| x.len_utf8())
        .sum();
    let (a, b) = (&a[pre..], &b[pre..]);
    let suf: usize = a
        .chars()
        .rev()
        .zip(b.chars().rev())
        .take_while(|(x, y)| x == y)
        .map(|(x, _)| x.len_utf8())
        .sum();
    let (a, b) = (&a[..a.len() - suf], &b[..b.len() - suf]);
    let mut row: Vec<usize> = (0..=b.chars().count()).collect();
    for (i, ca) in a.chars().enumerate() {
        // `diag` is the previous row's value left of `row[j + 1]`.
        let mut diag = row[0];
        row[0] = i + 1;
        for (j, cb) in b.chars().enumerate() {
            let up = row[j + 1];
            row[j + 1] = (diag + usize::from(ca != cb)).min(up + 1).min(row[j] + 1);
            diag = up;
        }
    }
    row[row.len() - 1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_distance_skips_shared_ends() {
        assert_eq!(edit_distance("kitten", "sitting"), 3);
        assert_eq!(edit_distance("net_1997q", "net_1998"), 2);
        assert_eq!(edit_distance("", "abc"), 3);
        assert_eq!(edit_distance("same", "same"), 0);
        assert_eq!(edit_distance("aab", "ab"), 1);
        assert_eq!(edit_distance("µF", "uF"), 1);
        assert_eq!(edit_distance("°Cx", "°C"), 1);
    }
}
