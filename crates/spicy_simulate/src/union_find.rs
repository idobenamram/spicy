#[derive(Debug)]
pub struct UnionFind {
    pub parent: Vec<usize>,
    rank: Vec<usize>,
}

impl UnionFind {
    pub fn new(n: usize) -> Self {
        UnionFind {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }

    /// Find the representative (root) of the set containing `x`.
    /// Path compression flattens the structure.
    pub fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            let root = self.find(self.parent[x]);
            self.parent[x] = root; // path compression
        }
        self.parent[x]
    }

    /// Find the representative (root) of the set containing `x` without
    /// performing path compression. Useful when only an immutable reference
    /// is available to the union-find structure.
    pub fn find_no_compress(&self, mut x: usize) -> usize {
        while self.parent[x] != x {
            x = self.parent[x];
        }
        x
    }

    /// Union the sets containing `x` and `y`.
    pub fn union(&mut self, x: usize, y: usize) {
        let mut root_x = self.find(x);
        let mut root_y = self.find(y);

        if root_x == root_y {
            return;
        }

        // If either root is 0, make 0 the root.
        if root_x == 0 || root_y == 0 {
            let other = if root_x == 0 { root_y } else { root_x };
            self.parent[other] = 0;
            return;
        }


        // union by rank
        if self.rank[root_x] < self.rank[root_y] {
            std::mem::swap(&mut root_x, &mut root_y);
        }

        self.parent[root_y] = root_x;

        if self.rank[root_x] == self.rank[root_y] {
            self.rank[root_x] += 1;
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_union_find_prefers_root_zero() {
        let mut uf = UnionFind::new(3);
        uf.union(0, 2);
        assert_eq!(uf.find(0), uf.find(2));
        assert_eq!(uf.find(2), 0);

        let mut uf = UnionFind::new(3);
        uf.union(2, 0);
        assert_eq!(uf.find(0), uf.find(2));
        assert_eq!(uf.find(2), 0);
    }

    #[test]
    fn test_union_double_union() {
        let mut uf = UnionFind::new(4);
        uf.union(0, 3);
        uf.union(2, 3);

        assert_eq!(uf.find(2), uf.find(3));
        assert_eq!(uf.find(0), uf.find(3));
        assert_eq!(uf.find(2), 0);
    }

    #[test]
    fn test_union_triple_union() {
        let mut uf = UnionFind::new(4);
        uf.union(0, 1);
        uf.union(2, 3);
        uf.union(1, 2);

        assert_eq!(uf.find(2), uf.find(3));
        assert_eq!(uf.find(0), uf.find(3));
        assert_eq!(uf.find(2), 0);
    }
}