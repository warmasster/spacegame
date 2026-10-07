//! What the tests share: dice that always roll the same, and a little world of one server and
//! some clients on an in-memory network, all on one clock that the test advances.
#![allow(dead_code)]
use glam::{DVec3, Quat, Vec3};
use lunar_net::{Addr, Client, Conditions, Event, Frame, Memory, MemoryNet, PlayerState, RigidState, Server, ServerConfig, ServerEvent, Status};

/// xorshift64*: the same numbers on every run and every machine.
pub struct Dice(pub u64);

impl Dice {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// 0..1.
    pub fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    /// -1..1.
    pub fn signed(&mut self) -> f64 {
        self.unit() * 2.0 - 1.0
    }
    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    pub fn bytes(&mut self, out: &mut [u8]) {
        out.iter_mut().for_each(|b| *b = self.next() as u8);
    }
    pub fn quat(&mut self) -> Quat {
        Quat::from_xyzw(self.signed() as f32, self.signed() as f32, self.signed() as f32, self.signed() as f32).normalize()
    }
}

/// A point on the Moon's surface (its radius from the origin, no axis near zero: the usual
/// case, and the one that costs the most bytes), `east` metres along.
pub fn on_the_moon(east: f64) -> DVec3 {
    DVec3::new(1_002_000.0, 1_003_000.0, 1_004_091.0 + east)
}

/// The angle between two rotations, in f64 (an f32 cannot tell angles this small apart).
pub fn angle_between(a: Quat, b: Quat) -> f64 {
    let d = a.as_dquat().normalize().conjugate() * b.as_dquat().normalize();
    2.0 * d.xyz().length().atan2(d.w.abs())
}

pub fn walker(east: f64) -> PlayerState {
    PlayerState { pos: on_the_moon(east), yaw: 0.3, pitch: -0.1, vel: Vec3::new(0.0, 0.0, 1.5), flags: lunar_net::flag::GROUNDED, eye_h: 1.62, ..PlayerState::default() }
}

pub fn ship(id: u64, east: f64, joints: usize) -> RigidState {
    RigidState {
        id,
        body: 0,
        frame: Frame::World,
        pos: on_the_moon(east) + DVec3::Y * 20.0,
        rot: Quat::from_rotation_y(0.4),
        vel: Vec3::new(3.0, -1.0, 12.0),
        spin: Vec3::new(0.01, 0.2, 0.0),
        joints: (0..joints).map(|j| j as f32 * 0.05).collect(),
        resting: false,
    }
}

/// A crate lying in the hold of thing `ship`, where `at` says (its frame).
pub fn cargo(id: u64, ship: u64, at: Vec3) -> RigidState {
    RigidState { id, body: 0, frame: Frame::Aboard(ship), pos: at.as_dvec3(), rot: Quat::from_rotation_y(0.2), vel: Vec3::ZERO, spin: Vec3::ZERO, joints: Vec::new(), resting: false }
}

pub const BUILD: &str = "V36";
/// The number of the scenario every client of a test starts with.
pub const SCENARIO: u32 = 0x5ce0_a210;
/// The frame of the simulated clients.
pub const FRAME: f64 = 1.0 / 60.0;

pub struct World {
    pub net: MemoryNet,
    pub server: Server,
    pub link: Memory,
    pub addr: Addr,
    pub clients: Vec<Client>,
    /// The endpoint address of each client, to cut its cable.
    pub addrs: Vec<Addr>,
    pub now: f64,
    /// What the server said happened, kept for the test to look at.
    pub log: Vec<ServerEvent>,
}

impl World {
    pub fn new(seed: u64, config: ServerConfig) -> World {
        let net = MemoryNet::new(seed);
        let link = net.endpoint();
        let addr = link.addr();
        // The clock does not start at zero: nothing may depend on that.
        World { net, server: Server::new(config), link, addr, clients: Vec::new(), addrs: Vec::new(), now: 100.0, log: Vec::new() }
    }
    pub fn plain(seed: u64) -> World {
        World::new(seed, ServerConfig::default())
    }
    pub fn conditions(&self, c: Conditions) {
        self.net.conditions(c);
    }
    /// A new client says hello; returns its index.
    pub fn join_as(&mut self, name: &str, build: &str, scenario: u32) -> usize {
        let end = self.net.endpoint();
        self.addrs.push(end.addr());
        self.clients.push(Client::with_transport(Box::new(end), self.addr, name, build, scenario));
        self.clients.len() - 1
    }
    pub fn join(&mut self, name: &str) -> usize {
        self.join_as(name, BUILD, SCENARIO)
    }
    /// A client is in and has been told the past.
    pub fn settle(&mut self, i: usize) -> u32 {
        for _ in 0..600 {
            self.step();
            if self.clients[i].synced() {
                return self.clients[i].id().expect("an id");
            }
            if let Status::Failed(why) = self.clients[i].status() {
                panic!("client {i} failed: {why}");
            }
        }
        panic!("client {i} never got in");
    }
    /// One frame: the clock advances, the server and every client update.
    pub fn step(&mut self) {
        self.now += FRAME;
        self.net.set_time(self.now);
        self.server.update(self.now, &mut self.link);
        self.log.extend(self.server.events());
        for c in &mut self.clients {
            c.update(self.now);
        }
    }
    pub fn run(&mut self, seconds: f64) {
        for _ in 0..(seconds / FRAME).round() as usize {
            self.step();
        }
    }
    /// One frame in which `each` gives every client its states first, as the game would.
    pub fn step_with(&mut self, mut each: impl FnMut(usize, &mut Client, f64)) {
        let now = self.now + FRAME;
        for (i, c) in self.clients.iter_mut().enumerate() {
            each(i, c, now);
        }
        self.step();
    }
    pub fn events(&mut self, i: usize) -> Vec<Event> {
        self.clients[i].events().collect()
    }
    /// The machine of client `i` vanishes: no goodbye, nothing more in or out.
    pub fn vanish(&mut self, i: usize) {
        self.net.cut(self.addrs[i], true);
    }
}
