//! The reliable messages a channel receives: each is handed over once and in the order it was
//! sent. One that comes early waits in its slot; the pieces of a long one are joined.
use super::inbox::Inbox;
use super::{ChannelError, MAX_MESSAGE, WINDOW};

#[derive(Default)]
struct Slot {
    full: bool,
    more: bool,
    data: Vec<u8>,
}

pub(super) struct Incoming {
    /// The id we hand over next.
    next: u16,
    slots: Vec<Slot>,
    /// The pieces so far of a long message.
    assembly: Vec<u8>,
}

impl Incoming {
    pub fn new() -> Incoming {
        Incoming { next: 0, slots: (0..WINDOW).map(|_| Slot::default()).collect(), assembly: Vec::new() }
    }
    /// A reliable item arrived (maybe again, maybe early).
    pub fn take(&mut self, id: u16, more: bool, data: &[u8], inbox: &mut Inbox) -> Result<(), ChannelError> {
        let ahead = id.wrapping_sub(self.next);
        if ahead >= 0x8000 {
            return Ok(()); // Handed over already: this is a resend that crossed with our ack.
        }
        if ahead >= WINDOW {
            // A sender that keeps to the window cannot get here; one that does not would leave a hole for ever.
            return Err(ChannelError::Window);
        }
        if ahead > 0 {
            let slot = &mut self.slots[(id % WINDOW) as usize];
            if !slot.full {
                slot.full = true;
                slot.more = more;
                slot.data.clear();
                slot.data.extend_from_slice(data);
            }
            return Ok(());
        }
        self.hand(more, data, inbox)?;
        // Whatever was waiting right behind it.
        loop {
            let at = (self.next % WINDOW) as usize;
            if !self.slots[at].full {
                return Ok(());
            }
            let data = std::mem::take(&mut self.slots[at].data);
            let more = self.slots[at].more;
            self.slots[at].full = false;
            let done = self.hand(more, &data, inbox);
            self.slots[at].data = data;
            done?;
        }
    }
    fn hand(&mut self, more: bool, data: &[u8], inbox: &mut Inbox) -> Result<(), ChannelError> {
        self.next = self.next.wrapping_add(1);
        if self.assembly.len() + data.len() > MAX_MESSAGE {
            return Err(ChannelError::TooLong);
        }
        if more {
            self.assembly.extend_from_slice(data);
        } else if self.assembly.is_empty() {
            inbox.push(true, data);
        } else {
            self.assembly.extend_from_slice(data);
            inbox.push(true, &self.assembly);
            self.assembly.clear();
        }
        Ok(())
    }
}
