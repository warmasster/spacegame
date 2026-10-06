//! The black box: what happened to a ship, in order, with when and how bad. Append-only (a ring of
//! the last entries); it travels with the ship and can be read from its wreck.

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub t: f64,
    pub text: String,
    /// 0 note, 1 caution, 2 warning.
    pub level: u8,
}

pub struct BlackBox {
    pub entries: std::collections::VecDeque<Entry>,
    cap: usize,
    /// Total ever logged (the ring forgets the oldest).
    pub count: u64,
}

impl BlackBox {
    pub fn new(cap: usize) -> BlackBox {
        BlackBox { entries: std::collections::VecDeque::with_capacity(cap.min(256)), cap, count: 0 }
    }

    pub fn log(&mut self, t: f64, text: &str, level: u8) {
        if self.entries.len() == self.cap {
            self.entries.pop_front();
        }
        self.entries.push_back(Entry { t, text: text.to_string(), level });
        self.count += 1;
    }
}
