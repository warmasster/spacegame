//! The server's console: the lines typed on it, read by a helper thread so the main loop never
//! waits for the keyboard, and what each one asks for.
use std::io::BufRead;
use std::sync::mpsc::{Receiver, channel};

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// `jugadores`: who is connected.
    Players,
    /// `expulsar <id> [motivo]`.
    Kick(u32, String),
    /// `decir <texto>`: a notice to everyone.
    Say(String),
    /// `salir`: say goodbye to everyone and stop.
    Quit,
    Help,
    /// An empty line.
    Nothing,
    /// Not understood; what to tell the person.
    Wrong(String),
}

pub const HELP: &str = "Órdenes: jugadores · expulsar <id> [motivo] · decir <texto> · salir";

pub fn parse(line: &str) -> Command {
    let line = line.trim();
    let (word, rest) = line.split_once(char::is_whitespace).map_or((line, ""), |(w, r)| (w, r.trim()));
    match word.to_lowercase().as_str() {
        "" => Command::Nothing,
        "jugadores" | "lista" => Command::Players,
        "expulsar" => {
            let (id, reason) = rest.split_once(char::is_whitespace).map_or((rest, ""), |(i, r)| (i, r.trim()));
            match id.parse() {
                Ok(id) => Command::Kick(id, reason.to_string()),
                Err(_) => Command::Wrong("expulsar: falta el número del jugador (míralo con «jugadores»). Ejemplo: expulsar 3".to_string()),
            }
        }
        "decir" if rest.is_empty() => Command::Wrong("decir: falta el texto. Ejemplo: decir Reinicio en cinco minutos".to_string()),
        "decir" => Command::Say(rest.to_string()),
        "salir" | "parar" => Command::Quit,
        "ayuda" | "?" => Command::Help,
        other => Command::Wrong(format!("No conozco la orden «{other}». {HELP}")),
    }
}

/// Starts reading the keyboard; the lines typed arrive through what it returns. When there is
/// no keyboard (the server was started without a console), nothing ever arrives, and that is all.
pub fn listen() -> Receiver<String> {
    let (tx, rx) = channel();
    // If the thread cannot be started the server runs all the same, without orders.
    let _ = std::thread::Builder::new().name("consola".to_string()).spawn(move || {
        for line in std::io::stdin().lock().lines() {
            // The keyboard is gone, or the server is: either way there is nothing more to read for.
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_are_understood() {
        assert_eq!(parse("jugadores"), Command::Players);
        assert_eq!(parse("  JUGADORES  "), Command::Players);
        assert_eq!(parse("expulsar 3"), Command::Kick(3, String::new()));
        assert_eq!(parse("expulsar 12   por pesado  "), Command::Kick(12, "por pesado".to_string()));
        assert_eq!(parse("decir   Reinicio en 5 minutos"), Command::Say("Reinicio en 5 minutos".to_string()));
        assert_eq!(parse("salir"), Command::Quit);
        assert_eq!(parse("ayuda"), Command::Help);
        assert_eq!(parse(""), Command::Nothing);
        assert_eq!(parse("   "), Command::Nothing);
    }

    #[test]
    fn orders_that_are_wrong_say_how_they_go() {
        assert!(matches!(parse("expulsar"), Command::Wrong(t) if t.starts_with("expulsar: falta el número")));
        assert!(matches!(parse("expulsar Ana"), Command::Wrong(t) if t.starts_with("expulsar: falta el número")));
        assert!(matches!(parse("decir"), Command::Wrong(t) if t.starts_with("decir: falta el texto")));
        assert_eq!(parse("bailar"), Command::Wrong(format!("No conozco la orden «bailar». {HELP}")));
    }
}
