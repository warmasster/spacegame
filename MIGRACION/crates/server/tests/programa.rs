//! The program itself, from outside: started as a person would start it (on a free port of this
//! machine), joined by two real clients over UDP, given orders through its console, and stopped.
//! What it prints is what is checked.
use lunar_net::{Client, Event, PlayerState, Status, now};
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
    fn expect(&mut self, what: &str, mut each: impl FnMut()) -> String {
        let until = Instant::now() + Duration::from_secs(5);
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

#[test]
fn the_program_takes_players_in_obeys_its_console_and_stops() {
    let port = free_port();
    let addr = format!("127.0.0.1:{port}");
    let mut program = Program::start(&["--puerto", &port.to_string(), "--nombre", "La Base"]);
    assert_eq!(program.expect("en marcha", || {}), format!("Servidor «La Base» en marcha en el puerto {port} (UDP): hasta 16 jugadores, 20 envíos por segundo."));
    assert!(program.expect("--servidor", || {}).contains(&format!("--servidor 127.0.0.1:{port}")));
    assert_eq!(program.expect("Órdenes", || {}), "Órdenes: jugadores · expulsar <id> [motivo] · decir <texto> · salir. Para pararlo: «salir» (o cerrar esta ventana).");
    program.order("jugadores");
    program.expect("No hay nadie conectado.", || {});

    let mut ana = Client::connect(&addr, "Ana", "V36", 3).expect("a client");
    let mut luis = Client::connect(&addr, "Luis", "V36", 3).expect("a client");
    let mut heard: Vec<Event> = Vec::new();
    program.expect("Entra Ana (jugador 1, desde 127.0.0.1:", || ana.update(now()));
    let both = |ana: &mut Client, luis: &mut Client, heard: &mut Vec<Event>| {
        for c in [&mut *ana, &mut *luis] {
            c.set_player(&PlayerState::default());
            c.update(now());
        }
        heard.extend(luis.events());
    };
    assert!(program.expect("Entra Luis", || both(&mut ana, &mut luis, &mut heard)).ends_with("Ahora hay 2 jugadores."));
    // Someone with another version of the game is told why not, and so is the console.
    let mut old = Client::connect(&addr, "Marta", "V35", 3).expect("a client");
    let refused = program.expect("No se deja entrar a Marta", || {
        old.update(now());
        both(&mut ana, &mut luis, &mut heard);
    });
    assert!(refused.ends_with("versión del juego distinta: la partida es de la versión «V36» y la tuya es «V35»."), "{refused}");
    for _ in 0..20 {
        old.update(now());
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(matches!(old.status(), Status::Failed(why) if why.contains("V35")));

    // What the game tells goes through the program unread, to everyone else and to one player.
    assert!(ana.tell(&[1, 2, 3]) && ana.tell_to(2, &[9; 3000]));
    ana.claim(lunar_net::key::seat(4, 1));
    for _ in 0..100 {
        both(&mut ana, &mut luis, &mut heard);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(heard.contains(&Event::Told { by: 1, data: vec![1, 2, 3] }) && heard.contains(&Event::Direct { by: 1, data: vec![9; 3000] }));
    assert!(heard.contains(&Event::Host { player: Some(1) }) && heard.contains(&Event::Owner { key: lunar_net::key::seat(4, 1), player: Some(1) }));

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
    // (the host gone, the one who is left is the host, and the seat she had is free)
    assert!(heard.contains(&Event::Host { player: Some(2) }) && heard.contains(&Event::Owner { key: lunar_net::key::seat(4, 1), player: None }));

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
    let mut program = Program::start(&["--puerto", &port.to_string()]);
    let line = program.expect("No se puede abrir el puerto", || {});
    assert!(line.starts_with(&format!("No se puede abrir el puerto {port} (UDP): ")) && line.ends_with("¿Hay ya otro servidor en marcha en este puerto?"), "{line}");
    assert!(!program.child.wait().expect("it ends").success());

    let out = Command::new(env!("CARGO_BIN_EXE_luna-servidor")).arg("--ayuda").output().expect("the program runs");
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("Uso: SeleneServidor [--puerto N] [--nombre TEXTO]"));
    let out = Command::new(env!("CARGO_BIN_EXE_luna-servidor")).args(["--puerto", "cero"]).output().expect("the program runs");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).starts_with("--puerto: «cero» no es un puerto (un número entre 1 y 65535)"));
}
