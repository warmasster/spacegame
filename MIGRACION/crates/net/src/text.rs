//! Every text of the library that a person ends up reading (reasons, notices: in Spanish, like
//! the rest of the game), and the cleaning of the texts people type.

/// Characters a player's name may have.
pub const NAME_CHARS: usize = 24;
/// Characters a chat line may have.
pub const CHAT_CHARS: usize = 240;
/// The name of someone who gave none.
pub const DEFAULT_NAME: &str = "Jugador";

pub const NO_ANSWER: &str = "el servidor no responde";
pub const LOST: &str = "se perdió la conexión con el servidor";
pub const KICKED: &str = "expulsado por el servidor";
pub const CLOSED: &str = "el servidor se ha cerrado";
pub const LEFT: &str = "se ha ido";
pub const SILENT: &str = "dejó de dar señal";
pub const BROKEN: &str = "no cumple el protocolo";
pub const SERVER_BROKEN: &str = "el servidor envía datos que no se entienden";
pub const BEHIND: &str = "su conexión no da abasto";
pub const BACK: &str = "ha vuelto a conectar desde la misma dirección";
pub const BYE: &str = "desconectado";
pub const TAKEN: &str = "ese sitio ya lo tiene otro jugador";

pub fn full(max: usize) -> String {
    format!("el servidor está lleno ({max} de {max} jugadores)")
}
pub fn version(server: u16, client: u16) -> String {
    let which = if client < server { "tu juego es más antiguo que el servidor: actualiza el juego" } else { "tu juego es más nuevo que el servidor: hay que actualizar el servidor" };
    format!("protocolo de red distinto: el servidor habla la versión {server} y tu juego la {client} ({which}; la 1 es la del juego V35 y la 2 la del V36)")
}
pub fn build(server: &str, client: &str) -> String {
    format!("versión del juego distinta: la partida es de la versión «{server}» y la tuya es «{client}»")
}
pub fn scenario(server: u32, client: u32) -> String {
    format!("escenario distinto: la partida empezó con otro mundo (el suyo es el {server:08x} y el tuyo el {client:08x}); hay que entrar con el mismo escenario")
}
pub fn bad_address(addr: &str) -> String {
    format!("no se encuentra el servidor «{addr}»")
}
pub fn no_socket(why: &str) -> String {
    format!("no se pudo abrir la red: {why}")
}

/// What someone typed, fit to show: no control characters, no spaces at the ends or repeated, at most `max` characters.
pub fn clean(text: &str, max: usize) -> String {
    let mut out = String::with_capacity(text.len().min(max * 4));
    let mut gap = false;
    for c in text.chars() {
        if c.is_control() || c.is_whitespace() {
            gap = !out.is_empty();
            continue;
        }
        // Characters that reorder or hide text have no place in a name or a chat line.
        if matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2069}' | '\u{FEFF}') {
            continue;
        }
        let need = if gap { 2 } else { 1 };
        if out.chars().count() + need > max {
            break;
        }
        if gap {
            out.push(' ');
            gap = false;
        }
        out.push(c);
    }
    out
}

/// `text` cut to at most `max` bytes, at a whole character.
pub fn cut(text: &str, max: usize) -> &str {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// A name fit to show, never empty.
pub fn clean_name(name: &str) -> String {
    let n = clean(name, NAME_CHARS);
    if n.is_empty() { DEFAULT_NAME.to_string() } else { n }
}
