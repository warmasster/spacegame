//! The server's settings: defaults, then `servidor.jsonc` beside the program if it is there, then
//! the flags of the command line. The file is JSON with `//` and `/* */` comments and trailing
//! commas allowed; whatever is wrong in it is said in plain words and the server does not start.
use serde_json::Value;
use std::path::Path;

pub const FILE: &str = "servidor.jsonc";

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    /// UDP port to listen on.
    pub puerto: u16,
    /// The server's name, shown to who joins.
    pub nombre: String,
    pub max_jugadores: usize,
    /// Times a second states are passed on.
    pub tasa: u8,
    /// Seconds without hearing a client after which it is dropped.
    pub espera: f64,
}

impl Default for Config {
    fn default() -> Self {
        Config { puerto: lunar_net::DEFAULT_PORT, nombre: "Servidor de Selene".to_string(), max_jugadores: 16, tasa: 20, espera: 10.0 }
    }
}

pub const USAGE: &str = "Uso: SeleneServidor [--puerto N] [--nombre TEXTO]\n  --puerto N      puerto UDP en el que escuchar (por defecto 47600)\n  --nombre TEXTO  nombre del servidor\nEl resto de ajustes están en servidor.jsonc, junto al programa.";

/// What the settings could not be read for: said to the person as it is.
pub type Problem = String;

/// JSON with comments and trailing commas, to plain JSON. Line breaks stay where they were, so
/// the line of an error is the line of the file.
pub fn strip(text: &str) -> String {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    // Comments out.
    let mut bare = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            bare.push(c);
            match c {
                '\\' => bare.extend(chars.next()),
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                bare.push(c);
            }
            '/' if chars.peek() == Some(&'/') => {
                if chars.by_ref().any(|n| n == '\n') {
                    bare.push('\n');
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut star = false;
                for n in chars.by_ref() {
                    if n == '\n' {
                        bare.push('\n');
                    }
                    if star && n == '/' {
                        break;
                    }
                    star = n == '*';
                }
            }
            _ => bare.push(c),
        }
    }
    // Trailing commas out: a comma with nothing but blanks between it and a closing bracket.
    let chars: Vec<char> = bare.chars().collect();
    let mut out = String::with_capacity(bare.len());
    let mut in_string = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if c == '\\' && i + 1 < chars.len() {
                i += 1;
                out.push(chars[i]);
            } else if c == '"' {
                in_string = false;
            }
        } else if c == ',' && chars[i + 1..].iter().find(|n| !n.is_whitespace()).is_some_and(|n| matches!(n, '}' | ']')) {
            // Dropped.
        } else {
            in_string = c == '"';
            out.push(c);
        }
        i += 1;
    }
    out
}

fn whole(v: &Value, name: &str, min: u64, max: u64) -> Result<u64, Problem> {
    v.as_u64().filter(|n| (min..=max).contains(n)).ok_or_else(|| format!("«{name}» tiene que ser un número entero entre {min} y {max} (pone {v})"))
}

impl Config {
    /// Applies the text of a settings file. Returns notes for the person (things ignored).
    pub fn apply_file(&mut self, text: &str) -> Result<Vec<String>, Problem> {
        let value: Value = serde_json::from_str(&strip(text)).map_err(|e| format!("no es JSON válido: mira la línea {}, columna {}", e.line(), e.column()))?;
        let Value::Object(map) = value else { return Err("tiene que ser un objeto: { \"puerto\": 47600, ... }".to_string()) };
        let mut notes = Vec::new();
        for (key, v) in &map {
            match key.as_str() {
                "puerto" => self.puerto = whole(v, key, 1, 65535)? as u16,
                "nombre" => self.nombre = v.as_str().map(str::to_string).ok_or_else(|| format!("«nombre» tiene que ser un texto entre comillas (pone {v})"))?,
                "max_jugadores" => self.max_jugadores = whole(v, key, 1, 64)? as usize,
                "tasa" => self.tasa = whole(v, key, 5, 60)? as u8,
                "espera" => self.espera = v.as_f64().filter(|s| (2.0..=300.0).contains(s)).ok_or_else(|| format!("«espera» tiene que ser un número de segundos entre 2 y 300 (pone {v})"))?,
                other => notes.push(format!("{FILE}: no conozco el ajuste «{other}»; lo ignoro.")),
            }
        }
        Ok(notes)
    }

    /// Applies the flags of the command line. `Ok(false)`: help was asked for, nothing else to do.
    pub fn apply_flags(&mut self, args: impl Iterator<Item = String>) -> Result<bool, Problem> {
        let mut args = args;
        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--puerto" => {
                    let v = args.next().ok_or("falta el número después de --puerto")?;
                    self.puerto = v.parse::<u16>().ok().filter(|p| *p != 0).ok_or_else(|| format!("--puerto: «{v}» no es un puerto (un número entre 1 y 65535)"))?;
                }
                "--nombre" => self.nombre = args.next().ok_or("falta el texto después de --nombre")?,
                "--ayuda" | "-h" | "--help" | "/?" => return Ok(false),
                other => return Err(format!("no conozco la opción «{other}»")),
            }
        }
        Ok(true)
    }

    /// The settings to run with: defaults, the file in `dir` if there is one, the flags. `Ok(None)`: help was asked for.
    pub fn load(dir: &Path, args: impl Iterator<Item = String>) -> Result<Option<(Config, Vec<String>)>, Problem> {
        let mut config = Config::default();
        let mut notes = Vec::new();
        let path = dir.join(FILE);
        match std::fs::read_to_string(&path) {
            Ok(text) => notes = config.apply_file(&text).map_err(|e| format!("{FILE}: {e}. Corrígelo, o bórralo para usar los ajustes por defecto."))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => notes.push(format!("No hay {FILE} junto al programa: se usan los ajustes por defecto.")),
            Err(e) => return Err(format!("{FILE}: no se puede leer ({e}).")),
        }
        Ok(config.apply_flags(args)?.then_some((config, notes)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_and_trailing_commas_are_stripped_and_lines_kept() {
        let text = "\u{feff}{\n  // el puerto\n  \"puerto\": 5000, /* dos\n líneas */ \"nombre\": \"a // b, } /* c */ \\\" d\",\n  \"lista\": [1, 2, ],\n}\n";
        let plain = strip(text);
        assert_eq!(plain.lines().count(), text.lines().count(), "{plain}");
        let v: Value = serde_json::from_str(&plain).expect("plain JSON");
        assert_eq!(v["puerto"], 5000);
        assert_eq!(v["nombre"], "a // b, } /* c */ \" d");
        assert_eq!(v["lista"], serde_json::json!([1, 2]));
        // Odd ends do not hang or panic.
        for odd in ["", "/", "//", "/*", "/* *", "\"", "\"\\", ",", "{,}", "// sin fin de línea"] {
            let _ = strip(odd);
        }
        assert_eq!(strip("[1,\n]"), "[1\n]");
    }

    #[test]
    fn a_file_sets_what_it_names_and_leaves_the_rest() {
        let mut c = Config::default();
        let notes = c.apply_file("{ \"puerto\": 5001, \"max_jugadores\": 4, // cuatro\n \"espera\": 20, \"tasa\": 30, \"color\": \"rojo\", }").expect("a good file");
        assert_eq!(c, Config { puerto: 5001, max_jugadores: 4, espera: 20.0, tasa: 30, ..Config::default() });
        assert_eq!(notes, ["servidor.jsonc: no conozco el ajuste «color»; lo ignoro."]);
        let mut d = Config::default();
        assert!(d.apply_file("{}").expect("an empty file").is_empty());
        assert_eq!(d, Config::default());
        assert_eq!((d.puerto, d.max_jugadores, d.tasa, d.espera), (47600, 16, 20, 10.0));
    }

    #[test]
    fn what_is_wrong_in_the_file_is_said() {
        let bad = |text: &str| Config::default().apply_file(text).expect_err("a bad file");
        assert_eq!(bad("{ \"puerto\": 70000 }"), "«puerto» tiene que ser un número entero entre 1 y 65535 (pone 70000)");
        assert_eq!(bad("{ \"puerto\": \"47600\" }"), "«puerto» tiene que ser un número entero entre 1 y 65535 (pone \"47600\")");
        assert_eq!(bad("{ \"max_jugadores\": 0 }"), "«max_jugadores» tiene que ser un número entero entre 1 y 64 (pone 0)");
        assert_eq!(bad("{ \"tasa\": 2.5 }"), "«tasa» tiene que ser un número entero entre 5 y 60 (pone 2.5)");
        assert_eq!(bad("{ \"nombre\": 3 }"), "«nombre» tiene que ser un texto entre comillas (pone 3)");
        assert_eq!(bad("{ \"espera\": 1 }"), "«espera» tiene que ser un número de segundos entre 2 y 300 (pone 1)");
        assert_eq!(bad("{\n \"puerto\": 1\n \"nombre\": \"x\" }"), "no es JSON válido: mira la línea 3, columna 2");
        assert_eq!(bad("[1, 2]"), "tiene que ser un objeto: { \"puerto\": 47600, ... }");
    }

    #[test]
    fn flags_override_the_file() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>().into_iter();
        let mut c = Config::default();
        assert_eq!(c.apply_flags(args(&["--puerto", "5002", "--nombre", "La Base"])), Ok(true));
        assert_eq!((c.puerto, c.nombre.as_str()), (5002, "La Base"));
        assert_eq!(c.apply_flags(args(&["--ayuda"])), Ok(false));
        assert_eq!(c.apply_flags(args(&["--puerto"])), Err("falta el número después de --puerto".to_string()));
        assert_eq!(c.apply_flags(args(&["--puerto", "cero"])), Err("--puerto: «cero» no es un puerto (un número entre 1 y 65535)".to_string()));
        assert_eq!(c.apply_flags(args(&["--rapido"])), Err("no conozco la opción «--rapido»".to_string()));
    }

    #[test]
    fn the_file_shipped_with_the_server_is_good_and_says_the_defaults() {
        let shipped = include_str!("../../../servidores/servidor.jsonc");
        let mut c = Config::default();
        assert!(c.apply_file(shipped).expect("the shipped file").is_empty());
        assert_eq!(c, Config::default());
    }
}
