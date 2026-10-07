//! The game, simulated by the server (protocol 2, `docs/PLAN_AUTORITATIVO.md` §3.9): the truth
//! (`lunar_play::host::Host`) stepped every 1/60 s on a thread of its own, the network on the main
//! one. They speak through two channels: what came from each player (and who came and went), and
//! what goes out to each; the buffers of what goes out come back to be used again, so nothing new
//! is made for each message once it runs.
use lunar_net::now;
use lunar_play::{
    defs::Defs,
    game::{Game, STEP},
    host::{Host, HostConfig},
};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// To the game: who came and went, and what each said.
pub enum In {
    Join(u32),
    Leave(u32),
    Game(u32, Vec<u8>),
    Quit,
}

/// From the game: what goes to a player (reliably or not), and what the server says.
pub enum Out {
    Send(u32, bool, Vec<u8>),
    Note(String),
}

pub struct Sim {
    pub to: Sender<In>,
    pub from: Receiver<Out>,
    /// The buffers of what went out, back to the game.
    pub spare: Sender<Vec<u8>>,
    thread: Option<JoinHandle<()>>,
}

/// Steps taken at once at most to catch up after a slow one; past that the game's clock is set to
/// now (the server was stopped, or is too slow for its game: said).
const CATCH_UP: u32 = 4;
/// How often the game says how it goes (s).
const REPORT_EVERY: f64 = 30.0;

/// Where the game's data is: what the settings say, or beside the program (`assets`, or the
/// `assets` of the newest version folder next to it), or where it was built.
pub fn find_data(home: &Path, said: Option<&str>) -> Result<PathBuf, String> {
    if let Some(dir) = said {
        let p = PathBuf::from(dir);
        return if p.join("defs").is_dir() { Ok(p) } else { Err(format!("«datos»: en {} no están los datos del juego (falta la carpeta defs)", p.display())) };
    }
    let mut tried = vec![home.join("assets"), home.join("../assets")];
    // (a version folder beside it: SELENE_V41/assets, the newest)
    if let Ok(rd) = std::fs::read_dir(home.join("..")) {
        let mut versions: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.join("assets/defs").is_dir()).collect();
        versions.sort();
        tried.extend(versions.into_iter().rev().map(|p| p.join("assets")));
    }
    tried.push(lunar_play::root().join("assets"));
    tried.into_iter().find(|p| p.join("defs").is_dir()).ok_or_else(|| "no encuentro los datos del juego (una carpeta assets con defs dentro) junto al programa: dime dónde con «datos» en servidor.jsonc o con --datos".to_string())
}

/// The game of `data` (its `defs`), started on a thread of its own.
pub fn start(defs: Defs, data: &Path, cheats: bool) -> Result<Sim, String> {
    // (the server draws nothing: room for a few particles, for what the effects keep count of)
    let game = Game::new_apart(&defs, &data.join("defs"), 256, |_| true)?;
    let host = Host::new(game, defs.scenario.player, HostConfig { cheats, ..HostConfig::default() });
    let (to, inbox) = channel();
    let (outbox, from) = channel();
    let (spare, spares) = channel();
    let thread = std::thread::Builder::new().name("partida".to_string()).spawn(move || run(host, inbox, outbox, spares)).map_err(|e| e.to_string())?;
    Ok(Sim { to, from, spare, thread: Some(thread) })
}

impl Sim {
    /// The game stopped (it ends its step first).
    pub fn stop(&mut self) {
        let _ = self.to.send(In::Quit);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn run(mut host: Host, inbox: Receiver<In>, outbox: Sender<Out>, spares: Receiver<Vec<u8>>) {
    let mut next = now();
    let mut times: Vec<f32> = Vec::with_capacity((REPORT_EVERY / STEP) as usize + 8);
    let mut reported = now();
    let mut stats = host.stats;
    loop {
        loop {
            match inbox.try_recv() {
                Ok(In::Join(id)) => host.join(id),
                Ok(In::Leave(id)) => host.leave(id),
                Ok(In::Game(id, data)) => host.take(id, &data),
                Ok(In::Quit) | Err(TryRecvError::Disconnected) => return,
                Err(TryRecvError::Empty) => break,
            }
        }
        let t = now();
        if t < next {
            std::thread::sleep(Duration::from_secs_f64((next - t).min(0.002)));
            continue;
        }
        let mut taken = 0;
        while now() >= next && taken < CATCH_UP {
            let began = Instant::now();
            host.step();
            host.send(|to, reliable, bytes| {
                let mut b = spares.try_recv().unwrap_or_default();
                b.clear();
                b.extend_from_slice(bytes);
                outbox.send(Out::Send(to, reliable, b)).is_ok()
            });
            times.push(began.elapsed().as_secs_f32() * 1000.0);
            next += STEP;
            taken += 1;
        }
        if now() - next > 0.25 {
            let late = now() - next;
            next = now();
            let _ = outbox.send(Out::Note(format!("La partida iba {:.0} ms tarde: este equipo no llega a simularla a tiempo (o el programa estuvo parado).", late * 1000.0)));
        }
        if now() - reported >= REPORT_EVERY && !times.is_empty() {
            let _ = outbox.send(Out::Note(report(&mut times, &stats, &host)));
            (reported, stats) = (now(), host.stats);
            times.clear();
        }
    }
}

/// How the game went since the last report: how long its steps took, what was put right.
fn report(times: &mut [f32], was: &lunar_play::host::HostStats, host: &Host) -> String {
    times.sort_by(f32::total_cmp);
    let at = |q: f32| times[((times.len() - 1) as f32 * q) as usize];
    let s = host.stats;
    format!(
        "Partida: paso {} · mediana {:.2} ms, p95 {:.2} ms, máx {:.2} ms · {} jugadores · correcciones {} · comandos adivinados {} · instantáneas {:.1} kB/s · sucesos {:.1} kB/s",
        s.steps,
        at(0.5),
        at(0.95),
        at(1.0),
        host.ids().count(),
        s.corrections - was.corrections,
        s.guessed - was.guessed,
        (s.snap_bytes - was.snap_bytes) as f64 / 1000.0 / REPORT_EVERY,
        (s.event_bytes - was.event_bytes) as f64 / 1000.0 / REPORT_EVERY,
    )
    .replace('.', ",")
}
