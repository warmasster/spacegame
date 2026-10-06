//! Connectivity: which live parts still hold together through live joints (union-find). The
//! runtime asks after every break which groups there are; any group not holding the anchor (or not
//! the heaviest) flies off as a structure of its own.

/// Reused between calls: no allocation once grown.
#[derive(Default)]
pub struct Groups {
    parent: Vec<u32>,
    /// Group of each part (`NONE` for dead parts), groups numbered 0.. in order of first part.
    pub label: Vec<u32>,
    pub count: usize,
}

pub const NONE: u32 = u32::MAX;

impl Groups {
    fn find(&mut self, mut a: u32) -> u32 {
        while self.parent[a as usize] != a {
            let up = self.parent[self.parent[a as usize] as usize];
            self.parent[a as usize] = up;
            a = up;
        }
        a
    }

    /// Group `n` parts (`alive(i)`) joined by `joints` (pairs of part indices, live ones only).
    pub fn build(&mut self, n: usize, alive: impl Fn(usize) -> bool, joints: impl Iterator<Item = (u32, u32)>) {
        self.parent.clear();
        self.parent.extend(0..n as u32);
        for (a, b) in joints {
            if !alive(a as usize) || !alive(b as usize) {
                continue;
            }
            let (ra, rb) = (self.find(a), self.find(b));
            if ra != rb {
                self.parent[ra.max(rb) as usize] = ra.min(rb);
            }
        }
        self.label.clear();
        self.label.resize(n, NONE);
        self.count = 0;
        for i in 0..n {
            if !alive(i) {
                continue;
            }
            let r = self.find(i as u32) as usize;
            if self.label[r] == NONE {
                self.label[r] = self.count as u32;
                self.count += 1;
            }
            self.label[i] = self.label[r];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_broken_chain_falls_in_two() {
        let mut g = Groups::default();
        let chain = [(0, 1), (1, 2), (2, 3), (3, 4)];
        g.build(5, |_| true, chain.iter().copied());
        assert_eq!(g.count, 1);
        // the joint 1-2 breaks
        g.build(5, |_| true, chain.iter().copied().filter(|j| *j != (1, 2)));
        assert_eq!(g.count, 2);
        assert_eq!(g.label, vec![0, 0, 1, 1, 1]);
        // part 3 is destroyed: 4 is on its own
        g.build(5, |i| i != 3, chain.iter().copied());
        assert_eq!(g.count, 2);
        assert_eq!(g.label[3], NONE);
        assert_ne!(g.label[4], g.label[0]);
    }
}
