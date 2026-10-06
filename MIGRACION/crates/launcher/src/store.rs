//! `launcher.json`, next to the launcher: the options and the history of what was played, in one
//! file. The options are its top keys, as they always were (a file of an older launcher is read
//! as it is); the history is under `history`.
//!
//! The launcher of before, which is still about, reads this file too, and when it keeps its
//! options it writes the file anew with the keys it knows: the history would go. So what is
//! gathered with time (the history, the servers joined) is also kept in a second file,
//! `launcher_historial.json`, which only this launcher writes; a `launcher.json` that comes back
//! without its history gets it from there.
use crate::{
    history::History,
    options::{FILE, Options},
};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The file with the copy of what is gathered with time.
pub const COPY: &str = "launcher_historial.json";

/// What is kept between one time the launcher is opened and the next.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Kept {
    #[serde(flatten)]
    pub options: Options,
    pub history: History,
}

/// What is gathered with time, as it is in its copy.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct Gathered {
    history: History,
    servers: Vec<String>,
}

impl Kept {
    /// What a file's text holds.
    #[cfg(test)]
    pub fn parse(text: &str) -> Kept {
        Kept::read(text).0
    }

    /// What a file's text holds, and whether it holds a history at all (one written by the
    /// launcher of before holds none): what it does not say is the default, and so is what of it
    /// cannot be read (options that make no sense do not take the history with them, nor the
    /// other way).
    fn read(text: &str) -> (Kept, bool) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else { return (Kept::default(), false) };
        let history = value.get("history").and_then(|h| serde_json::from_value(h.clone()).ok());
        (Kept { options: Options::of(&value), history: history.clone().unwrap_or_default() }, history.is_some())
    }

    /// What is kept in the folder `dir`: its `launcher.json`, with what was gathered taken from
    /// its copy if the file has lost it.
    pub fn load(dir: &Path) -> Kept {
        let (mut kept, whole) = std::fs::read_to_string(dir.join(FILE)).map(|text| Kept::read(&text)).unwrap_or_default();
        if !whole && let Some(copy) = std::fs::read_to_string(dir.join(COPY)).ok().and_then(|text| serde_json::from_str::<Gathered>(&text).ok()) {
            kept.history = copy.history;
            if kept.options.servers.is_empty() {
                kept.options.servers = Options::of(&serde_json::json!({ "servers": copy.servers })).servers;
            }
        }
        kept
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::write(dir.join(FILE), serde_json::to_string_pretty(self)?)?;
        std::fs::write(dir.join(COPY), serde_json::to_string_pretty(&Gathered { history: self.history.clone(), servers: self.options.servers.clone() })?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        builds::Edition,
        options::{AfterPlay, Quality},
    };

    fn folder(test: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("luna-launcher-guardar-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// Something of everything that is kept.
    fn played() -> Kept {
        let mut kept = Kept { options: Options { version: Some(35), edition: Edition::Multiplayer, quality: Quality::Alta, name: "Ñandú".to_string(), ..Options::default() }, history: History::default() };
        kept.options.joined("192.168.1.67:47600");
        kept.history.started(35, Edition::Multiplayer, 1_791_000_000);
        kept.history.ended(35, Edition::Multiplayer, 1_791_000_000, 2520, Some(0), None);
        kept
    }

    #[test]
    fn the_options_and_the_history_share_a_file_and_survive_a_round_trip() {
        let dir = folder("ida-y-vuelta");
        assert_eq!(Kept::load(&dir), Kept::default());
        std::fs::create_dir_all(&dir).unwrap();
        let kept = played();
        kept.save(&dir).unwrap();
        assert_eq!(Kept::load(&dir), kept);
        let text = std::fs::read_to_string(dir.join(FILE)).unwrap();
        // the options are the file's top keys, as before; the history has its own
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!((value["version"].as_u64(), value["edition"].as_str(), value["quality"].as_str()), (Some(35), Some("multiplayer"), Some("alta")));
        assert_eq!(value["servers"][0].as_str(), Some("192.168.1.67:47600"));
        assert_eq!(value["history"]["played"]["35/multiplayer"]["seconds"].as_u64(), Some(2520));
        assert_eq!(value["history"]["last"]["code"].as_i64(), Some(0));
        assert!(value.get("options").is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_file_of_the_launcher_before_is_read_as_it_is() {
        let old = "{\n  \"version\": 34,\n  \"edition\": \"debug\",\n  \"quality\": \"muyalta\",\n  \"fullscreen\": false,\n  \"window\": [1920, 1080],\n  \"vsync\": false,\n  \"backend\": \"vulkan\",\n  \"sound\": false,\n  \"skip_menu\": true,\n  \"close_on_play\": false,\n  \"server\": \"luna.example:47611\",\n  \"name\": \"Ana\"\n}";
        let kept = Kept::parse(old);
        let want = Options {
            version: Some(34),
            edition: Edition::Debug,
            quality: Quality::MuyAlta,
            fullscreen: false,
            window: (1920, 1080),
            vsync: false,
            backend: crate::options::Backend::Vulkan,
            sound: false,
            skip_menu: true,
            after_play: AfterPlay::Stay,
            server: "luna.example:47611".to_string(),
            name: "Ana".to_string(),
            ..Options::default()
        };
        assert_eq!(kept, Kept { options: want, history: History::default() });
    }

    #[test]
    fn what_cannot_be_read_of_a_file_is_the_default_and_the_rest_is_kept() {
        assert_eq!(Kept::parse(""), Kept::default());
        assert_eq!(Kept::parse("[1, 2]"), Kept::default());
        // options that make no sense: the history is still read
        let kept = Kept::parse("{\"quality\": \"imposible\", \"history\": {\"played\": {\"35/demo\": {\"starts\": 4, \"seconds\": 100, \"last\": 9}}}}");
        assert_eq!(kept.options, Options::default());
        assert_eq!(kept.history.of(35, Edition::Demo).starts, 4);
        // a history that makes no sense: the options are still read
        let kept = Kept::parse("{\"vsync\": false, \"history\": [\"roto\"]}");
        assert_eq!(kept, Kept { options: Options { vsync: false, ..Options::default() }, history: History::default() });
    }

    #[test]
    fn what_was_gathered_survives_the_launcher_of_before_writing_the_file_anew() {
        let dir = folder("copia");
        std::fs::create_dir_all(&dir).unwrap();
        let kept = played();
        kept.save(&dir).unwrap();
        // the launcher of before keeps its options: the keys it knows, and nothing else
        std::fs::write(dir.join(FILE), "{\n  \"version\": 34,\n  \"edition\": \"demo\",\n  \"vsync\": false,\n  \"close_on_play\": true,\n  \"server\": \"127.0.0.1:47600\",\n  \"name\": \"Ana\"\n}").unwrap();
        let back = Kept::load(&dir);
        assert_eq!(back.history, kept.history);
        assert_eq!(back.options.servers, ["192.168.1.67:47600"]);
        // (and the options are the ones that launcher left)
        assert_eq!((back.options.version, back.options.vsync, back.options.after_play, back.options.name.as_str()), (Some(34), false, AfterPlay::Close, "Ana"));
        // a file that has its history is not given another: an empty one stays empty
        Kept { history: History::default(), ..back.clone() }.save(&dir).unwrap();
        std::fs::write(dir.join(COPY), serde_json::to_string(&Gathered { history: kept.history.clone(), servers: Vec::new() }).unwrap()).unwrap();
        assert_eq!(Kept::load(&dir).history, History::default());
        // with no file at all, or a copy that cannot be read, there is nothing to take
        std::fs::remove_file(dir.join(FILE)).unwrap();
        assert_eq!(Kept::load(&dir), Kept { history: kept.history.clone(), ..Kept::default() });
        std::fs::write(dir.join(COPY), "roto").unwrap();
        assert_eq!(Kept::load(&dir), Kept::default());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
