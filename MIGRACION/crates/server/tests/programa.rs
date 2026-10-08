//! The program itself, from outside: started as a person would start it (on a free port of this
//! machine), joined by two real clients over UDP, given orders through its console, and stopped.
//! What it prints is what is checked.
use lunar_net::{Client, Event, Status, now};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

struct Program {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    /// Everything it has printed so far, without the time at the start of each line.
    said: Vec<String>,
}

impl Program {
    fn start(args: &[&str]) -> Program {
        let mut child = Command::new(env!("CARGO_BIN_EXE_luna-servidor")).args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("the program starts");
        let (stdin, stdout) = (child.stdin.take().expect("its keyboard"), child.stdout.take().expect("its screen"));
        let (tx, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Program { child, stdin, lines, said: Vec::new() }
    }
    fn order(&mut self, line: &str) {
        writeln!(self.stdin, "{line}").expect("it listens");
    }
    /// Waits (up to 5 s) for a line that contains `what`; `each` runs meanwhile (to keep clients alive).
    fn expect(&mut self, what: &str, each: impl FnMut()) -> String {
        self.expect_within(what, 5.0, each)
    }
    fn expect_within(&mut self, what: &str, seconds: f64, mut each: impl FnMut()) -> String {
        let until = Instant::now() + Duration::from_secs_f64(seconds);
        let mut from = 0;
        loop {
            while let Ok(line) = self.lines.try_recv() {
                // "[12:34:56 UTC] text": the text.
                let text = line.split_once("] ").map_or(line.as_str(), |(_, t)| t).to_string();
                assert!(line.starts_with('[') && line.contains(" UTC] "), "every line says when: {line}");
                self.said.push(text);
            }
            if let Some(found) = self.said[from..].iter().find(|l| l.contains(what)) {
                return found.clone();
            }
            from = self.said.len();
            assert!(Instant::now() < until, "it never said «{what}». It said:\n{}", self.said.join("\n"));
            each();
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for Program {
    fn drop(&mut self) {
        // Only ours, by its handle; if it already stopped this does nothing.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A UDP port of this machine that is free right now.
fn free_port() -> u16 {
    std::net::UdpSocket::bind("127.0.0.1:0").expect("a socket").local_addr().expect("an address").port()
}

/// The game's data, as the program is told where it is, and its fingerprint (what a client says).
fn data() -> (String, u32) {
    let dir = lunar_play::root().join("assets");
    (dir.to_str().expect("a path").to_string(), lunar_play::defs::fingerprint(&dir.join("defs")))
}

#[test]
fn the_program_takes_players_in_obeys_its_console_and_stops() {
    let port = free_port();
    let addr = format!("127.0.0.1:{port}");
    let (dir, scenario) = data();
    let build = lunar_play::net::BUILD;
    let mut program = Program::start(&["--puerto", &port.to_string(), "--nombre", "La Base", "--datos", &dir, "--sin-guardar"]);
    let line = program.expect_within("en marcha", 120.0, || {});
    assert!(line.starts_with(&format!("Servidor «La Base» en marcha en el puerto {port} (UDP): hasta 16 jugadores; la partida la simula él, 60 pasos por segundo (juego {build}, datos {scenario:08x}).")), "{line}");
    assert!(program.expect("--servidor", || {}).contains(&format!("--servidor 127.0.0.1:{port}")));
    assert_eq!(program.expect("Órdenes", || {}), "Órdenes: jugadores · expulsar <id> [motivo] · decir <texto> · salir. Para pararlo: «salir» (o cerrar esta ventana).");
    program.order("jugadores");
    program.expect("No hay nadie conectado.", || {});

    let mut ana = Client::connect(&addr, "Ana", build, scenario).expect("a client");
    let mut luis = Client::connect(&addr, "Luis", build, scenario).expect("a client");
    let mut heard: Vec<Event> = Vec::new();
    program.expect("Entra Ana (jugador 1, desde 127.0.0.1:", || ana.update(now()));
    let both = |ana: &mut Client, luis: &mut Client, heard: &mut Vec<Event>| {
        for c in [&mut *ana, &mut *luis] {
            c.update(now());
        }
        heard.extend(luis.events().filter(|e| !matches!(e, Event::Game { .. })));
        ana.events().for_each(drop);
    };
    assert!(program.expect("Entra Luis", || both(&mut ana, &mut luis, &mut heard)).ends_with("Ahora hay 2 jugadores."));
    // Someone with another version of the game is told why not, and so is the console.
    let mut old = Client::connect(&addr, "Marta", "V35", scenario).expect("a client");
    let refused = program.expect("No se deja entrar a Marta", || {
        old.update(now());
        both(&mut ana, &mut luis, &mut heard);
    });
    assert!(refused.ends_with(&format!("versión del juego distinta: la partida es de la versión «{build}» y la tuya es «V35».")), "{refused}");
    for _ in 0..20 {
        old.update(now());
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(matches!(old.status(), Status::Failed(why) if why.contains("V35")));


    program.order("jugadores");
    program.expect("2 jugadores:", || both(&mut ana, &mut luis, &mut heard));
    let line = program.expect("jugador 2: Luis · 127.0.0.1:", || both(&mut ana, &mut luis, &mut heard));
    assert!(line.ends_with(" ms"), "{line}");
    program.order("decir Reinicio en cinco minutos");
    program.expect("<servidor> Reinicio en cinco minutos", || both(&mut ana, &mut luis, &mut heard));
    ana.chat("vale");
    program.expect("<Ana> vale", || both(&mut ana, &mut luis, &mut heard));
    program.order("bailar");
    program.expect("No conozco la orden «bailar».", || both(&mut ana, &mut luis, &mut heard));
    program.order("expulsar 7");
    program.expect("No hay ningún jugador 7.", || both(&mut ana, &mut luis, &mut heard));
    program.order("expulsar 1 por pesada");
    assert_eq!(program.expect("Sale Ana", || both(&mut ana, &mut luis, &mut heard)), "Sale Ana (jugador 1): expulsado por el servidor. Queda 1 jugador.");
    for _ in 0..20 {
        both(&mut ana, &mut luis, &mut heard);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(ana.status(), Status::Failed("expulsado por el servidor: por pesada".to_string()));
    assert!(heard.contains(&Event::Chat { from: None, text: "Reinicio en cinco minutos".to_string() }));
    assert!(heard.contains(&Event::Chat { from: Some(1), text: "vale".to_string() }));
    assert!(heard.contains(&Event::Left { id: 1, name: "Ana".to_string() }));

    program.order("salir");
    program.expect("Sale Luis (jugador 2): el servidor se ha cerrado.", || both(&mut ana, &mut luis, &mut heard));
    program.expect("El servidor se queda vacío", || {});
    program.expect("Servidor parado.", || {});
    for _ in 0..50 {
        luis.update(now());
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(luis.status(), Status::Failed("el servidor se ha cerrado".to_string()));
    let status = program.child.wait().expect("it ends");
    assert!(status.success());
}

#[test]
fn a_port_already_taken_is_said_and_help_is_given() {
    let taken = std::net::UdpSocket::bind("0.0.0.0:0").expect("a socket");
    let port = taken.local_addr().expect("an address").port();
    let (dir, _) = data();
    let mut program = Program::start(&["--puerto", &port.to_string(), "--datos", &dir, "--sin-guardar"]);
    let line = program.expect_within("No se puede abrir el puerto", 120.0, || {});
    assert!(line.starts_with(&format!("No se puede abrir el puerto {port} (UDP): ")) && line.ends_with("¿Hay ya otro servidor en marcha en este puerto?"), "{line}");
    assert!(!program.child.wait().expect("it ends").success());

    let out = Command::new(env!("CARGO_BIN_EXE_luna-servidor")).arg("--ayuda").output().expect("the program runs");
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("Uso: SeleneServidor [--puerto N] [--nombre TEXTO] [--datos CARPETA] [--partida RUTA] [--nueva] [--sin-guardar] [--trucos]"));
    let out = Command::new(env!("CARGO_BIN_EXE_luna-servidor")).args(["--puerto", "cero"]).output().expect("the program runs");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).starts_with("--puerto: «cero» no es un puerto (un número entre 1 y 65535)"));
}

#[test]
fn the_program_has_the_game_and_a_player_walks_in_it() {
    // started as it is by default: it loads the game and simulates it; a player's game comes in over
    // UDP, is told where the game is, walks, and is never put right; one of another build is not
    // let in, and says why. Stopped, it keeps the game; started again, it takes it up, and the
    // player comes back to their body with their key
    use lunar_play::{defs::Defs, game::Game, game::Player, online::Online};
    let port = free_port();
    let addr = format!("127.0.0.1:{port}");
    let kept = std::env::temp_dir().join(format!("luna-partida-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&kept);
    let base = kept.join("partida");
    let data = lunar_play::root().join("assets");
    let args = ["--puerto", &port.to_string(), "--datos", data.to_str().expect("a path"), "--partida", base.to_str().expect("a path")].map(str::to_string);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut program = Program::start(&args);
    // (the game's data is read first: a moment)
    let line = program.expect_within("en marcha", 120.0, || {});
    assert!(line.contains("la partida la simula él, 60 pasos por segundo"), "{line}");
    let defs = Defs::load(&lunar_play::root().join("assets/defs")).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message));
    let mut game = Game::new(&defs, &lunar_play::root().join("assets/defs"), 500, |_| true).expect("the game");
    let mut me = Player::new(game.bodies.clone(), &game.site, defs.scenario.player);
    let client = Client::connect(&addr, "Ana", lunar_play::net::BUILD, defs.fingerprint).expect("a client");
    let mut online = Online::new(client, defs.scenario.player);
    let (mut last, mut walked) = (now(), 0.0);
    let start = Instant::now();
    while walked < 4.0 {
        assert!(start.elapsed() < Duration::from_secs(20), "it never let us play: {:?}", online.status());
        let t = now();
        online.receive(t, &mut game, &mut me);
        for _ in 0..online.steps(t - last) {
            me.input = lunar_play::pilot::Input { forward: 1.0, ..Default::default() };
            online.step(&mut game, &mut me);
            walked += lunar_play::game::STEP;
        }
        last = t;
        std::thread::sleep(Duration::from_millis(4));
    }
    program.expect("Entra Ana", || {});
    assert_eq!(online.stats.corrections, 0, "{:?}", online.stats);
    let mut old = Client::connect(&addr, "Marta", "V39", 3).expect("a client");
    let refused = program.expect("No se deja entrar a Marta", || old.update(now()));
    assert!(refused.contains(lunar_play::net::BUILD) && refused.contains("V39"), "{refused}");
    // garbage thrown at it over the real network, from a stranger: it plays on as if nothing
    let junk = std::net::UdpSocket::bind("127.0.0.1:0").expect("a socket");
    let mut dice = 0x9E37_79B9_7F4A_7C15u64;
    for k in 0..3000u64 {
        dice ^= dice << 13;
        dice ^= dice >> 7;
        dice ^= dice << 17;
        let n = 1 + (dice % 400) as usize;
        let mut d: Vec<u8> = (0..n).map(|i| (dice >> (i % 8 * 8)) as u8 ^ i as u8).collect();
        d[0] = 1 + (k % 6) as u8;
        let _ = junk.send_to(&d, &addr);
    }
    let (mut last, mut walked) = (now(), 0.0);
    while walked < 1.0 {
        let t = now();
        online.receive(t, &mut game, &mut me);
        for _ in 0..online.steps(t - last) {
            me.input = lunar_play::pilot::Input { forward: 1.0, ..Default::default() };
            online.step(&mut game, &mut me);
            walked += lunar_play::game::STEP;
        }
        last = t;
        std::thread::sleep(Duration::from_millis(4));
    }
    assert_eq!(online.stats.corrections, 0, "garbage changed nothing: {:?}", online.stats);
    let key = online.key.expect("a key to come back with");
    program.order("salir");
    let saved = program.expect("Partida guardada en", || online.receive(now(), &mut game, &mut me));
    assert!(saved.contains("partida.a.bin"), "{saved}");
    program.expect("Servidor parado.", || online.receive(now(), &mut game, &mut me));
    assert!(program.child.wait().expect("it ends").success());
    // started again: the game as it was, and the body waiting for its player
    let mut program = Program::start(&args);
    let line = program.expect_within("Partida retomada", 120.0, || {});
    assert!(line.contains("1 cuerpo esperando"), "{line}");
    program.expect_within("en marcha", 60.0, || {});
    let client = Client::connect(&addr, "Ana", lunar_play::net::BUILD, defs.fingerprint).expect("a client");
    let mut online = Online::back(client, defs.scenario.player, key);
    let start = Instant::now();
    while !online.live() {
        assert!(start.elapsed() < Duration::from_secs(20), "it never let us back: {:?}", online.status());
        online.receive(now(), &mut game, &mut me);
        std::thread::sleep(Duration::from_millis(4));
    }
    assert!(online.stats.back, "back in the body that waited");
    program.order("salir");
    let saved = program.expect("Partida guardada en", || online.receive(now(), &mut game, &mut me));
    assert!(saved.contains("partida.b.bin"), "the other slot: {saved}");
    program.expect("Servidor parado.", || online.receive(now(), &mut game, &mut me));
    assert!(program.child.wait().expect("it ends").success());
    let _ = std::fs::remove_dir_all(&kept);
}
