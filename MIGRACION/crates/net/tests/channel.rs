//! The channel over a network that loses, delays, duplicates and reorders: reliable messages
//! arrive once and in order, unreliable ones never arrive stale, long ones arrive whole, and a
//! peer that goes silent is noticed.
mod common;

use common::Dice;
use lunar_net::channel::{FRAGMENT, HEADER, KEEPALIVE, MAX_MESSAGE, MAX_UNRELIABLE, TAG};
use lunar_net::{Channel, Conditions, Inbox, MTU, Memory, MemoryNet, Transport};

const LEAD: u8 = 4;
const STEP: f64 = 1.0 / 60.0;

/// Two channels facing each other across an in-memory network.
struct Pair {
    net: MemoryNet,
    a: (Channel, Memory),
    b: (Channel, Memory),
    now: f64,
    buf: Vec<u8>,
    /// What each side received, in order: (reliable, bytes).
    got_a: Vec<(bool, Vec<u8>)>,
    got_b: Vec<(bool, Vec<u8>)>,
    inbox: Inbox,
    /// The largest datagram seen.
    largest: usize,
}

/// The keys of both sides of a handshake (`seed` makes their secrets).
fn keys(seed: u64) -> (lunar_net::seal::Keys, lunar_net::seal::Keys) {
    use lunar_net::seal::{KEY, Keys, Secret};
    let (a, b) = (Secret::from_bytes([seed as u8 | 1; KEY]), Secret::from_bytes([(seed >> 8) as u8 ^ 0xA5; KEY]));
    (Keys::agree(&a, &b.public, 0x5EED ^ seed, 77, true).unwrap(), Keys::agree(&b, &a.public, 0x5EED ^ seed, 77, false).unwrap())
}

impl Pair {
    fn new(seed: u64, cond: Conditions) -> Pair {
        let net = MemoryNet::new(seed);
        net.conditions(cond);
        let (ea, eb) = (net.endpoint(), net.endpoint());
        let now = 50.0;
        net.set_time(now);
        // (sealed, as every session's channel is: each side's keys, from a handshake of its own)
        let (ka, kb) = keys(seed);
        let (mut a, mut b) = (Channel::new(now, LEAD), Channel::new(now, LEAD));
        a.seal(ka);
        b.seal(kb);
        Pair { net, a: (a, ea), b: (b, eb), now, buf: vec![0; 2048], got_a: Vec::new(), got_b: Vec::new(), inbox: Inbox::new(), largest: 0 }
    }
    fn step(&mut self) {
        self.now += STEP;
        self.net.set_time(self.now);
        let (to_a, to_b) = (self.a.1.addr(), self.b.1.addr());
        for (side, got) in [(&mut self.a, &mut self.got_a), (&mut self.b, &mut self.got_b)] {
            while let Some((_, n)) = side.1.recv(&mut self.buf) {
                assert_eq!(self.buf[0], LEAD);
                self.largest = self.largest.max(n);
                self.inbox.clear();
                side.0.receive(&self.buf[1..n], self.now, &mut self.inbox).expect("a sound datagram");
                got.extend(self.inbox.iter().map(|(reliable, bytes)| (reliable, bytes.to_vec())));
            }
        }
        self.a.0.flush(self.now, to_b, &mut self.a.1);
        self.b.0.flush(self.now, to_a, &mut self.b.1);
    }
    fn run(&mut self, seconds: f64) {
        for _ in 0..(seconds / STEP) as usize {
            self.step();
        }
    }
}

const ROUGH: Conditions = Conditions { loss: 0.30, duplicate: 0.10, delay: 0.02, jitter: 0.12 };

fn numbered(i: u32, dice: &mut Dice) -> Vec<u8> {
    let mut m = i.to_le_bytes().to_vec();
    m.extend((0..dice.below(40)).map(|k| (i as u8).wrapping_add(k as u8)));
    m
}

#[test]
fn reliable_messages_arrive_once_and_in_order_through_a_rough_network() {
    let mut dice = Dice(3);
    let mut pair = Pair::new(101, ROUGH);
    let sent_ab: Vec<Vec<u8>> = (0..1000).map(|i| numbered(i, &mut dice)).collect();
    let sent_ba: Vec<Vec<u8>> = (0..300).map(|i| numbered(1_000_000 + i, &mut dice)).collect();
    // Some at once, the rest a few per frame, both ways at the same time.
    let (mut next_ab, mut next_ba) = (0, 0);
    for m in &sent_ab[..200] {
        assert!(pair.a.0.send_reliable(m));
        next_ab += 1;
    }
    for _ in 0..3000 {
        for _ in 0..3 {
            if next_ab < sent_ab.len() {
                assert!(pair.a.0.send_reliable(&sent_ab[next_ab]));
                next_ab += 1;
            }
        }
        if next_ba < sent_ba.len() {
            assert!(pair.b.0.send_reliable(&sent_ba[next_ba]));
            next_ba += 1;
        }
        pair.step();
        if pair.got_b.len() == sent_ab.len() && pair.got_a.len() == sent_ba.len() && pair.a.0.backlog() == 0 && pair.b.0.backlog() == 0 {
            break;
        }
    }
    let (sent, lost) = pair.net.counts();
    println!("1000 + 300 reliable messages through 30 % loss: {:.1} s, {sent} datagrams sent, {lost} lost, {} items resent by a, rtt seen by a {:.0} ms", pair.now - 50.0, pair.a.0.stats().resent, pair.a.0.rtt() * 1000.0);
    assert!(pair.got_b.iter().all(|(reliable, _)| *reliable));
    assert_eq!(pair.got_b.iter().map(|g| &g.1).collect::<Vec<_>>(), sent_ab.iter().collect::<Vec<_>>(), "a to b: once each, in order");
    assert_eq!(pair.got_a.iter().map(|g| &g.1).collect::<Vec<_>>(), sent_ba.iter().collect::<Vec<_>>(), "b to a: once each, in order");
    assert!(lost as f64 > sent as f64 * 0.2, "the network did lose");
    assert!(pair.a.0.stats().resent > 100 && pair.b.0.stats().repeated > 0, "and the channel had to work for it");
    assert!(pair.largest <= MTU);
    // Everything acked: nothing more goes out but keep-alives.
    assert_eq!((pair.a.0.backlog(), pair.b.0.backlog()), (0, 0));
}

#[test]
fn unreliable_messages_never_arrive_stale() {
    let mut pair = Pair::new(202, ROUGH);
    let total = 2000u32;
    for i in 0..total {
        assert!(pair.a.0.send_unreliable(&i.to_le_bytes()));
        // A reliable one now and then: its resends must not bring old unreliable ones back.
        if i % 50 == 0 {
            pair.a.0.send_reliable(b"event");
        }
        pair.step();
    }
    pair.run(2.0);
    let seen: Vec<u32> = pair.got_b.iter().filter(|g| !g.0).map(|g| u32::from_le_bytes(g.1[..4].try_into().expect("four bytes"))).collect();
    println!("{} of {total} unreliable messages arrived (30 % lost, the late ones dropped)", seen.len());
    assert!(seen.windows(2).all(|w| w[1] > w[0]), "each newer than the one before: none stale, none twice");
    assert!(seen.len() > total as usize / 3 && seen.len() < total as usize * 4 / 5);
    assert_eq!(pair.got_b.iter().filter(|g| g.0).count(), 40, "the reliable ones all arrived");
}

#[test]
fn on_a_clean_network_nothing_is_lost_or_resent() {
    let mut pair = Pair::new(303, Conditions::default());
    for i in 0..500u32 {
        pair.a.0.send_unreliable(&i.to_le_bytes());
        pair.a.0.send_reliable(&i.to_le_bytes());
        pair.step();
    }
    pair.run(0.5);
    assert_eq!(pair.got_b.len(), 1000);
    assert_eq!((pair.a.0.stats().resent, pair.b.0.stats().repeated), (0, 0));
    // One datagram a frame carried both messages: batching.
    assert!(pair.a.0.stats().sent_datagrams <= 501 + 2, "{}", pair.a.0.stats().sent_datagrams);
    // With the peer answering within a frame, the round trip measured is about one frame.
    assert!(pair.a.0.rtt() > 0.0 && pair.a.0.rtt() < 0.04, "{}", pair.a.0.rtt());
}

#[test]
fn a_long_reliable_message_arrives_whole() {
    let mut dice = Dice(5);
    for (seed, cond) in [(404, Conditions::default()), (405, ROUGH)] {
        let mut pair = Pair::new(seed, cond);
        let mut long = vec![0u8; 5000];
        dice.bytes(&mut long);
        let mut longest = vec![0u8; MAX_MESSAGE];
        dice.bytes(&mut longest);
        assert!(pair.a.0.send_reliable(b"before"));
        assert!(pair.a.0.send_reliable(&long));
        assert!(pair.a.0.send_reliable(b""));
        assert!(pair.a.0.send_reliable(&longest));
        assert!(pair.a.0.send_reliable(&long[..FRAGMENT]));
        assert!(pair.a.0.send_reliable(b"after"));
        assert!(!pair.a.0.send_reliable(&vec![0u8; MAX_MESSAGE + 1]), "over the cap is refused");
        for _ in 0..6000 {
            pair.step();
            if pair.got_b.len() == 6 {
                break;
            }
        }
        let got: Vec<&[u8]> = pair.got_b.iter().map(|g| &g.1[..]).collect();
        assert_eq!(got, [&b"before"[..], &long[..], &b""[..], &longest[..], &long[..FRAGMENT], &b"after"[..]]);
        assert!(pair.largest <= MTU, "pieces fit a datagram: {}", pair.largest);
    }
}

#[test]
fn unreliable_messages_have_a_size_limit_and_fill_datagrams() {
    let mut pair = Pair::new(505, Conditions::default());
    assert!(!pair.a.0.send_unreliable(&vec![1u8; MAX_UNRELIABLE + 1]));
    assert!(pair.a.0.send_unreliable(&vec![2u8; MAX_UNRELIABLE]));
    // Thirty messages of 100 bytes take three datagrams, not thirty.
    for i in 0..30u8 {
        assert!(pair.a.0.send_unreliable(&[i; 100]));
    }
    let before = pair.a.0.stats().sent_datagrams;
    pair.step();
    assert_eq!(pair.a.0.stats().sent_datagrams - before, 1 + 3);
    pair.step();
    assert_eq!(pair.got_b.len(), 31);
    assert_eq!(pair.largest, MTU);
    assert_eq!(pair.got_b[0].1.len(), MAX_UNRELIABLE);
    assert!(pair.got_b[1..].iter().enumerate().all(|(i, g)| g.1 == [i as u8; 100]));
}

#[test]
fn keep_alives_keep_a_quiet_peer_heard_and_a_dead_one_is_noticed() {
    let mut pair = Pair::new(606, Conditions::default());
    pair.run(10.0);
    // Nobody said anything for ten seconds, and yet each knows the other is there.
    assert!(pair.a.0.silence(pair.now) < KEEPALIVE + 0.1 && pair.b.0.silence(pair.now) < KEEPALIVE + 0.1);
    let quiet = pair.a.0.stats();
    assert!((9..=12).contains(&quiet.sent_datagrams), "about one keep-alive a second: {}", quiet.sent_datagrams);
    assert_eq!(quiet.sent_bytes, quiet.sent_datagrams * (HEADER + TAG) as u64, "and each is only a header (and its tag)");
    // b's cable is cut: a hears nothing more, and its reliable message stays unacked.
    pair.net.cut(pair.b.1.addr(), true);
    pair.a.0.send_reliable(b"anyone?");
    pair.run(6.0);
    assert!(pair.a.0.silence(pair.now) > 5.9, "{}", pair.a.0.silence(pair.now));
    assert_eq!(pair.a.0.backlog(), 1);
    assert!(pair.got_b.is_empty());
    // Mended: the message gets there after all.
    pair.net.cut(pair.b.1.addr(), false);
    pair.run(2.0);
    assert_eq!(pair.got_b.len(), 1);
    assert_eq!(pair.a.0.backlog(), 0);
}

#[test]
fn sequences_wrap_around_without_a_hitch() {
    // More than 65 536 datagrams and reliable messages each way: every counter wraps.
    let mut pair = Pair::new(707, Conditions { loss: 0.05, duplicate: 0.02, delay: 0.0, jitter: 0.03 });
    let total = 70_000u32;
    let (mut reliable, mut newest) = (0u32, None);
    for i in 0..total + 200 {
        if i < total {
            assert!(pair.a.0.send_reliable(&i.to_le_bytes()));
            assert!(pair.a.0.send_unreliable(&i.to_le_bytes()));
            assert!(pair.b.0.send_unreliable(&[8]));
        }
        pair.step();
        for (is_reliable, bytes) in pair.got_b.drain(..) {
            let n = u32::from_le_bytes(bytes[..4].try_into().expect("four bytes"));
            if is_reliable {
                assert_eq!(n, reliable, "in order, none missing, none twice");
                reliable += 1;
            } else {
                assert!(newest.is_none_or(|last| n > last), "never stale");
                newest = Some(n);
            }
        }
        pair.got_a.clear();
    }
    assert_eq!(reliable, total);
    assert!(newest.is_some_and(|n| n > total - 20));
    assert!(pair.a.0.stats().sent_datagrams > 70_000 && pair.b.0.stats().sent_datagrams > 70_000);
}

#[test]
fn the_same_seed_gives_the_same_run() {
    let run = |seed| {
        let mut pair = Pair::new(seed, ROUGH);
        for i in 0..300u32 {
            pair.a.0.send_reliable(&i.to_le_bytes());
            pair.a.0.send_unreliable(&i.to_le_bytes());
            pair.step();
        }
        pair.run(3.0);
        (pair.net.counts(), pair.a.0.stats().resent, pair.got_b.len())
    };
    assert_eq!(run(808), run(808));
    assert_ne!(run(808), run(809));
}

#[test]
fn a_datagram_not_sealed_with_the_key_is_dropped_unread() {
    // whoever does not know the session's keys: a datagram of a channel of their own (another
    // handshake's keys, or none), a real one with a bit changed, one cut short, a real one said
    // again. None of them gets in, none changes what the channel had, and the real ones go on
    let mut pair = Pair::new(707, Conditions::default());
    pair.a.0.send_reliable(b"uno");
    pair.step();
    pair.step();
    let mut stranger = Channel::new(pair.now, LEAD);
    stranger.seal(keys(1).0);
    stranger.send_reliable(b"falso");
    let mut unsigned = Channel::new(pair.now, LEAD);
    unsigned.send_reliable(b"sin firma");
    let to = pair.b.1.addr();
    let mut caught = Vec::new();
    {
        let mut tap = Tap(&mut caught);
        stranger.flush(pair.now, to, &mut tap);
        unsigned.flush(pair.now, to, &mut tap);
    }
    let mut inbox = Inbox::new();
    for d in &caught {
        assert_eq!(pair.b.0.receive(&d[1..], pair.now, &mut inbox), Err(lunar_net::ChannelError::Forged));
        let mut flipped = d.clone();
        let at = flipped.len() / 2;
        flipped[at] ^= 1;
        assert_eq!(pair.b.0.receive(&flipped[1..], pair.now, &mut inbox), Err(lunar_net::ChannelError::Forged));
        assert_eq!(pair.b.0.receive(&d[1..d.len() / 3], pair.now, &mut inbox), Err(lunar_net::ChannelError::Forged));
    }
    assert!(inbox.iter().next().is_none(), "nothing of theirs got in");
    pair.a.0.send_reliable(b"dos");
    pair.step();
    pair.step();
    let got: Vec<&[u8]> = pair.got_b.iter().map(|g| &g.1[..]).collect();
    assert_eq!(got, [&b"uno"[..], &b"dos"[..]]);
    // (what goes is not there to be read; and a real one said again is not taken again)
    assert!(!caught[0].windows(5).any(|w| w == b"falso"), "a sealed datagram shows what it carries");
    let mut real = Vec::new();
    pair.a.0.send_reliable(b"secreto");
    pair.a.0.flush(pair.now, to, &mut Tap(&mut real));
    assert!(!real[0].windows(7).any(|w| w == b"secreto"));
    assert_eq!(pair.b.0.receive(&real[0][1..], pair.now, &mut inbox), Ok(()));
    assert!(inbox.iter().any(|(_, m)| m.ends_with(b"secreto")));
    inbox.clear();
    assert_eq!(pair.b.0.receive(&real[0][1..], pair.now, &mut inbox), Ok(()));
    assert!(inbox.iter().next().is_none(), "said again, taken again");
}

/// A transport that keeps what is sent through it.
struct Tap<'a>(&'a mut Vec<Vec<u8>>);

impl lunar_net::Transport for Tap<'_> {
    fn send(&mut self, _: lunar_net::Addr, data: &[u8]) {
        self.0.push(data.to_vec());
    }
    fn recv(&mut self, _: &mut [u8]) -> Option<(lunar_net::Addr, usize)> {
        None
    }
}
