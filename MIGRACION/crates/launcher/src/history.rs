//! What has been played: for each build, how many times it was started, for how long in all and
//! when last; and how the last game went. The launcher sees a game start and — unless it closes
//! as the game starts — end; it is kept in `launcher.json` with the options.
use crate::{
    builds::Edition,
    words::{self, Clock},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// What has been played of a build, or of several.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Played {
    /// Times it was started.
    pub starts: u32,
    /// The time it ran, of the games that were seen to end (seconds).
    pub seconds: u64,
    /// When it was last started (seconds since 1970); 0: never.
    pub last: u64,
    /// The last game of it that was seen to end stopped with an error.
    pub failed: bool,
}

impl Played {
    fn with(self, other: Played) -> Played {
        Played { starts: self.starts + other.starts, seconds: self.seconds + other.seconds, last: self.last.max(other.last), failed: self.failed || other.failed }
    }

    /// As it is said in a line: "3 veces · 1 h 12 min · ayer 21:10"; nothing if it never was.
    pub fn says(self, clock: Clock, now: u64) -> Option<String> {
        if self.starts == 0 {
            return None;
        }
        let times = if self.starts == 1 { "1 vez".to_string() } else { format!("{} veces", self.starts) };
        let time = if self.seconds == 0 { String::new() } else { format!(" · {}", words::span(self.seconds)) };
        Some(format!("{times}{time} · {}", clock.ago(self.last, now)))
    }
}

/// How the last game went.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastRun {
    pub version: u32,
    pub edition: Edition,
    /// When it was started (seconds since 1970).
    pub started: u64,
    /// How long it ran; none: the launcher did not see it end (it closed as the game started, or
    /// the game is still up).
    #[serde(default)]
    pub seconds: Option<u64>,
    /// Its exit code (0: it ended well); none: not seen, or the system gave none.
    #[serde(default)]
    pub code: Option<i32>,
    /// What it said as it stopped with an error (the first line of its error file).
    #[serde(default)]
    pub said: Option<String>,
}

impl LastRun {
    /// Whether it was seen to end, and well.
    pub fn good(&self) -> Option<bool> {
        self.seconds.map(|_| self.code == Some(0))
    }

    /// How it ended, as it is said: "salió bien tras 42 min", "se cerró con un error (código 3)
    /// tras 12 s: …", "no se vio cómo acabó".
    pub fn says(&self) -> String {
        let Some(seconds) = self.seconds else { return "no se vio cómo acabó (el launcher se cerró al iniciarla)".to_string() };
        let span = words::span(seconds);
        match (self.code, &self.said) {
            (Some(0), _) => format!("salió bien tras {span}"),
            (code, said) => {
                let code = code.map_or("sin código de salida".to_string(), |c| format!("código {c}"));
                let said = said.as_ref().map_or(String::new(), |s| format!(": {s}"));
                format!("se cerró con un error ({code}) tras {span}{said}")
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct History {
    /// By build: "35/demo".
    pub played: BTreeMap<String, Played>,
    pub last: Option<LastRun>,
}

fn key(version: u32, edition: Edition) -> String {
    format!("{version}/{}", edition.id())
}

impl History {
    /// A game of that build was started at `at`.
    pub fn started(&mut self, version: u32, edition: Edition, at: u64) {
        let played = self.played.entry(key(version, edition)).or_default();
        played.starts += 1;
        played.last = played.last.max(at);
        self.last = Some(LastRun { version, edition, started: at, seconds: None, code: None, said: None });
    }

    /// The game of that build started at `at` ended after `seconds` with that exit code, having
    /// said that.
    pub fn ended(&mut self, version: u32, edition: Edition, at: u64, seconds: u64, code: Option<i32>, said: Option<String>) {
        let played = self.played.entry(key(version, edition)).or_default();
        played.seconds += seconds;
        played.failed = code != Some(0);
        self.last = Some(LastRun { version, edition, started: at, seconds: Some(seconds), code, said });
    }

    #[cfg(test)]
    pub fn of(&self, version: u32, edition: Edition) -> Played {
        self.played.get(&key(version, edition)).copied().unwrap_or_default()
    }

    /// What has been played of a version, in all its editions.
    pub fn of_version(&self, version: u32) -> Played {
        let prefix = format!("{version}/");
        self.played.iter().filter(|(k, _)| k.starts_with(&prefix)).fold(Played::default(), |all, (_, p)| all.with(*p))
    }

    /// This history, of which `known` was already kept, put on top of `other` (what someone else
    /// kept meanwhile): what was played here since is added to it.
    pub fn added_to(&self, known: &History, other: &History) -> History {
        let mut all = other.clone();
        for (key, here) in &self.played {
            let before = known.played.get(key).copied().unwrap_or_default();
            let entry = all.played.entry(key.clone()).or_default();
            entry.starts += here.starts.saturating_sub(before.starts);
            entry.seconds += here.seconds.saturating_sub(before.seconds);
            entry.last = entry.last.max(here.last);
            // (how its last game ended is what was seen last: here, if one ended here since)
            if here != &before {
                entry.failed = here.failed;
            }
        }
        if self.last != known.last {
            all.last = self.last.clone();
        }
        all
    }

    pub fn total(&self) -> Played {
        self.played.values().fold(Played::default(), |all, p| all.with(*p))
    }

    /// The builds played, the most played first: (version, edition, what).
    pub fn most(&self) -> Vec<(u32, Edition, Played)> {
        let mut all: Vec<(u32, Edition, Played)> = self.played.iter().filter_map(|(k, p)| k.split_once('/').and_then(|(v, e)| Some((v.parse().ok()?, Edition::of(e)?, *p)))).collect();
        all.sort_by_key(|(version, edition, p)| (std::cmp::Reverse(p.seconds), std::cmp::Reverse(p.starts), std::cmp::Reverse(*version), *edition));
        all
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 86_400;
    /// 2026-10-04 16:46 UTC.
    const NOW: u64 = 20_730 * DAY + 16 * 3600 + 46 * 60;

    #[test]
    fn a_game_started_and_ended_is_counted() {
        let mut h = History::default();
        assert_eq!(h.of(35, Edition::Demo), Played::default());
        assert_eq!(h.total().says(Clock::default(), NOW), None);
        h.started(35, Edition::Demo, NOW - DAY);
        assert_eq!(h.of(35, Edition::Demo), Played { starts: 1, seconds: 0, last: NOW - DAY, failed: false });
        assert_eq!(h.last.as_ref().map(LastRun::says).as_deref(), Some("no se vio cómo acabó (el launcher se cerró al iniciarla)"));
        assert_eq!(h.last.as_ref().and_then(LastRun::good), None);
        h.ended(35, Edition::Demo, NOW - DAY, 42 * 60, Some(0), None);
        assert_eq!(h.of(35, Edition::Demo), Played { starts: 1, seconds: 2520, last: NOW - DAY, failed: false });
        assert_eq!(h.last.as_ref().map(LastRun::says).as_deref(), Some("salió bien tras 42 min"));
        assert_eq!(h.last.as_ref().and_then(LastRun::good), Some(true));
        // another edition of the same version, and another version
        h.started(35, Edition::Debug, NOW - 3600);
        h.ended(35, Edition::Debug, NOW - 3600, 12, Some(3), Some("opción desconocida: --x".to_string()));
        h.started(34, Edition::Demo, NOW - 9 * DAY);
        // (one of the two ended with an error: the version says so until that build ends well)
        assert_eq!(h.of_version(35), Played { starts: 2, seconds: 2532, last: NOW - 3600, failed: true });
        assert!(!h.of(35, Edition::Demo).failed && h.of(35, Edition::Debug).failed);
        assert_eq!(h.of_version(3), Played::default());
        assert_eq!(h.total(), Played { starts: 3, seconds: 2532, last: NOW - 3600, failed: true });
        h.started(35, Edition::Debug, NOW - 60);
        h.ended(35, Edition::Debug, NOW - 60, 30, Some(0), None);
        assert!(!h.of_version(35).failed);
        assert_eq!(h.most().iter().map(|(v, e, p)| (*v, *e, p.starts)).collect::<Vec<_>>(), [(35, Edition::Demo, 1), (35, Edition::Debug, 2), (34, Edition::Demo, 1)]);
    }

    #[test]
    fn what_was_played_here_is_added_to_what_someone_else_kept_meanwhile() {
        let mut known = History::default();
        known.started(35, Edition::Demo, NOW - DAY);
        known.ended(35, Edition::Demo, NOW - DAY, 600, Some(0), None);
        // here: the V35 demo once more, for ten minutes
        let mut here = known.clone();
        here.started(35, Edition::Demo, NOW - 3600);
        here.ended(35, Edition::Demo, NOW - 3600, 600, Some(0), None);
        // elsewhere, meanwhile: the V35 demo too (still up), and the V36 debug
        let mut other = known.clone();
        other.started(35, Edition::Demo, NOW - 7200);
        other.started(36, Edition::Debug, NOW - 60);
        let all = here.added_to(&known, &other);
        assert_eq!(all.of(35, Edition::Demo), Played { starts: 3, seconds: 1200, last: NOW - 3600, failed: false });
        assert_eq!(all.of(36, Edition::Debug), Played { starts: 1, seconds: 0, last: NOW - 60, failed: false });
        // (the last game is the one that ended here; with nothing played here, the other's stands)
        assert_eq!(all.last, here.last);
        assert_eq!(known.added_to(&known, &other), other);
        assert_eq!(here.added_to(&known, &known), here);
    }

    #[test]
    fn what_was_played_is_said_in_a_line() {
        let clock = Clock { offset: 7200 };
        assert_eq!(Played { starts: 1, seconds: 0, last: NOW - 60, failed: false }.says(clock, NOW).as_deref(), Some("1 vez · hoy 18:45"));
        assert_eq!(Played { starts: 3, seconds: 4320, last: NOW - DAY, failed: true }.says(clock, NOW).as_deref(), Some("3 veces · 1 h 12 min · ayer 18:46"));
        assert_eq!(Played { starts: 12, seconds: 59, last: NOW - 4 * DAY, failed: false }.says(clock, NOW).as_deref(), Some("12 veces · 59 s · hace 4 días"));
    }

    #[test]
    fn how_the_last_game_ended_is_said() {
        let run = |seconds, code, said: Option<&str>| LastRun { version: 35, edition: Edition::Demo, started: NOW, seconds, code, said: said.map(str::to_string) };
        assert_eq!(run(Some(12), Some(3), Some("opción desconocida: --x")).says(), "se cerró con un error (código 3) tras 12 s: opción desconocida: --x");
        assert_eq!(run(Some(4000), Some(-1073741819), None).says(), "se cerró con un error (código -1073741819) tras 1 h 6 min");
        assert_eq!(run(Some(5), None, None).says(), "se cerró con un error (sin código de salida) tras 5 s");
        assert_eq!(run(Some(12), Some(3), None).good(), Some(false));
    }

    #[test]
    fn the_history_survives_a_round_trip_and_an_old_file_has_none() {
        let mut h = History::default();
        h.started(36, Edition::Multiplayer, NOW);
        h.ended(36, Edition::Multiplayer, NOW, 90, Some(0), None);
        let text = serde_json::to_string_pretty(&h).unwrap();
        assert!(text.contains("\"36/multiplayer\"") && text.contains("\"starts\": 1") && text.contains("\"seconds\": 90"), "{text}");
        assert_eq!(serde_json::from_str::<History>(&text).unwrap(), h);
        assert_eq!(serde_json::from_str::<History>("{}").unwrap(), History::default());
        // (a record with a part missing is read with what it has)
        let partial: History = serde_json::from_str("{\"played\": {\"35/demo\": {\"starts\": 2}}, \"last\": {\"version\": 35, \"edition\": \"demo\", \"started\": 7}}").unwrap();
        assert_eq!(partial.of(35, Edition::Demo), Played { starts: 2, seconds: 0, last: 0, failed: false });
        assert_eq!(partial.last, Some(LastRun { version: 35, edition: Edition::Demo, started: 7, seconds: None, code: None, said: None }));
    }
}
