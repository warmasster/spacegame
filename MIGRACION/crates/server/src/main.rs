//! The dedicated server. It has the game (`sim`): it simulates it, decides everything and tells
//! each player's game what it needs (`docs/PLAN_AUTORITATIVO.md`); the network is
//! `lunar_net::Server` on the main thread, the game is `lunar_play::host` on one of its own, and
//! it is kept on disk now and then (`keep`).
//! - `config`: `servidor.jsonc` and the flags;
//! - `console`: the orders typed while it runs;
//! - `journal`: what it says, to the console and to `servidor.log`;
//! - `sim`: the game, on its thread;
//! (its two slots on disk: `lunar_play::keep`).
mod config;
mod console;
mod journal;
mod sim;

use config::Config;
use console::Command;
use journal::Journal;
use lunar_net::{Server, ServerConfig, ServerEvent, ServerStats, Udp, now};
use std::net::UdpSocket;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

/// Seconds between two status lines (when there is anyone to count).
const STATUS_EVERY: f64 = 30.0;
/// With players the loop sleeps this little between rounds: what each step says goes out as soon
/// as it is said. With nobody, a hello can wait this long.
const GAME_SLEEP: Duration = Duration::from_millis(1);
const IDLE_SLEEP: Duration = Duration::from_millis(50);

/// The folder the program is in: its settings and its log live beside it.
fn home() -> PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)).unwrap_or_else(|| PathBuf::from("."))
}

/// The address the others on this network reach this machine at: the one it would use to go
/// out. Nothing is sent to find it out (a UDP socket only notes where it would send).
fn lan_address() -> Option<std::net::IpAddr> {
    let probe = UdpSocket::bind("0.0.0.0:0").ok()?;
    probe.connect("192.0.2.1:9").ok()?;
    probe.local_addr().ok().map(|a| a.ip()).filter(|ip| !ip.is_unspecified() && !ip.is_loopback())
}

/// 1234.5 as "1,2 kB/s": Spanish decimals.
fn rate(bytes: u64, seconds: f64) -> String {
    let per = bytes as f64 / seconds.max(0.001);
    if per < 1000.0 { format!("{per:.0} B/s") } else { format!("{:.1} kB/s", per / 1000.0).replace('.', ",") }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn describe(event: &ServerEvent) -> String {
    match event {
        ServerEvent::Joined { id, name, addr, players } => format!("Entra {name} (jugador {id}, desde {addr}). Ahora hay {}.", plural(*players, "jugador", "jugadores")),
        ServerEvent::Left { id, name, reason, players } => {
            let remain = match players {
                0 => "No queda nadie".to_string(),
                1 => "Queda 1 jugador".to_string(),
                n => format!("Quedan {n} jugadores"),
            };
            format!("Sale {name} (jugador {id}): {reason}. {remain}.")
        }
        ServerEvent::Refused { addr, name, reason } => format!("No se deja entrar a {name} (desde {addr}): {reason}."),
        ServerEvent::Moved { id, name, addr } => format!("{name} (jugador {id}) sigue desde otra dirección: {addr}."),
        ServerEvent::Chat { name, text, .. } => format!("<{name}> {text}"),
        ServerEvent::Empty => "El servidor se queda vacío (la partida sigue).".to_string(),
    }
}

fn status(server: &Server, since: &ServerStats, seconds: f64) -> String {
    let s = server.stats();
    format!("{} · entra {} · sale {}", plural(server.player_count(), "jugador", "jugadores"), rate(s.bytes_in - since.bytes_in, seconds), rate(s.bytes_out - since.bytes_out, seconds))
}

fn main() -> ExitCode {
    let dir = home();
    let (config, notes) = match Config::load(&dir, std::env::args().skip(1)) {
        Ok(Some(loaded)) => loaded,
        Ok(None) => {
            println!("{}", config::USAGE);
            return ExitCode::SUCCESS;
        }
        Err(problem) => {
            eprintln!("{problem}\n{}", config::USAGE);
            return ExitCode::FAILURE;
        }
    };
    let mut journal = Journal::open(&dir);
    // the game first (its data read, its scenario set, or the game kept taken up): it takes a moment
    let data = match sim::find_data(&dir, config.datos.as_deref()) {
        Ok(d) => d,
        Err(e) => {
            journal.say(&format!("No se puede simular la partida: {e}."));
            return ExitCode::FAILURE;
        }
    };
    journal.say(&format!("Cargando la partida de {}…", data.display()));
    let defs = match lunar_play::defs::Defs::load(&data.join("defs")) {
        Ok(d) => d,
        Err(e) => {
            journal.say(&format!("Los datos del juego tienen un problema: {}: {}", e.file, e.message));
            return ExitCode::FAILURE;
        }
    };
    let (build, fingerprint) = (lunar_play::net::BUILD.to_string(), defs.fingerprint);
    let keeping = config.partida.as_ref().map(|p| {
        let p = PathBuf::from(p);
        let base = if p.is_absolute() { p } else { dir.join(p) };
        lunar_play::keep::Keeping { slots: lunar_play::keep::Slots::new(&base), every: config.guardar_cada, fresh: config.nueva }
    });
    let mut game = match sim::start(defs, &data, config.trucos, keeping) {
        Ok(s) => s,
        Err(e) => {
            journal.say(&format!("No se puede montar la partida: {e}"));
            return ExitCode::FAILURE;
        }
    };
    let mut socket = match Udp::bind(&format!("0.0.0.0:{}", config.puerto)) {
        Ok(s) => s,
        Err(e) => {
            journal.say(&format!("No se puede abrir el puerto {} (UDP): {e}. ¿Hay ya otro servidor en marcha en este puerto?", config.puerto));
            return ExitCode::FAILURE;
        }
    };
    let mut server = Server::new(ServerConfig { name: config.nombre.clone(), max_players: config.max_jugadores, timeout: config.espera, game: Some((build.clone(), fingerprint)) });
    journal.say(&format!(
        "Servidor «{}» en marcha en el puerto {} (UDP): hasta {}; la partida la simula él, 60 pasos por segundo (juego {build}, datos {fingerprint:08x}).",
        config.nombre,
        config.puerto,
        plural(config.max_jugadores, "jugador", "jugadores")
    ));
    match lan_address() {
        Some(ip) => journal.say(&format!("En esta red se entra con: --servidor {ip}:{}   (en esta misma máquina: --servidor 127.0.0.1:{})", config.puerto, config.puerto)),
        None => journal.say(&format!("En esta misma máquina se entra con: --servidor 127.0.0.1:{}", config.puerto)),
    }
    notes.iter().for_each(|n| journal.say(n));
    // (Ctrl+C, and a polite kill on Linux: as «salir», the game kept first. Closing the window
    // Windows gives no time for it)
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let stop = stop.clone();
        if let Err(e) = ctrlc::set_handler(move || stop.store(true, std::sync::atomic::Ordering::SeqCst)) {
            journal.say(&format!("No se oye Ctrl+C ({e}): para pararlo guardando, «salir»."));
        }
    }
    journal.say(&format!("{}. Para pararlo: «salir» o Ctrl+C (guardan la partida; cerrar la ventana, no).", console::HELP));

    let orders = console::listen();
    let (mut counted_at, mut counted) = (now(), server.stats());
    // what goes to a player through a channel not through yet (in order, before anything new)
    let mut waiting: Vec<(u32, Vec<u8>)> = Vec::new();
    loop {
        let t = now();
        server.update(t, &mut socket);
        for e in server.events() {
            journal.say(&describe(&e));
            match e {
                ServerEvent::Joined { id, .. } => {
                    let _ = game.to.send(sim::In::Join(id));
                }
                ServerEvent::Left { id, .. } => {
                    let _ = game.to.send(sim::In::Leave(id));
                    waiting.retain(|w| w.0 != id);
                }
                _ => {}
            }
        }
        for m in server.take_game() {
            let _ = game.to.send(sim::In::Game(m.from, m.data));
        }
        // what was said before goes first
        let mut k = 0;
        while k < waiting.len() {
            let id = waiting[k].0;
            if waiting[..k].iter().all(|w| w.0 != id) && server.send_game(id, true, &waiting[k].1) {
                let (_, b) = waiting.remove(k);
                let _ = game.spare.send(b);
            } else {
                k += 1;
            }
        }
        while let Ok(out) = game.from.try_recv() {
            match out {
                sim::Out::Send(to, true, b) => {
                    if waiting.iter().any(|w| w.0 == to) || !server.send_game(to, true, &b) {
                        waiting.push((to, b));
                    } else {
                        let _ = game.spare.send(b);
                    }
                }
                sim::Out::Send(to, false, b) => {
                    server.send_game(to, false, &b);
                    let _ = game.spare.send(b);
                }
                sim::Out::Note(n) => journal.say(&n),
            }
        }
        // (sent now, not at the next round)
        server.update(now(), &mut socket);
        let mut quit = stop.load(std::sync::atomic::Ordering::SeqCst);
        if quit {
            journal.say("Ctrl+C: se para el servidor.");
        }
        while let Ok(line) = orders.try_recv() {
            match console::parse(&line) {
                Command::Players if server.player_count() == 0 => journal.say("No hay nadie conectado."),
                Command::Players => {
                    let lines: Vec<String> = server.players().map(|p| format!("  jugador {}: {} · {} · {:.0} ms", p.id, p.name, p.addr, p.ping_ms)).collect();
                    journal.say(&format!("{}:", plural(lines.len(), "jugador", "jugadores")));
                    lines.iter().for_each(|l| journal.say(l));
                }
                Command::Kick(id, reason) => {
                    if !server.kick(id, &reason) {
                        journal.say(&format!("No hay ningún jugador {id}. Mira los que hay con «jugadores»."));
                    }
                }
                Command::Say(text) => {
                    server.say(&text);
                    journal.say(&format!("<servidor> {text}"));
                }
                Command::Quit => quit = true,
                Command::Help => journal.say(console::HELP),
                Command::Nothing => {}
                Command::Wrong(how) => journal.say(&how),
            }
        }
        if quit {
            // (it keeps the game before it ends: what it says of that, said)
            game.stop();
            while let Ok(out) = game.from.try_recv() {
                if let sim::Out::Note(n) = out {
                    journal.say(&n);
                }
            }
            server.close(&mut socket);
            server.events().for_each(|e| journal.say(&describe(&e)));
            journal.say("Servidor parado.");
            return ExitCode::SUCCESS;
        }
        if t - counted_at >= STATUS_EVERY {
            if server.player_count() > 0 {
                journal.say(&status(&server, &counted, t - counted_at));
            }
            (counted_at, counted) = (t, server.stats());
        }
        // Never spinning: with nobody connected a hello can wait a moment; with players, a millisecond.
        std::thread::sleep(if server.player_count() == 0 { IDLE_SLEEP } else { GAME_SLEEP });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_and_counts_read_well() {
        assert_eq!(rate(15_000, 30.0), "500 B/s");
        assert_eq!(rate(126_000, 30.0), "4,2 kB/s");
        assert_eq!(rate(0, 30.0), "0 B/s");
        assert_eq!(plural(1, "jugador", "jugadores"), "1 jugador");
        assert_eq!(plural(0, "jugador", "jugadores"), "0 jugadores");
    }

    #[test]
    fn events_are_told_in_plain_words() {
        let server = Server::new(ServerConfig::default());
        let addr = lunar_net::Addr::Udp("192.168.1.20:50123".parse().expect("an address"));
        assert_eq!(describe(&ServerEvent::Joined { id: 2, name: "Ana".to_string(), addr, players: 2 }), "Entra Ana (jugador 2, desde 192.168.1.20:50123). Ahora hay 2 jugadores.");
        assert_eq!(describe(&ServerEvent::Joined { id: 1, name: "Ana".to_string(), addr, players: 1 }), "Entra Ana (jugador 1, desde 192.168.1.20:50123). Ahora hay 1 jugador.");
        assert_eq!(describe(&ServerEvent::Left { id: 2, name: "Ana".to_string(), reason: "se ha ido".to_string(), players: 0 }), "Sale Ana (jugador 2): se ha ido. No queda nadie.");
        assert_eq!(describe(&ServerEvent::Left { id: 2, name: "Ana".to_string(), reason: "dejó de dar señal".to_string(), players: 3 }), "Sale Ana (jugador 2): dejó de dar señal. Quedan 3 jugadores.");
        let full = ServerEvent::Refused { addr, name: "Luis".to_string(), reason: "el servidor está lleno (16 de 16 jugadores)".to_string() };
        assert_eq!(describe(&full), "No se deja entrar a Luis (desde 192.168.1.20:50123): el servidor está lleno (16 de 16 jugadores).");
        assert_eq!(describe(&ServerEvent::Empty), "El servidor se queda vacío (la partida sigue).");
        assert_eq!(status(&server, &ServerStats::default(), 30.0), "0 jugadores · entra 0 B/s · sale 0 B/s");
    }
}
