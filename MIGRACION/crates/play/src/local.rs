//! A server in the process (`docs/PLAN_AUTORITATIVO.md` fase 10): a game of one's own is a game
//! over a server like any other, only the server is here, on a thread of its own, and the way
//! there is a network in memory with no delay (`lunar_net::MemoryNet`): the prediction is always
//! right, and everything is done as it is done over the real network (one way to play). Others can
//! come in too over the real one (`LocalConfig::udp`): the game hosted from this machine, with
//! its player in it.
//!
//! The thread does what `luna-servidor` does with its two (the network and the game): takes in
//! who comes and goes and what each says, steps the game at 60 Hz (a few steps at once at most
//! after a slow one) and sends what each step says; when it is stopped it gives the game back
//! (`Local::stop`), to keep it.
use crate::{
    game::{Game, STEP},
    host::{Host, HostConfig},
    keep::{self, Keeper, Keeping},
};
use lunar_core::scenario::PlayerDef;
use lunar_net::{Addr, Client, Memory, MemoryNet, Server, ServerConfig, ServerEvent, Transport, Udp, now};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct LocalConfig {
    pub host: HostConfig,
    /// Others let in over the real network at this UDP port too (0: one the system picks).
    pub udp: Option<u16>,
    /// What the server is called, and how many may be in.
    pub name: String,
    pub max_players: usize,
    /// The build and the fingerprint of the data (`defs::fingerprint`): who comes over the real
    /// network must have the same.
    pub build: String,
    pub fingerprint: u32,
}

/// How the server's steps go (what the thread last counted, once a second).
#[derive(Clone, Copy, Debug, Default)]
pub struct LocalStats {
    pub steps: u64,
    /// Over the last second: the mean and the slowest step (ms), and the share of the second the
    /// thread was at work.
    pub mean_ms: f32,
    pub worst_ms: f32,
    pub busy: f32,
    pub players: usize,
    pub corrections: u64,
}

pub struct Local {
    net: MemoryNet,
    server: Addr,
    udp: Option<u16>,
    build: String,
    fingerprint: u32,
    quit: Arc<AtomicBool>,
    thread: Option<JoinHandle<Host>>,
    /// The key of the body that waits in the game taken up (`start_kept`): who plays it comes
    /// back to it (`Online::back`).
    pub back: Option<u64>,
    /// What the server says (who came and went, what it could not do).
    pub notes: Receiver<String>,
    pub stats: Arc<Mutex<LocalStats>>,
}

/// The way in for this machine's players (in memory) and, if asked, for the rest (UDP).
struct Both {
    mem: Memory,
    udp: Option<Udp>,
}

impl Transport for Both {
    fn send(&mut self, to: Addr, data: &[u8]) {
        match to {
            Addr::Mem(_) => self.mem.send(to, data),
            Addr::Udp(_) => {
                if let Some(u) = &mut self.udp {
                    u.send(to, data);
                }
            }
        }
    }
    fn recv(&mut self, buf: &mut [u8]) -> Option<(Addr, usize)> {
        self.mem.recv(buf).or_else(|| self.udp.as_mut()?.recv(buf))
    }
}

/// Steps taken at once at most to catch up after a slow one (past them the clock is set to now).
const CATCH_UP: u32 = 4;

impl Local {
    /// The server of `game` (as the scenario starts it), on its thread.
    pub fn start(game: Game, def: PlayerDef, config: LocalConfig) -> Result<Local, String> {
        let host = Host::new(game, def, config.host.clone());
        Local::start_host(host, config)
    }

    /// The game kept in `keeping` taken up (the newest whole one), or a new one `make` makes, kept
    /// as it goes and when it is stopped.
    pub fn start_kept(make: impl Fn() -> Result<Game, String>, def: PlayerDef, config: LocalConfig, mut keeping: Keeping) -> Result<Local, String> {
        let (says, notes) = channel();
        let mut say = |n: String| {
            let _ = says.send(n);
        };
        let host = match keep::take_up(&mut keeping, &make, def, &config.host, config.fingerprint, &mut say)? {
            Some(h) => h,
            None => Host::new(make()?, def, config.host.clone()),
        };
        let back = host.waiting_key();
        let mut local = Local::launch(host, config, Some(keeping), says, notes)?;
        local.back = back;
        Ok(local)
    }

    /// The same, with a game already made into a server (taken up from a game kept: `Host::load`).
    pub fn start_host(host: Host, config: LocalConfig) -> Result<Local, String> {
        let (says, notes) = channel();
        Local::launch(host, config, None, says, notes)
    }

    fn launch(host: Host, config: LocalConfig, keeping: Option<Keeping>, says: Sender<String>, notes: Receiver<String>) -> Result<Local, String> {
        let net = MemoryNet::new(0);
        let mem = net.endpoint();
        let server = mem.addr();
        let (udp, port) = match config.udp {
            Some(p) => {
                let u = Udp::bind(&format!("0.0.0.0:{p}")).map_err(|e| format!("no se puede abrir el puerto {p} (UDP): {e}"))?;
                let port = u.local().map_err(|e| e.to_string())?.port();
                (Some(u), Some(port))
            }
            None => (None, None),
        };
        let quit = Arc::new(AtomicBool::new(false));
        let stats = Arc::new(Mutex::new(LocalStats::default()));
        let (q, st, n) = (quit.clone(), stats.clone(), net.clone());
        let sc = ServerConfig { name: config.name.clone(), max_players: config.max_players, game: Some((config.build.clone(), config.fingerprint)), ..ServerConfig::default() };
        let keeper = keeping.map(|k| Keeper::new(k, config.fingerprint, now()));
        let way = Both { mem, udp };
        let thread = std::thread::Builder::new().name("servidor-local".to_string()).spawn(move || serve(host, Server::new(sc), way, n, q, st, says, keeper)).map_err(|e| e.to_string())?;
        Ok(Local { net, server, udp: port, build: config.build, fingerprint: config.fingerprint, quit, thread: Some(thread), back: None, notes, stats })
    }

    /// A connection for a player of this machine, as `name`.
    pub fn client(&self, name: &str) -> Client {
        Client::with_transport(Box::new(self.net.endpoint()), self.server, name, &self.build, self.fingerprint)
    }

    /// The UDP port others come in at, if they may.
    pub fn port(&self) -> Option<u16> {
        self.udp
    }

    /// The server stopped (it ends its step first): its game, to keep.
    pub fn stop(&mut self) -> Option<Host> {
        self.quit.store(true, Ordering::Relaxed);
        self.thread.take().and_then(|t| t.join().ok())
    }
}

impl Drop for Local {
    fn drop(&mut self) {
        self.stop();
    }
}

#[allow(clippy::too_many_arguments)]
fn serve(mut host: Host, mut server: Server, mut way: Both, net: MemoryNet, quit: Arc<AtomicBool>, stats: Arc<Mutex<LocalStats>>, says: Sender<String>, mut keeper: Option<Keeper>) -> Host {
    let mut say = |n: String| {
        let _ = says.send(n);
    };
    let mut next = now();
    let (mut second, mut work, mut worst, mut n) = (now(), 0.0f64, 0.0f64, 0u32);
    while !quit.load(Ordering::Relaxed) {
        let t = now();
        net.set_time(t);
        server.update(t, &mut way);
        for e in server.events() {
            match &e {
                ServerEvent::Joined { id, name, .. } => {
                    host.join(*id);
                    say(format!("Entra {name}"));
                }
                ServerEvent::Left { id, name, reason, .. } => {
                    host.leave(*id);
                    say(format!("Sale {name}: {reason}"));
                }
                ServerEvent::Refused { name, reason, .. } => say(format!("No se deja entrar a {name}: {reason}")),
                _ => {}
            }
        }
        for m in server.take_game() {
            host.take(m.from, &m.data);
        }
        let mut taken = 0;
        while now() >= next && taken < CATCH_UP {
            let began = Instant::now();
            host.step();
            host.send(|to, reliable, bytes| server.send_game(to, reliable, bytes));
            let ms = began.elapsed().as_secs_f64() * 1000.0;
            (work, worst, n) = (work + ms, worst.max(ms), n + 1);
            next += STEP;
            taken += 1;
        }
        if now() - next > 0.25 {
            next = now();
        }
        if taken > 0 {
            net.set_time(now());
            server.update(now(), &mut way);
        }
        if now() - second >= 1.0 {
            if let Ok(mut s) = stats.lock() {
                let span = now() - second;
                *s = LocalStats { steps: host.stats.steps, mean_ms: (work / f64::from(n.max(1))) as f32, worst_ms: worst as f32, busy: (work / 1000.0 / span) as f32, players: host.ids().count(), corrections: host.stats.corrections };
            }
            (second, work, worst, n) = (now(), 0.0, 0.0, 0);
        }
        if let Some(k) = &mut keeper {
            k.tick(&mut host, now(), &mut say);
        }
        let wait = next - now();
        if wait > 0.0 {
            std::thread::sleep(Duration::from_secs_f64(wait.min(0.001)));
        }
    }
    // (kept as it ends)
    if let Some(k) = &mut keeper {
        k.save(&mut host, true, &mut say);
    }
    host
}
