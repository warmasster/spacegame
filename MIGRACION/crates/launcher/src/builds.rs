//! The builds of the game next to the launcher: which there are (told by their file names, here
//! and among the ones put away in `versiones_antiguas`) and which one is started for what the
//! player chose. The game is SELENE; it was called Luna up to its V35, whose builds keep the
//! names they were given: both are found, and listed together by their version.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The game's name, as the player reads it. (It is said here and nowhere else.)
pub const GAME: &str = "SELENE";
/// What a build's file name starts with, before its version: the game's name and, of the builds
/// made when it was called Luna, that one. The first is the one of the builds made now.
const PREFIXES: [&str; 2] = ["SeleneV", "LunaV"];
/// The first version whose builds bear the game's name of now.
pub const RENAMED_SINCE: u32 = 36;
/// What tells the game's folder from any other.
pub const ASSETS: &str = "assets/defs/system.jsonc";
/// Where the camera scripts are, from the game's folder.
pub const SCRIPTS: &str = "tools/camara";
/// Where the versions put away are, from the game's folder: they are still builds, and can be
/// played from there or brought back.
pub const ARCHIVE: &str = "versiones_antiguas";

/// What a build is for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Edition {
    /// `LunaV<N>_demo.exe`: the game as it is played.
    #[default]
    Demo,
    /// `LunaV<N>_multiplayer.exe`: the game played with others, through a server.
    Multiplayer,
    /// `LunaV<N>_debug.exe`: free flight, the ship editor, the inspector.
    Debug,
    /// `LunaV<N>.exe`: from before there were editions.
    Old,
}

/// The first version made in two editions (demo and debug), and the first of which a multiplayer
/// build is made.
pub const EDITIONS_SINCE: u32 = 22;
pub const MULTIPLAYER_SINCE: u32 = 35;

impl Edition {
    /// The three there are to choose from, in the order of their cards and of their keys (1, 2, 3).
    pub const CARDS: [Edition; 3] = [Edition::Demo, Edition::Debug, Edition::Multiplayer];

    /// As it is named on its card and on the big button.
    pub fn name(self) -> &'static str {
        match self {
            Edition::Demo => "DEMO",
            Edition::Multiplayer => "MULTIJUGADOR",
            Edition::Debug => "DEBUG",
            Edition::Old => "ANTIGUA",
        }
    }

    /// What it is called in the middle of a line ("solo demo", "sin multijugador").
    pub fn word(self) -> &'static str {
        match self {
            Edition::Demo => "demo",
            Edition::Multiplayer => "multijugador",
            Edition::Debug => "debug",
            Edition::Old => "antigua",
        }
    }

    /// As it is written in `launcher.json`.
    pub fn id(self) -> &'static str {
        match self {
            Edition::Demo => "demo",
            Edition::Multiplayer => "multiplayer",
            Edition::Debug => "debug",
            Edition::Old => "old",
        }
    }

    pub fn of(id: &str) -> Option<Edition> {
        [Edition::Demo, Edition::Multiplayer, Edition::Debug, Edition::Old].into_iter().find(|e| e.id() == id)
    }

    /// What it is, in the line under its name on its card.
    pub fn says(self) -> &'static str {
        match self {
            Edition::Demo => "El juego tal como se juega.",
            Edition::Debug => "Con herramientas de desarrollo: vuelo libre, catálogo, editor…",
            Edition::Multiplayer => "La demo con otros jugadores.",
            Edition::Old => "De antes de que hubiera ediciones.",
        }
    }

    /// The first version of which a build of it was made.
    pub fn since(self) -> u32 {
        match self {
            Edition::Demo | Edition::Debug => EDITIONS_SINCE,
            Edition::Multiplayer => MULTIPLAYER_SINCE,
            Edition::Old => 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Build {
    pub version: u32,
    pub edition: Edition,
    /// Its file, as it is named in the folder.
    pub file: String,
    /// It is not in the game's folder but put away in `ARCHIVE`.
    pub archived: bool,
}

impl Build {
    /// Its file from the game's folder: its name, or its name in the archive.
    pub fn path(&self) -> String {
        if self.archived { format!("{ARCHIVE}/{}", self.file) } else { self.file.clone() }
    }

    /// As it is told: "V35 · DEMO".
    pub fn name(&self) -> String {
        format!("V{} · {}", self.version, self.edition.name())
    }
}

/// The name the build of that version and edition goes by: `SeleneV36_demo.exe`; of a version
/// from before the game was renamed, `LunaV35_demo.exe`; of one from before there were editions,
/// `LunaV21.exe`.
pub fn file_name(version: u32, edition: Edition) -> String {
    let prefix = PREFIXES[usize::from(version < RENAMED_SINCE)];
    match edition {
        Edition::Old => format!("{prefix}{version}.exe"),
        edition => format!("{prefix}{version}_{}.exe", edition.id()),
    }
}

/// The name of a build of any version, as it is told to whoever looks for one
/// ("SeleneV…_demo.exe").
pub fn any_name() -> String {
    format!("{}…_demo.exe", PREFIXES[0])
}

/// Whether a build's file bears the game's name of now (not the one it had).
fn renamed(file: &str) -> bool {
    file.get(..PREFIXES[0].len()).is_some_and(|head| head.eq_ignore_ascii_case(PREFIXES[0]))
}

/// The build a file name is, if it is one: `SeleneV<N>_demo.exe`, `SeleneV<N>_multiplayer.exe`,
/// `SeleneV<N>_debug.exe`, `SeleneV<N>.exe`; or the same with the name the game had (`LunaV<N>…`).
pub fn parse(name: &str) -> Option<Build> {
    let lower = name.to_ascii_lowercase();
    let stem = lower.strip_suffix(".exe")?;
    let stem = PREFIXES.iter().find_map(|prefix| stem.strip_prefix(&prefix.to_ascii_lowercase()))?;
    let digits = stem.bytes().take_while(u8::is_ascii_digit).count();
    let version = stem[..digits].parse().ok()?;
    let edition = match &stem[digits..] {
        "_demo" => Edition::Demo,
        "_multiplayer" => Edition::Multiplayer,
        "_debug" => Edition::Debug,
        "" => Edition::Old,
        _ => return None,
    };
    Some(Build { version, edition, file: name.to_string(), archived: false })
}

/// The builds among `names`, best first: those with an edition from the newest version down (of a
/// version, its demo, then its multiplayer, then its debug: any of the three may be missing), then
/// the old ones, newest first.
pub fn discover<S: AsRef<str>>(names: &[S]) -> Vec<Build> {
    discover_both(names, &[] as &[&str])
}

/// The builds among the names of the game's folder and among those of its archive, in the order
/// of `discover`. A build that is in both places is the one of the game's folder; one that is
/// there under both of the game's names is the one with the name of now.
pub fn discover_both<S: AsRef<str>, T: AsRef<str>>(here: &[S], archived: &[T]) -> Vec<Build> {
    let away = archived.iter().filter_map(|n| parse(n.as_ref())).map(|b| Build { archived: true, ..b });
    let mut builds: Vec<Build> = here.iter().filter_map(|n| parse(n.as_ref())).chain(away).collect();
    builds.sort_by_key(|b| (b.edition == Edition::Old, std::cmp::Reverse(b.version), b.edition, b.archived, !renamed(&b.file)));
    builds.dedup_by(|a, b| a.version == b.version && a.edition == b.edition);
    builds
}

/// The editions a version has, in the order of `discover`.
pub fn editions(builds: &[Build], version: u32) -> Vec<Edition> {
    builds.iter().filter(|b| b.version == version).map(|b| b.edition).collect()
}

/// The build to start for what was asked. A version by its number: that one, in the edition asked
/// or, if it lacks it, in the first it has (demo, multiplayer, debug). No version (or one that is
/// gone): the newest build of the edition asked ("the most recent" is told apart for each edition)
/// or, if there is none of it, the best there is.
pub fn pick(builds: &[Build], version: Option<u32>, edition: Edition) -> Option<&Build> {
    let Some(version) = version.filter(|v| builds.iter().any(|b| b.version == *v)) else {
        return builds.iter().find(|b| b.edition == edition).or_else(|| builds.first());
    };
    let mut of = builds.iter().filter(|b| b.version == version);
    of.clone().find(|b| b.edition == edition).or_else(|| of.next())
}

/// The game's folder, from where the launcher is: there, if it has builds; else the nearest folder
/// from there up that holds the game's assets (the launcher run from `target/.../release`); else there.
pub fn locate(start: &Path, has_builds: impl Fn(&Path) -> bool, has_assets: impl Fn(&Path) -> bool) -> PathBuf {
    if has_builds(start) {
        return start.to_path_buf();
    }
    start.ancestors().find(|d| has_assets(d)).unwrap_or(start).to_path_buf()
}

/// The names of the files of a folder (none if it cannot be read).
pub fn names(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    entries.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_file())).filter_map(|e| e.file_name().into_string().ok()).collect()
}

/// The folder of the launcher's own file.
pub fn home() -> PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_else(|| PathBuf::from("."))
}

/// The game's folder, from the launcher's.
pub fn root(home: &Path) -> PathBuf {
    locate(home, |d| !discover(&names(d)).is_empty(), |d| d.join(ASSETS).exists())
}

/// The builds of a game's folder: the ones in it and the ones put away.
pub fn find(root: &Path) -> Vec<Build> {
    discover_both(&names(root), &names(&root.join(ARCHIVE)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(builds: &[Build]) -> Vec<&str> {
        builds.iter().map(|b| b.file.as_str()).collect()
    }

    /// The versions there are, in the order of `discover`, each once.
    fn versions(builds: &[Build]) -> Vec<u32> {
        let mut seen = Vec::new();
        for b in builds {
            if !seen.contains(&b.version) {
                seen.push(b.version);
            }
        }
        seen
    }

    #[test]
    fn a_name_is_a_build_only_in_its_four_shapes() {
        assert_eq!(parse("LunaV33_demo.exe"), Some(Build { version: 33, edition: Edition::Demo, file: "LunaV33_demo.exe".into(), archived: false }));
        assert_eq!(parse("LunaV33_debug.exe").map(|b| b.edition), Some(Edition::Debug));
        assert_eq!(parse("LunaV35_multiplayer.exe"), Some(Build { version: 35, edition: Edition::Multiplayer, file: "LunaV35_multiplayer.exe".into(), archived: false }));
        assert_eq!(parse("LunaV9.exe").map(|b| (b.version, b.edition)), Some((9, Edition::Old)));
        assert_eq!(parse("lunav34_DEMO.EXE").map(|b| (b.version, b.edition)), Some((34, Edition::Demo)));
        assert_eq!(parse("LUNAV36_Multiplayer.exe").map(|b| (b.version, b.edition)), Some((36, Edition::Multiplayer)));
        assert_eq!(parse("SeleneV36_demo.exe"), Some(Build { version: 36, edition: Edition::Demo, file: "SeleneV36_demo.exe".into(), archived: false }));
        assert_eq!(parse("SeleneV36_multiplayer.exe").map(|b| (b.version, b.edition)), Some((36, Edition::Multiplayer)));
        assert_eq!(parse("selenev40_DEBUG.EXE").map(|b| (b.version, b.edition)), Some((40, Edition::Debug)));
        assert_eq!(parse("SeleneV36.exe").map(|b| (b.version, b.edition)), Some((36, Edition::Old)));
        for no in [
            "Selene.exe",
            "SeleneLauncher.exe",
            "SeleneLauncher_nuevo.exe",
            "SeleneServidor.exe",
            "SeleneV.exe",
            "SeleneV_demo.exe",
            "SeleneV36_demo",
            "SeleneV36demo.exe",
            "SeleneV36_release.exe",
            "XSeleneV36_demo.exe",
            "SelenaV36_demo.exe",
            "Luna.exe",
            "Luna_nueva.exe",
            "LunaMCP.exe",
            "LunaLauncher.exe",
            "LunaServidor.exe",
            "LunaV.exe",
            "LunaV_demo.exe",
            "LunaV_multiplayer.exe",
            "LunaV33_demo",
            "LunaV33_demo.exe.bak",
            "LunaV33_release.exe",
            "LunaV33demo.exe",
            "LunaV35multiplayer.exe",
            "LunaV35_multijugador.exe",
            "LunaV35_multiplayer_demo.exe",
            "XLunaV33_demo.exe",
            "LunaV99999999999_demo.exe",
            "LEEME.txt",
        ] {
            assert_eq!(parse(no), None, "{no}");
        }
    }

    #[test]
    fn builds_are_ordered_newest_first_demo_first_old_last() {
        let names = ["LunaV9.exe", "LunaV33_debug.exe", "Luna.exe", "LunaV21.exe", "LunaV29_demo.exe", "LunaV33_demo.exe", "LunaV2.exe", "LunaV29_debug.exe", "LEEME.txt", "LunaV34_debug.exe", "LunaMCP.exe"];
        let builds = discover(&names);
        assert_eq!(files(&builds), ["LunaV34_debug.exe", "LunaV33_demo.exe", "LunaV33_debug.exe", "LunaV29_demo.exe", "LunaV29_debug.exe", "LunaV21.exe", "LunaV9.exe", "LunaV2.exe"]);
        assert_eq!(versions(&builds), [34, 33, 29, 21, 9, 2]);
    }

    #[test]
    fn of_a_version_the_demo_comes_first_then_the_multiplayer_then_the_debug() {
        let names = ["LunaV35_debug.exe", "LunaV21.exe", "LunaV36_multiplayer.exe", "LunaV35_multiplayer.exe", "LunaV34_debug.exe", "LunaV35_demo.exe", "LunaV34_demo.exe", "LunaV37_debug.exe", "LunaV37_multiplayer.exe", "servidor.jsonc"];
        let builds = discover(&names);
        assert_eq!(files(&builds), ["LunaV37_multiplayer.exe", "LunaV37_debug.exe", "LunaV36_multiplayer.exe", "LunaV35_demo.exe", "LunaV35_multiplayer.exe", "LunaV35_debug.exe", "LunaV34_demo.exe", "LunaV34_debug.exe", "LunaV21.exe"]);
        assert_eq!(versions(&builds), [37, 36, 35, 34, 21]);
    }

    #[test]
    fn the_newest_multiplayer_is_the_newest_version_that_has_one() {
        let builds = discover(&["LunaV21.exe", "LunaV37_debug.exe", "LunaV36_demo.exe", "LunaV36_debug.exe", "LunaV35_demo.exe", "LunaV35_multiplayer.exe", "LunaV35_debug.exe"]);
        let file = |version, edition| pick(&builds, version, edition).map(|b| b.file.as_str());
        assert_eq!(file(None, Edition::Multiplayer), Some("LunaV35_multiplayer.exe"));
        assert_eq!(file(None, Edition::Demo), Some("LunaV36_demo.exe"));
        assert_eq!(file(None, Edition::Debug), Some("LunaV37_debug.exe"));
        assert_eq!(file(Some(35), Edition::Multiplayer), Some("LunaV35_multiplayer.exe"));
        // a version without it starts in the first edition it has
        assert_eq!(file(Some(36), Edition::Multiplayer), Some("LunaV36_demo.exe"));
        assert_eq!(file(Some(37), Edition::Multiplayer), Some("LunaV37_debug.exe"));
        // and with no multiplayer build at all, the most recent is the best there is
        let none = discover(&["LunaV21.exe", "LunaV34_demo.exe", "LunaV34_debug.exe"]);
        assert_eq!(pick(&none, None, Edition::Multiplayer).map(|b| b.file.as_str()), Some("LunaV34_demo.exe"));
        // a version that only has the multiplayer build gives it whatever is asked
        let only = discover(&["LunaV35_multiplayer.exe", "LunaV34_demo.exe"]);
        assert_eq!(pick(&only, Some(35), Edition::Demo).map(|b| b.file.as_str()), Some("LunaV35_multiplayer.exe"));
        assert_eq!(pick(&only, None, Edition::Demo).map(|b| b.file.as_str()), Some("LunaV34_demo.exe"));
        assert_eq!(pick(&only, None, Edition::Debug).map(|b| b.file.as_str()), Some("LunaV35_multiplayer.exe"));
    }

    #[test]
    fn the_same_build_twice_counts_once() {
        let builds = discover(&["LunaV30_demo.exe", "lunav30_demo.exe", "LunaV30.exe"]);
        assert_eq!(builds.len(), 2);
        assert_eq!(versions(&builds), [30]);
    }

    #[test]
    fn the_default_is_the_newest_demo() {
        let builds = discover(&["LunaV21.exe", "LunaV33_debug.exe", "LunaV33_demo.exe", "LunaV32_demo.exe", "LunaV32_debug.exe"]);
        assert_eq!(pick(&builds, None, Edition::Demo).map(|b| b.file.as_str()), Some("LunaV33_demo.exe"));
        assert_eq!(pick(&builds, None, Edition::Debug).map(|b| b.file.as_str()), Some("LunaV33_debug.exe"));
        assert_eq!(pick(&builds, Some(32), Edition::Debug).map(|b| b.file.as_str()), Some("LunaV32_debug.exe"));
    }

    #[test]
    fn a_version_asked_by_number_without_the_edition_gives_the_one_it_has() {
        let builds = discover(&["LunaV21.exe", "LunaV34_debug.exe", "LunaV33_demo.exe"]);
        assert_eq!(pick(&builds, Some(34), Edition::Demo).map(|b| b.file.as_str()), Some("LunaV34_debug.exe"));
        assert_eq!(pick(&builds, Some(33), Edition::Debug).map(|b| b.file.as_str()), Some("LunaV33_demo.exe"));
        assert_eq!(pick(&builds, Some(21), Edition::Demo).map(|b| b.file.as_str()), Some("LunaV21.exe"));
    }

    #[test]
    fn the_newest_is_the_newest_of_the_edition_asked() {
        // (a version of which only the debug build has been made yet is not what "play" starts)
        let builds = discover(&["LunaV21.exe", "LunaV34_debug.exe", "LunaV33_demo.exe", "LunaV33_debug.exe"]);
        assert_eq!(pick(&builds, None, Edition::Demo).map(|b| b.file.as_str()), Some("LunaV33_demo.exe"));
        assert_eq!(pick(&builds, None, Edition::Debug).map(|b| b.file.as_str()), Some("LunaV34_debug.exe"));
        let no_demo = discover(&["LunaV21.exe", "LunaV34_debug.exe", "LunaV33_debug.exe"]);
        assert_eq!(pick(&no_demo, None, Edition::Demo).map(|b| b.file.as_str()), Some("LunaV34_debug.exe"));
    }

    #[test]
    fn a_version_that_is_gone_gives_the_newest_and_nothing_gives_nothing() {
        let builds = discover(&["LunaV33_demo.exe", "LunaV32_demo.exe"]);
        assert_eq!(pick(&builds, Some(40), Edition::Demo).map(|b| b.version), Some(33));
        assert_eq!(pick(&[], None, Edition::Demo), None);
        let old = discover(&["LunaV3.exe", "LunaV12.exe"]);
        assert_eq!(pick(&old, None, Edition::Demo).map(|b| b.file.as_str()), Some("LunaV12.exe"));
    }

    #[test]
    fn the_builds_of_both_names_are_listed_together_by_their_version() {
        let names =
            ["LunaV35_debug.exe", "SeleneV36_demo.exe", "LunaV21.exe", "SeleneV37_debug.exe", "LunaV35_demo.exe", "SeleneV36_multiplayer.exe", "LunaV35_multiplayer.exe", "SeleneV36_debug.exe", "SeleneLauncher.exe", "LunaLauncher.exe", "LunaMCP.exe"];
        let builds = discover(&names);
        assert_eq!(files(&builds), ["SeleneV37_debug.exe", "SeleneV36_demo.exe", "SeleneV36_multiplayer.exe", "SeleneV36_debug.exe", "LunaV35_demo.exe", "LunaV35_multiplayer.exe", "LunaV35_debug.exe", "LunaV21.exe"]);
        assert_eq!(versions(&builds), [37, 36, 35, 21]);
        assert_eq!(pick(&builds, None, Edition::Demo).map(Build::name).as_deref(), Some("V36 · DEMO"));
        assert_eq!(pick(&builds, Some(35), Edition::Multiplayer).map(|b| b.file.as_str()), Some("LunaV35_multiplayer.exe"));
        // a build that is there under both names is the one with the name of now, wherever each is
        let both = discover_both(&["LunaV36_demo.exe", "SeleneV36_demo.exe", "LunaV36_debug.exe"], &["SeleneV36_debug.exe", "SeleneV20.exe"]);
        assert_eq!(both.iter().map(Build::path).collect::<Vec<_>>(), ["SeleneV36_demo.exe", "LunaV36_debug.exe", "versiones_antiguas/SeleneV20.exe"]);
    }

    #[test]
    fn a_build_s_name_is_the_game_s_of_its_time() {
        assert_eq!(file_name(36, Edition::Demo), "SeleneV36_demo.exe");
        assert_eq!(file_name(40, Edition::Multiplayer), "SeleneV40_multiplayer.exe");
        assert_eq!(file_name(35, Edition::Debug), "LunaV35_debug.exe");
        assert_eq!(file_name(21, Edition::Old), "LunaV21.exe");
        assert_eq!(any_name(), "SeleneV…_demo.exe");
        for (version, edition) in [(36, Edition::Demo), (35, Edition::Multiplayer), (22, Edition::Debug), (9, Edition::Old), (36, Edition::Old)] {
            assert_eq!(parse(&file_name(version, edition)).map(|b| (b.version, b.edition)), Some((version, edition)));
        }
        assert_eq!(GAME, "SELENE");
    }

    #[test]
    fn the_builds_put_away_are_builds_too_and_the_one_at_hand_comes_first() {
        let builds = discover_both(&["LunaV35_demo.exe", "LunaV34_demo.exe", "LunaV21.exe", "LunaLauncher.exe"], &["LunaV34_demo.exe", "LunaV34_debug.exe", "LunaV9.exe", "LEEME.txt", "Luna.exe"]);
        let told: Vec<(String, bool)> = builds.iter().map(|b| (b.path(), b.archived)).collect();
        let want = [("LunaV35_demo.exe", false), ("LunaV34_demo.exe", false), ("versiones_antiguas/LunaV34_debug.exe", true), ("LunaV21.exe", false), ("versiones_antiguas/LunaV9.exe", true)];
        assert_eq!(told, want.map(|(path, archived)| (path.to_string(), archived)));
        assert_eq!(versions(&builds), [35, 34, 21, 9]);
        assert_eq!(editions(&builds, 34), [Edition::Demo, Edition::Debug]);
        // one put away is started like any other
        assert_eq!(pick(&builds, Some(34), Edition::Debug).map(Build::path).as_deref(), Some("versiones_antiguas/LunaV34_debug.exe"));
        assert_eq!(pick(&builds, Some(9), Edition::Demo).map(Build::name).as_deref(), Some("V9 · ANTIGUA"));
        assert!(discover(&["LunaV35_demo.exe"]).iter().all(|b| !b.archived));
    }

    #[test]
    fn an_edition_has_its_names_and_its_place_among_the_cards() {
        assert_eq!(Edition::CARDS.map(Edition::name), ["DEMO", "DEBUG", "MULTIJUGADOR"]);
        assert_eq!(Edition::CARDS.map(Edition::since), [22, 22, 35]);
        for edition in [Edition::Demo, Edition::Multiplayer, Edition::Debug, Edition::Old] {
            assert_eq!(Edition::of(edition.id()), Some(edition));
            assert_eq!(serde_json::to_string(&edition).unwrap(), format!("\"{}\"", edition.id()));
            assert!(!edition.says().is_empty());
        }
        assert_eq!(Edition::of("release"), None);
        assert_eq!(Build { version: 36, edition: Edition::Debug, file: "LunaV36_debug.exe".into(), archived: false }.name(), "V36 · DEBUG");
    }

    #[test]
    fn the_folder_is_where_the_builds_are_or_the_assets_above() {
        let here = Path::new("juego/target/lanzador/release");
        assert_eq!(locate(here, |_| true, |_| false), here);
        assert_eq!(locate(here, |_| false, |d| d == Path::new("juego")), Path::new("juego"));
        assert_eq!(locate(here, |_| false, |d| d == here), here);
        assert_eq!(locate(here, |_| false, |_| false), here);
    }
}
