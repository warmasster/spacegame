//! Putting old versions away, and bringing them back: the builds older than the newest few are
//! MOVED to `versiones_antiguas` (never deleted, never over a file that is already there), where
//! they are still listed and can still be played. Only files that are builds by their name are
//! ever moved.
use crate::{
    builds::{self, ARCHIVE},
    versions::Version,
    words,
};
use std::path::Path;

/// What putting the old versions away would move.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// How many of the newest versions stay at hand.
    pub keep: usize,
    /// The versions that go, the newest first, and their files (their names in the game's folder).
    pub versions: Vec<u32>,
    pub files: Vec<String>,
    pub bytes: u64,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// As it is asked before it is done.
    pub fn says(&self) -> String {
        let (Some(newest), Some(oldest)) = (self.versions.first(), self.versions.last()) else { return format!("No hay nada que archivar: no hay más de {} versiones a mano.", self.keep) };
        let which = if newest == oldest { format!("la V{newest}") } else { format!("de la V{oldest} a la V{newest}") };
        let will = if self.versions.len() == 1 { "Se moverá" } else { "Se moverán" };
        format!("{will} {} ({which}; {}, {}) a la carpeta {ARCHIVE}.", words::plural(self.versions.len(), "versión", "versiones"), words::plural(self.files.len(), "ejecutable", "ejecutables"), words::size(self.bytes))
    }
}

/// The name of a file of the game's folder, if that is where it is (not in a folder of it).
fn at_hand(path: &str) -> Option<&str> {
    (!path.contains(['/', '\\'])).then_some(path)
}

/// What there is to put away keeping the newest `keep` versions at hand: the files, still at
/// hand, of every version older than those.
pub fn plan(versions: &[Version], keep: usize) -> Plan {
    let mut at_hand_versions: Vec<&Version> = versions.iter().filter(|v| !v.archived).collect();
    at_hand_versions.sort_by_key(|v| std::cmp::Reverse(v.number));
    let mut plan = Plan { keep, ..Plan::default() };
    for version in at_hand_versions.into_iter().skip(keep) {
        plan.versions.push(version.number);
        for (path, facts) in &version.files {
            if let Some(name) = at_hand(path) {
                plan.files.push(name.to_string());
                plan.bytes += facts.size;
            }
        }
    }
    plan
}

/// The files of a version that are put away (their names), to bring them back.
pub fn away(version: &Version) -> Vec<String> {
    version.files.iter().filter_map(|(path, _)| path.strip_prefix(ARCHIVE).and_then(|rest| rest.strip_prefix('/'))).map(str::to_string).collect()
}

/// What a move came to: the files moved and, of the ones that were not, why not.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Moved {
    pub done: Vec<String>,
    pub failed: Vec<(String, String)>,
}

impl Moved {
    /// As it is told: what was done ("Guardados en …: 3 ejecutables") and, if something was not,
    /// how many and the first reason.
    pub fn says(&self, done: &str) -> (String, bool) {
        let moved = format!("{done}: {}", words::plural(self.done.len(), "ejecutable", "ejecutables"));
        match self.failed.first() {
            None => (format!("{moved}."), true),
            Some((file, why)) => (format!("{moved}. No se movieron {} ({file}: {why}).", self.failed.len()), false),
        }
    }
}

/// Move the builds `names` from one folder to another (made if it is not there). A name that is
/// not a build's is refused; a file is never moved over one that is already there.
fn move_builds(from: &Path, to: &Path, names: &[String]) -> Moved {
    let mut moved = Moved::default();
    if let Err(e) = std::fs::create_dir_all(to) {
        moved.failed = names.iter().map(|n| (n.clone(), format!("no se pudo crear {}: {e}", to.display()))).collect();
        return moved;
    }
    for name in names {
        let why = if builds::parse(name).is_none() || at_hand(name).is_none() {
            Some("no es una versión del juego".to_string())
        } else if !from.join(name).is_file() {
            Some("ya no está".to_string())
        } else if to.join(name).exists() {
            Some("ya hay un fichero con ese nombre en el destino".to_string())
        } else {
            std::fs::rename(from.join(name), to.join(name)).err().map(|e| e.to_string())
        };
        match why {
            None => moved.done.push(name.clone()),
            Some(why) => moved.failed.push((name.clone(), why)),
        }
    }
    moved
}

/// Put builds of the game's folder away.
pub fn put_away(root: &Path, names: &[String]) -> Moved {
    move_builds(root, &root.join(ARCHIVE), names)
}

/// Bring builds that were put away back to the game's folder.
pub fn bring_back(root: &Path, names: &[String]) -> Moved {
    move_builds(&root.join(ARCHIVE), root, names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::versions::{Facts, catalog};

    /// A game's folder made for the test, with these files (each holds its own name).
    fn folder(test: &str, files: &[&str]) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("luna-launcher-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        for file in files {
            std::fs::write(root.join(file), file).unwrap();
        }
        root
    }

    fn looked(root: &Path) -> Vec<Version> {
        catalog(&builds::find(root), |b| Facts { size: std::fs::metadata(root.join(b.path())).map_or(0, |m| m.len()), modified: 0 }, &[])
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names = builds::names(dir);
        names.sort();
        names
    }

    /// (Builds under both of the game's names: the V36 is from after it was renamed.)
    const FILES: [&str; 10] = ["SeleneV36_demo.exe", "SeleneV36_debug.exe", "LunaV35_demo.exe", "LunaV35_multiplayer.exe", "LunaV34_demo.exe", "LunaV21.exe", "LunaV9.exe", "Luna.exe", "LunaLauncher.exe", "SeleneLauncher.exe"];

    #[test]
    fn the_plan_is_what_is_older_than_the_newest_few() {
        let root = folder("plan", &FILES);
        let versions = looked(&root);
        let p = plan(&versions, 3);
        assert_eq!((p.versions.as_slice(), p.files.as_slice(), p.bytes), (&[21, 9][..], &["LunaV21.exe".to_string(), "LunaV9.exe".to_string()][..], 21));
        assert_eq!(p.says(), "Se moverán 2 versiones (de la V9 a la V21; 2 ejecutables, 1 kB) a la carpeta versiones_antiguas.");
        assert_eq!(plan(&versions, 4).says(), "Se moverá 1 versión (la V9; 1 ejecutable, 1 kB) a la carpeta versiones_antiguas.");
        let none = plan(&versions, 5);
        assert!(none.is_empty() && none.versions.is_empty());
        assert_eq!(none.says(), "No hay nada que archivar: no hay más de 5 versiones a mano.");
        assert_eq!(plan(&versions, 0).versions, [36, 35, 34, 21, 9]);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn old_versions_are_moved_never_deleted_and_come_back() {
        let root = folder("mover", &FILES);
        let p = plan(&looked(&root), 2);
        assert_eq!(p.versions, [34, 21, 9]);
        let moved = put_away(&root, &p.files);
        assert_eq!((moved.done.len(), moved.failed.len()), (3, 0));
        assert_eq!(moved.says("Archivados en versiones_antiguas"), ("Archivados en versiones_antiguas: 3 ejecutables.".to_string(), true));
        assert_eq!(names(&root), ["Luna.exe", "LunaLauncher.exe", "LunaV35_demo.exe", "LunaV35_multiplayer.exe", "SeleneLauncher.exe", "SeleneV36_debug.exe", "SeleneV36_demo.exe"]);
        assert_eq!(names(&root.join(ARCHIVE)), ["LunaV21.exe", "LunaV34_demo.exe", "LunaV9.exe"]);
        // (each is the file it was: it was moved, not made again)
        assert_eq!(std::fs::read_to_string(root.join(ARCHIVE).join("LunaV21.exe")).unwrap(), "LunaV21.exe");
        // they are still versions, put away; and there is nothing more to put away
        let versions = looked(&root);
        assert_eq!(versions.iter().map(|v| (v.number, v.archived)).collect::<Vec<_>>(), [(36, false), (35, false), (34, true), (21, true), (9, true)]);
        assert!(plan(&versions, 2).is_empty());
        // one comes back
        let v34 = versions.iter().find(|v| v.number == 34).unwrap();
        assert_eq!(away(v34), ["LunaV34_demo.exe"]);
        assert!(away(&versions[0]).is_empty());
        let back = bring_back(&root, &away(v34));
        assert_eq!((back.done.as_slice(), back.failed.len()), (&["LunaV34_demo.exe".to_string()][..], 0));
        assert!(root.join("LunaV34_demo.exe").is_file() && !root.join(ARCHIVE).join("LunaV34_demo.exe").exists());
        assert_eq!(looked(&root).iter().map(|v| (v.number, v.archived)).collect::<Vec<_>>(), [(36, false), (35, false), (34, false), (21, true), (9, true)]);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn nothing_is_moved_over_a_file_that_is_there_and_only_builds_are_moved() {
        let root = folder("no-pisar", &FILES);
        std::fs::create_dir_all(root.join(ARCHIVE)).unwrap();
        std::fs::write(root.join(ARCHIVE).join("LunaV9.exe"), "el que ya estaba").unwrap();
        let asked: Vec<String> = ["LunaV9.exe", "LunaV21.exe", "Luna.exe", "LunaLauncher.exe", "LunaV1.exe", "../LunaV34_demo.exe", "LEEME.txt", "SeleneLauncher.exe", "SeleneV36_debug.exe"].map(str::to_string).to_vec();
        let moved = put_away(&root, &asked);
        assert_eq!(moved.done, ["LunaV21.exe", "SeleneV36_debug.exe"]);
        let why: Vec<(&str, &str)> = moved.failed.iter().map(|(file, why)| (file.as_str(), why.as_str())).collect();
        assert_eq!(
            why,
            [
                ("LunaV9.exe", "ya hay un fichero con ese nombre en el destino"),
                ("Luna.exe", "no es una versión del juego"),
                ("LunaLauncher.exe", "no es una versión del juego"),
                ("LunaV1.exe", "ya no está"),
                ("../LunaV34_demo.exe", "no es una versión del juego"),
                ("LEEME.txt", "no es una versión del juego"),
                ("SeleneLauncher.exe", "no es una versión del juego")
            ]
        );
        assert_eq!(moved.says("Archivados"), ("Archivados: 2 ejecutables. No se movieron 7 (LunaV9.exe: ya hay un fichero con ese nombre en el destino).".to_string(), false));
        // both of the V9 are as they were, and so is everything that is no build
        assert_eq!(std::fs::read_to_string(root.join("LunaV9.exe")).unwrap(), "LunaV9.exe");
        assert_eq!(std::fs::read_to_string(root.join(ARCHIVE).join("LunaV9.exe")).unwrap(), "el que ya estaba");
        assert!(root.join("Luna.exe").is_file() && root.join("LunaLauncher.exe").is_file() && root.join("SeleneLauncher.exe").is_file() && root.join("LunaV34_demo.exe").is_file());
        assert_eq!(std::fs::read_to_string(root.join(ARCHIVE).join("SeleneV36_debug.exe")).unwrap(), "SeleneV36_debug.exe");
        // and the same coming back: the one at hand is not written over
        let back = bring_back(&root, &["LunaV9.exe".to_string(), "LunaV21.exe".to_string()]);
        assert_eq!((back.done.as_slice(), back.failed.len()), (&["LunaV21.exe".to_string()][..], 1));
        assert_eq!(std::fs::read_to_string(root.join("LunaV9.exe")).unwrap(), "LunaV9.exe");
        std::fs::remove_dir_all(&root).unwrap();
    }
}
