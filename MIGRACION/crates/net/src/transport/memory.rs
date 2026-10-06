//! A network inside the process, for tests: endpoints of one `MemoryNet` send datagrams to each
//! other, and the net can lose, delay (so they arrive out of order) and duplicate them. Its
//! random numbers come from its seed and its clock is set by the test: a run repeats exactly.
use super::{Addr, Transport};
use std::sync::{Arc, Mutex, MutexGuard};

/// What the net does to the datagrams that cross it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Conditions {
    /// Chance (0..1) that a datagram is lost.
    pub loss: f32,
    /// Chance (0..1) that a datagram arrives twice.
    pub duplicate: f32,
    /// Seconds every datagram takes.
    pub delay: f64,
    /// Extra seconds, random between 0 and this, each datagram takes: with it they overtake each other.
    pub jitter: f64,
}

struct Flight {
    at: f64,
    order: u64,
    to: u32,
    from: u32,
    data: Vec<u8>,
}

struct Hub {
    rng: u64,
    now: f64,
    next: u32,
    order: u64,
    cond: Conditions,
    flights: Vec<Flight>,
    /// Buffers of delivered datagrams, to carry the next ones.
    spare: Vec<Vec<u8>>,
    /// Endpoints whose cable is cut: nothing leaves them, nothing reaches them.
    cut: Vec<u32>,
    sent: u64,
    lost: u64,
}

impl Hub {
    /// xorshift64*: enough for dice, and the same on every machine.
    fn dice(&mut self) -> f64 {
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        (self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
    fn launch(&mut self, from: u32, to: u32, data: &[u8]) {
        let at = self.now + self.cond.delay + self.dice() * self.cond.jitter;
        let mut buf = self.spare.pop().unwrap_or_default();
        buf.clear();
        buf.extend_from_slice(data);
        self.order += 1;
        self.flights.push(Flight { at, order: self.order, to, from, data: buf });
    }
}

/// The net itself: makes endpoints, sets the conditions and the time. Cloning it gives another handle to the same net.
#[derive(Clone)]
pub struct MemoryNet {
    hub: Arc<Mutex<Hub>>,
}

fn lock(hub: &Arc<Mutex<Hub>>) -> MutexGuard<'_, Hub> {
    hub.lock().unwrap_or_else(|e| e.into_inner())
}

impl MemoryNet {
    pub fn new(seed: u64) -> MemoryNet {
        // The seed is stirred (splitmix64) so that neighbouring seeds give unrelated runs; the dice need a state that is not zero.
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        let hub = Hub { rng: (z ^ (z >> 31)).max(1), now: 0.0, next: 1, order: 0, cond: Conditions::default(), flights: Vec::new(), spare: Vec::new(), cut: Vec::new(), sent: 0, lost: 0 };
        MemoryNet { hub: Arc::new(Mutex::new(hub)) }
    }
    /// A new endpoint with its own address.
    pub fn endpoint(&self) -> Memory {
        let mut h = lock(&self.hub);
        let addr = h.next;
        h.next += 1;
        Memory { hub: self.hub.clone(), addr }
    }
    /// Another endpoint at an address that already exists: a program started again on the same
    /// port. What is sent to the address goes to whichever of them asks first.
    pub fn endpoint_at(&self, addr: Addr) -> Option<Memory> {
        let Addr::Mem(addr) = addr else { return None };
        Some(Memory { hub: self.hub.clone(), addr })
    }
    pub fn conditions(&self, cond: Conditions) {
        lock(&self.hub).cond = cond;
    }
    /// The net's clock, in the same seconds the sessions are updated with: delayed datagrams arrive when it passes their time.
    pub fn set_time(&self, now: f64) {
        lock(&self.hub).now = now;
    }
    /// Cuts (or mends) the cable of an endpoint: a machine that vanishes without a word.
    pub fn cut(&self, addr: Addr, cut: bool) {
        let Addr::Mem(a) = addr else { return };
        let mut h = lock(&self.hub);
        h.cut.retain(|x| *x != a);
        if cut {
            h.cut.push(a);
        }
    }
    /// Puts a datagram on the wire by hand, as if `from` had sent it to `to` (whatever the conditions and the cuts).
    pub fn inject(&self, from: Addr, to: Addr, data: &[u8]) {
        if let (Addr::Mem(from), Addr::Mem(to)) = (from, to) {
            let mut h = lock(&self.hub);
            let at = h.now;
            h.order += 1;
            let order = h.order;
            h.flights.push(Flight { at, order, to, from, data: data.to_vec() });
        }
    }
    /// Datagrams sent through the net and how many of them it lost.
    pub fn counts(&self) -> (u64, u64) {
        let h = lock(&self.hub);
        (h.sent, h.lost)
    }
}

/// One endpoint of a `MemoryNet`.
pub struct Memory {
    hub: Arc<Mutex<Hub>>,
    addr: u32,
}

impl Memory {
    pub fn addr(&self) -> Addr {
        Addr::Mem(self.addr)
    }
}

impl Transport for Memory {
    fn send(&mut self, to: Addr, data: &[u8]) {
        let Addr::Mem(to) = to else { return };
        let mut h = lock(&self.hub);
        h.sent += 1;
        if h.cut.contains(&self.addr) || h.cut.contains(&to) || h.dice() < h.cond.loss as f64 {
            h.lost += 1;
            return;
        }
        h.launch(self.addr, to, data);
        if h.dice() < h.cond.duplicate as f64 {
            h.launch(self.addr, to, data);
        }
    }
    fn recv(&mut self, buf: &mut [u8]) -> Option<(Addr, usize)> {
        let mut h = lock(&self.hub);
        if h.cut.contains(&self.addr) {
            return None;
        }
        let now = h.now;
        // The earliest arrival for us that is due; ties in the order they were sent.
        let mut best: Option<usize> = None;
        for (i, f) in h.flights.iter().enumerate() {
            if f.to == self.addr && f.at <= now && best.is_none_or(|b| (f.at, f.order) < (h.flights[b].at, h.flights[b].order)) {
                best = Some(i);
            }
        }
        let f = h.flights.swap_remove(best?);
        let n = f.data.len().min(buf.len());
        buf[..n].copy_from_slice(&f.data[..n]);
        h.spare.push(f.data);
        Some((Addr::Mem(f.from), n))
    }
}
