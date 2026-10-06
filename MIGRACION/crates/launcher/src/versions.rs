//! The versions there are, each told whole: its number, the editions it has, when it was made and
//! what it takes on the disk, whether it is put away, and what it brought. It is what the list of
//! versions shows; it is put together once, when the game's folder is looked at.
use crate::{
    builds::{Build, Edition},
    news::Section,
};

/// What the disk says of a file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    pub size: u64,
    /// When it was last written (seconds since 1970).
    pub modified: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Version {
    pub number: u32,
    /// The editions it has, in the order they are started in.
    pub editions: Vec<Edition>,
    /// Its files, from the game's folder, each with its size and date.
    pub files: Vec<(String, Facts)>,
    /// All of it is put away; and some of it is.
    pub archived: bool,
    pub some_archived: bool,
    /// What its files take, and when the newest of them was made.
    pub size: u64,
    pub modified: u64,
    /// What it brought, in a line; and where its news are among the sections (none: the manual
    /// does not tell).
    pub headline: String,
    pub section: Option<usize>,
}

impl Version {
    pub fn has(&self, edition: Edition) -> bool {
        self.editions.contains(&edition)
    }

    /// It is from before there were editions.
    pub fn old(&self) -> bool {
        self.editions == [Edition::Old]
    }
}

/// The versions of these builds, in their order (the newest first), each with what `facts` says
/// of its files and what `sections` tell it brought.
pub fn catalog(builds: &[Build], facts: impl Fn(&Build) -> Facts, sections: &[Section]) -> Vec<Version> {
    let mut versions: Vec<Version> = Vec::new();
    for build in builds {
        let at = versions.iter().position(|v| v.number == build.version).unwrap_or_else(|| {
            let section = sections.iter().position(|s| s.version == build.version);
            versions.push(Version { number: build.version, archived: true, headline: section.map(|n| sections[n].headline.clone()).unwrap_or_default(), section, ..Version::default() });
            versions.len() - 1
        });
        let (version, file) = (&mut versions[at], facts(build));
        version.editions.push(build.edition);
        version.files.push((build.path(), file));
        version.archived &= build.archived;
        version.some_archived |= build.archived;
        version.size += file.size;
        version.modified = version.modified.max(file.modified);
    }
    versions
}

/// What a set of versions takes on the disk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Disk {
    pub versions: usize,
    pub files: usize,
    pub bytes: u64,
}

/// What the versions at hand take, and what the ones put away do (a version half put away counts
/// in both, each file where it is).
pub fn disk(versions: &[Version], archive: &str) -> (Disk, Disk) {
    let mut both = (Disk::default(), Disk::default());
    for version in versions {
        let (mut here, mut away) = (false, false);
        for (path, facts) in &version.files {
            let is_away = path.starts_with(archive) && path[archive.len()..].starts_with('/');
            let disk = if is_away { &mut both.1 } else { &mut both.0 };
            disk.files += 1;
            disk.bytes += facts.size;
            here |= !is_away;
            away |= is_away;
        }
        both.0.versions += usize::from(here);
        both.1.versions += usize::from(away);
    }
    both
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{builds, news};

    const NAMES: [&str; 8] = ["LunaV36_demo.exe", "LunaV36_debug.exe", "LunaV36_multiplayer.exe", "LunaV35_demo.exe", "LunaV35_debug.exe", "LunaV34_demo.exe", "LunaV21.exe", "Luna.exe"];
    const AWAY: [&str; 3] = ["LunaV34_debug.exe", "LunaV9.exe", "LunaV2.exe"];
    const TEXT: &str = "NOVEDADES V36 (LunaV36_demo.exe):\nRED: la carga se comparte.\nTIEMPOS: el arranque dice lo que tarda.\n\nNOVEDADES V35:\nPANTALLA DE CARGA: una cabina arrancando.\n\nNOVEDADES V29:\n1: soldador-escaner.\n\nNAVES:\nmanual.\n";

    /// A megabyte for each letter of the name, made on the day of its version number.
    fn facts(build: &Build) -> Facts {
        Facts { size: build.file.len() as u64 * 1_000_000, modified: u64::from(build.version) * 86_400 }
    }

    #[test]
    fn each_version_is_told_whole() {
        let builds = builds::discover_both(&NAMES, &AWAY);
        let versions = catalog(&builds, facts, &news::read_all(TEXT));
        assert_eq!(versions.iter().map(|v| v.number).collect::<Vec<_>>(), [36, 35, 34, 21, 9, 2]);
        let v36 = &versions[0];
        assert_eq!(v36.editions, [Edition::Demo, Edition::Multiplayer, Edition::Debug]);
        assert_eq!((v36.size, v36.modified, v36.archived, v36.some_archived), (16_000_000 + 23_000_000 + 17_000_000, 36 * 86_400, false, false));
        assert_eq!((v36.headline.as_str(), v36.section), ("Red · Tiempos", Some(0)));
        assert!(v36.has(Edition::Multiplayer) && !v36.old());
        assert_eq!((versions[1].headline.as_str(), versions[1].section, versions[1].has(Edition::Multiplayer)), ("Pantalla de carga", Some(1), false));
        // one half put away: each file where it is
        let v34 = &versions[2];
        assert_eq!(v34.files.iter().map(|(path, _)| path.as_str()).collect::<Vec<_>>(), ["LunaV34_demo.exe", "versiones_antiguas/LunaV34_debug.exe"]);
        assert_eq!((v34.archived, v34.some_archived, v34.headline.as_str(), v34.section), (false, true, "", None));
        // the old ones: one edition, and the ones put away say so
        assert!(versions[3].old() && !versions[3].archived);
        assert_eq!((versions[4].number, versions[4].archived, versions[4].some_archived, versions[4].old()), (9, true, true, true));
        assert!(catalog(&[], facts, &[]).is_empty());
    }

    #[test]
    fn what_the_versions_take_is_told_apart_for_those_put_away() {
        let versions = catalog(&builds::discover_both(&NAMES, &AWAY), facts, &[]);
        let (here, away) = disk(&versions, builds::ARCHIVE);
        assert_eq!(here, Disk { versions: 4, files: 7, bytes: (16 + 23 + 17 + 16 + 17 + 16 + 11) * 1_000_000 });
        assert_eq!(away, Disk { versions: 3, files: 3, bytes: (17 + 10 + 10) * 1_000_000 });
        assert_eq!(disk(&[], builds::ARCHIVE), (Disk::default(), Disk::default()));
    }
}
