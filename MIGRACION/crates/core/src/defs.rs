//! Content definitions on disk: JSON with `//` and `/* */` comments and trailing commas (`.jsonc`),
//! read into serde types. Registries load a folder: the file name (without extension) is the id.
use serde::de::DeserializeOwned;
use std::{
    fmt,
    path::{Path, PathBuf},
};

pub const EXTENSION: &str = "jsonc";

#[derive(Debug)]
pub struct DefError {
    pub file: String,
    pub message: String,
}

impl DefError {
    pub fn new(file: impl Into<String>, message: impl Into<String>) -> DefError {
        DefError { file: file.into(), message: message.into() }
    }
}

impl fmt::Display for DefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.file, self.message)
    }
}

impl std::error::Error for DefError {}

/// Parse one definition; `name` only labels errors.
pub fn parse<T: DeserializeOwned>(name: &str, text: &str) -> Result<T, DefError> {
    serde_json::from_str(&strip(text)).map_err(|e| DefError::new(name, e.to_string()))
}

pub fn load<T: DeserializeOwned>(path: &Path) -> Result<T, DefError> {
    let text = std::fs::read_to_string(path).map_err(|e| DefError::new(path.display().to_string(), e.to_string()))?;
    parse(&path.display().to_string(), &text)
}

/// `dir/id.jsonc`.
pub fn file(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.{EXTENSION}"))
}

/// Every definition of a folder as (id, value), sorted by id.
pub fn load_dir<T: DeserializeOwned>(dir: &Path) -> Result<Vec<(String, T)>, DefError> {
    let entries = std::fs::read_dir(dir).map_err(|e| DefError::new(dir.display().to_string(), e.to_string()))?;
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == EXTENSION)).collect();
    paths.sort();
    paths
        .iter()
        .map(|p| {
            let id = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            load(p).map(|v| (id, v))
        })
        .collect()
}

/// Blank out comments and trailing commas; lines and columns stay where they were.
fn strip(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = b.to_vec();
    let mut i = 0;
    let mut in_str = false;
    // last significant byte outside strings/comments: a comma there is dropped if a bracket follows
    let mut comma: Option<usize> = None;
    while i < b.len() {
        let c = b[i];
        if in_str {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' => {
                in_str = true;
                comma = None;
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    out[i] = b' ';
                    i += 1;
                }
                continue;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                while i < b.len() && !(b[i] == b'*' && b.get(i + 1) == Some(&b'/')) {
                    if b[i] != b'\n' {
                        out[i] = b' ';
                    }
                    i += 1;
                }
                for k in i..(i + 2).min(b.len()) {
                    out[k] = b' ';
                }
                i += 2;
                continue;
            }
            b',' => comma = Some(i),
            b']' | b'}' => {
                if let Some(k) = comma.take() {
                    out[k] = b' ';
                }
            }
            b' ' | b'\t' | b'\r' | b'\n' => {}
            _ => comma = None,
        }
        i += 1;
    }
    // only ASCII bytes were replaced (by spaces), so the text is still UTF-8
    String::from_utf8(out).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #[test]
    fn comments_and_trailing_commas() {
        #[derive(serde::Deserialize)]
        struct T {
            a: Vec<u32>,
            s: String,
        }
        let t: T = super::parse("t", "{ // x\n \"a\": [1, 2, /* 3 */ ], \"s\": \"// no, /* */\",\n }").unwrap();
        assert_eq!(t.a, [1, 2]);
        assert_eq!(t.s, "// no, /* */");
        let e = super::parse::<T>("t", "{\n\"a\": 1 }").err().unwrap();
        assert!(e.to_string().contains("line 2"), "{e}");
    }
}
