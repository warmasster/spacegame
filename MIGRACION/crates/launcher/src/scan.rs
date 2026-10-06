//! Looking at the game's folder: its builds (at hand and put away) with their sizes and dates,
//! what `LEEME.txt` tells each version brought, the camera scripts, the documents, the logs, the
//! ship tools' server, the game's last start; and, once, this machine's clock and system. It is
//! all done in one go on a thread of its own (`jobs`): the window is up before it is asked for
//! and never waits for it.
use crate::{
    builds::{self, Build},
    launch,
    news::{self, Section},
    system::{self, Boot},
    tools::{self, Doc, Log, Mcp, Script},
    versions::{self, Disk, Facts, Version},
    words::{self, Clock},
};
use std::path::{Path, PathBuf};

/// What was found.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scan {
    /// The game's folder.
    pub root: PathBuf,
    pub builds: Vec<Build>,
    pub versions: Vec<Version>,
    /// What the versions at hand take on the disk, and what the ones put away do.
    pub disk: (Disk, Disk),
    /// What each version brought, as the manual tells it (the newest first).
    pub sections: Vec<Section>,
    /// The manual is there; and so are the game's assets.
    pub manual: bool,
    pub assets: bool,
    pub scripts: Vec<Script>,
    pub docs: Vec<Doc>,
    pub logs: Vec<Log>,
    pub mcp: Mcp,
    pub boot: Option<Boot>,
    /// A newer launcher left beside this one (a launcher that is open cannot be written over, so
    /// a new one is put next to it under another name): its file's name.
    pub newer: Option<String>,
    pub known: Known,
    /// How long the look took (milliseconds).
    pub took_ms: u32,
}

impl Scan {
    pub fn clock(&self) -> Clock {
        self.known.clock
    }
}

/// What does not change while the launcher is open: it is asked of the system once.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Known {
    pub clock: Clock,
    /// The system, as it is told ("Windows 11 (10.0.26200.6584)").
    pub windows: Option<String>,
    /// This machine's addresses on its local network.
    pub lan: Vec<std::net::Ipv4Addr>,
}

impl Known {
    /// Asked of the system: two small programs of it, with no window.
    fn ask() -> Known {
        Known { clock: words::clock(), windows: launch::said("cmd", &["/C", "ver"]).as_deref().and_then(system::windows), lan: crate::server::lan() }
    }
}

/// The text of a file, whatever its letters; none if it is not there.
fn text(path: &Path) -> Option<String> {
    std::fs::read(path).ok().map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

/// What the disk says of a build's file.
fn facts(root: &Path, build: &Build) -> Facts {
    std::fs::metadata(root.join(build.path())).map_or(Facts::default(), |m| Facts { size: m.len(), modified: m.modified().map_or(0, words::unix) })
}

/// What a newer launcher is called when it is left beside the one that is open: its name with
/// this at its end.
const NEWER: &str = "_nuevo";

/// The launcher left beside `own` (this launcher's file) to take its place, if there is one and
/// it is newer: its file's name.
pub fn newer(own: &Path) -> Option<String> {
    let name = format!("{}{NEWER}.exe", own.file_stem()?.to_str()?);
    let written = |path: &Path| std::fs::metadata(path).ok().filter(std::fs::Metadata::is_file)?.modified().ok();
    (written(&own.with_file_name(&name))? > written(own)?).then_some(name)
}

/// Look at the game's folder `root`. What is `known` of the system from a look before is not
/// asked again.
pub fn look(root: &Path, known: Option<Known>) -> Scan {
    let started = std::time::Instant::now();
    let builds = builds::find(root);
    let manual = text(&root.join("LEEME.txt"));
    let sections = manual.as_deref().map(news::read_all).unwrap_or_default();
    let versions = versions::catalog(&builds, |b| facts(root, b), &sections);
    Scan {
        disk: versions::disk(&versions, builds::ARCHIVE),
        versions,
        builds,
        sections,
        manual: manual.is_some(),
        assets: root.join(builds::ASSETS).exists(),
        scripts: tools::scripts(&root.join(builds::SCRIPTS)),
        docs: tools::docs(&root.join(tools::DOCS)),
        logs: tools::logs(root),
        mcp: Mcp::look(root),
        boot: text(&root.join(tools::OUT).join("arranque.json")).as_deref().and_then(Boot::read),
        newer: std::env::current_exe().ok().as_deref().and_then(newer),
        known: known.unwrap_or_else(Known::ask),
        root: root.to_path_buf(),
        took_ms: started.elapsed().as_millis() as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builds::Edition;

    #[test]
    fn a_game_s_folder_is_looked_at_whole() {
        let root = std::env::temp_dir().join(format!("luna-launcher-mirar-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for dir in ["versiones_antiguas", "tools/camara", "docs", "out", "assets/defs"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        for (file, said) in [
            ("LunaV36_demo.exe", "demo"),
            ("LunaV36_debug.exe", "debug!"),
            ("LunaV35_demo.exe", "x"),
            ("versiones_antiguas/LunaV21.exe", "antigua"),
            ("LunaLauncher.exe", ""),
            ("LEEME.txt", "NOVEDADES V36 (LunaV36_demo.exe):\nRED: la carga se comparte.\n\nNOVEDADES V35:\nANDAR: un pie tras otro.\n\nNAVES:\nmanual\n"),
            ("tools/camara/carga.jsonc", "// Cargo and clamps.\n// Run: luna\n{}"),
            ("docs/NAVES.md", "# Naves\n"),
            ("out/error.log", "opción desconocida: --x\n"),
            ("out/arranque.json", "{\"defs\": 4.0, \"mundo\": 2.5}"),
            ("assets/defs/system.jsonc", "{}"),
        ] {
            std::fs::write(root.join(file), said).unwrap();
        }
        let known = Known { clock: Clock { offset: 7200 }, windows: Some("Windows 11".to_string()), lan: vec!["192.168.1.67".parse().unwrap()] };
        let scan = look(&root, Some(known.clone()));
        assert_eq!(scan.root, root);
        assert_eq!(scan.builds.iter().map(Build::path).collect::<Vec<_>>(), ["LunaV36_demo.exe", "LunaV36_debug.exe", "LunaV35_demo.exe", "versiones_antiguas/LunaV21.exe"]);
        assert_eq!(scan.versions.iter().map(|v| (v.number, v.size, v.archived, v.headline.as_str())).collect::<Vec<_>>(), [(36, 10, false, "Red"), (35, 1, false, "Andar"), (21, 7, true, "")]);
        assert!(scan.versions.iter().all(|v| v.modified > 1_700_000_000));
        assert_eq!(scan.versions[0].editions, [Edition::Demo, Edition::Debug]);
        assert_eq!((scan.disk.0.bytes, scan.disk.1.bytes), (11, 7));
        assert_eq!(scan.sections.iter().map(|s| s.version).collect::<Vec<_>>(), [36, 35]);
        assert!(scan.manual && scan.assets);
        assert_eq!(scan.scripts, [Script { name: "carga".into(), says: "Cargo and clamps.".into() }]);
        assert_eq!(scan.docs, [Doc { file: "NAVES.md".into(), title: "Naves".into() }]);
        assert_eq!(scan.logs.iter().map(|l| (l.path.as_str(), l.first.as_str())).collect::<Vec<_>>(), [("out/error.log", "opción desconocida: --x")]);
        assert_eq!(scan.boot.as_ref().map(Boot::says).as_deref(), Some("6,5 s (defs 4,0 · mundo 2,5)"));
        assert_eq!((scan.known.clone(), scan.clock(), scan.mcp), (known, Clock { offset: 7200 }, Mcp::default()));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_newer_launcher_left_beside_this_one_is_seen() {
        let dir = std::env::temp_dir().join(format!("luna-launcher-nuevo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (own, other) = (dir.join("SeleneLauncher.exe"), dir.join("SeleneLauncher_nuevo.exe"));
        let write = |path: &Path, age: u64| {
            std::fs::write(path, "").unwrap();
            std::fs::File::options().write(true).open(path).unwrap().set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(age)).unwrap();
        };
        write(&own, 3600);
        assert_eq!(newer(&own), None, "nothing beside it");
        write(&other, 60);
        assert_eq!(newer(&own).as_deref(), Some("SeleneLauncher_nuevo.exe"));
        // (one left there from before this launcher was put in its place is not newer)
        write(&other, 7200);
        assert_eq!(newer(&own), None);
        assert_eq!(newer(&dir.join("no_existe.exe")), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_folder_that_is_not_there_has_nothing_and_says_so() {
        let scan = look(Path::new("no/existe/esta/carpeta"), Some(Known::default()));
        assert_eq!(scan, Scan { root: PathBuf::from("no/existe/esta/carpeta"), took_ms: scan.took_ms, newer: scan.newer.clone(), ..Scan::default() });
    }

    /// (This asks the system for its clock and its name, as the launcher does once: two `cmd`
    /// with no window.)
    #[cfg(windows)]
    #[test]
    fn the_system_tells_its_clock_and_its_name() {
        let known = Known::ask();
        assert!(known.windows.as_deref().is_some_and(|w| w.starts_with("Windows ")), "{known:?}");
        assert!((-12 * 3600..=14 * 3600).contains(&known.clock.offset) && known.clock.offset % 900 == 0, "{known:?}");
    }
}
