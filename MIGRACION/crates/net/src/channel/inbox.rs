//! Where a channel leaves the messages of the datagrams it reads: one flat buffer of bytes and a
//! list of where each message is in it. Owned by whoever reads (not by the channel), so reading a
//! message can touch other channels; cleared and filled again, it allocates nothing once warm.
#[derive(Default)]
pub struct Inbox {
    bytes: Vec<u8>,
    items: Vec<(bool, u32, u32)>,
}

impl Inbox {
    pub fn new() -> Inbox {
        Inbox::default()
    }
    pub fn clear(&mut self) {
        self.bytes.clear();
        self.items.clear();
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    /// Message `i`: whether it came reliably, and its bytes.
    pub fn get(&self, i: usize) -> (bool, &[u8]) {
        let (reliable, at, len) = self.items[i];
        (reliable, &self.bytes[at as usize..(at + len) as usize])
    }
    /// The messages in the order they must be handled.
    pub fn iter(&self) -> impl Iterator<Item = (bool, &[u8])> {
        (0..self.items.len()).map(|i| self.get(i))
    }
    pub(super) fn push(&mut self, reliable: bool, data: &[u8]) {
        self.items.push((reliable, self.bytes.len() as u32, data.len() as u32));
        self.bytes.extend_from_slice(data);
    }
}
