//! The dedicated server that may be beside the builds, in `servidores/`: whether it is there, the
//! port its config asks for, and the addresses its players reach it at.
use std::{
    net::{IpAddr, Ipv4Addr, ToSocketAddrs},
    path::{Path, PathBuf},
};

/// The server's folder, from the game's folder.
pub const FOLDER: &str = "servidores";
/// The names the server's program goes by, in its folder: the one of now, and the one it had
/// when the game was called Luna (the first that is there is the one started).
pub const PROGRAMS: [&str; 2] = ["SeleneServidor.exe", "LunaServidor.exe"];
/// Its config, in its folder: JSON with `//` comments and trailing commas.
pub const CONFIG: &str = "servidor.jsonc";
/// Its log, in its folder: it holds it open for as long as it is up.
pub const LOG: &str = "servidor.log";
/// The UDP port a server listens on unless its config says another.
pub const PORT: u16 = 47600;
/// This same machine, as an address.
pub const LOCALHOST: &str = "127.0.0.1";
/// How many addresses of this machine are told at most.
const ADDRESSES: usize = 2;

/// What there is of the server at a moment.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Server {
    /// Its folder, be it there or not.
    pub folder: PathBuf,
    /// The folder is there.
    pub has_folder: bool,
    /// The program is there: the server can be started.
    pub ready: bool,
    /// The program's name: the one that is there, or the one it should have.
    pub program: &'static str,
    /// The port it listens on: its config's, or `PORT`.
    pub port: u16,
    /// The name it shows to whoever joins, if its config gives it one.
    pub name: String,
}

impl Server {
    /// The server of the game's folder `root`, as it is now.
    pub fn look(root: &Path) -> Server {
        let folder = root.join(FOLDER);
        let config = std::fs::read(folder.join(CONFIG)).ok().map(|bytes| String::from_utf8_lossy(&bytes).into_owned()).unwrap_or_default();
        let program = PROGRAMS.into_iter().find(|program| folder.join(program).is_file());
        Server { has_folder: folder.is_dir(), ready: program.is_some(), program: program.unwrap_or(PROGRAMS[0]), port: port(&config).unwrap_or(PORT), name: name(&config).unwrap_or_default(), folder }
    }
}

/// Whether the server of that folder is up, told without touching it: while it runs it holds its
/// log open, so nobody else can have that file for itself alone. (A server that is not up, or one
/// that kept no log, is not seen.)
#[cfg(windows)]
pub fn up(folder: &Path) -> bool {
    use std::os::windows::fs::OpenOptionsExt;
    /// The system's "another process has this file" (ERROR_SHARING_VIOLATION).
    const HELD: i32 = 32;
    std::fs::OpenOptions::new().read(true).share_mode(0).open(folder.join(LOG)).is_err_and(|e| e.raw_os_error() == Some(HELD))
}

#[cfg(not(windows))]
pub fn up(_folder: &Path) -> bool {
    false
}

/// A `.jsonc` text without its comments (`//` to the end of its line, `/* */`); what stands
/// between quotes is kept as it is.
fn without_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut quoted = false;
    while let Some(c) = chars.next() {
        if quoted {
            out.push(c);
            match c {
                '\\' => out.extend(chars.next()),
                '"' => quoted = false,
                _ => {}
            }
        } else if c == '/' && chars.peek() == Some(&'/') {
            // (the end of the line stays: it parts what is before from what is after)
            while chars.next_if(|c| *c != '\n').is_some() {}
        } else if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            let mut last = ' ';
            for c in chars.by_ref() {
                if last == '*' && c == '/' {
                    break;
                }
                last = c;
            }
            out.push(' ');
        } else {
            quoted = c == '"';
            out.push(c);
        }
    }
    out
}

/// What follows the first key `key` of a config (after its colon), read leniently (the file is
/// not parsed: comments, trailing commas and whatever else it holds are let be). None if it has
/// no such key.
fn value_of(text: &str, key: &str) -> Option<String> {
    let key = format!("\"{key}\"");
    let plain = without_comments(text);
    let mut rest = plain.as_str();
    while let Some(at) = rest.find(&key) {
        rest = &rest[at + key.len()..];
        // (the same word as a value, or in a text, is followed by no colon)
        if let Some(value) = rest.trim_start().strip_prefix(':') {
            return Some(value.trim_start().to_string());
        }
    }
    None
}

/// The port a server's config asks for: the number that follows its first `"puerto"` key. None
/// if it says none, or one that is no port.
pub fn port(text: &str) -> Option<u16> {
    let value = value_of(text, "puerto")?;
    let value = value.strip_prefix('"').unwrap_or(&value).trim_start();
    let digits = value.bytes().take_while(u8::is_ascii_digit).count();
    value[..digits].parse().ok().filter(|port| *port != 0)
}

/// The most letters of a server's name that are told.
const NAME_MOST: usize = 40;

/// The name a server's config gives it: the text that follows its first `"nombre"` key. None if
/// it gives none.
pub fn name(text: &str) -> Option<String> {
    let value = value_of(text, "nombre")?;
    let mut chars = value.strip_prefix('"')?.chars();
    let mut name = String::new();
    loop {
        match chars.next()? {
            '"' => break,
            '\\' => name.push(chars.next()?),
            c => name.push(c),
        }
    }
    let name: String = name.trim().chars().filter(|c| !c.is_control()).take(NAME_MOST).collect();
    (!name.is_empty()).then_some(name)
}

/// Of a machine's addresses, the ones it has on a local network (IPv4, private), a home's
/// first: the ones another machine of that network reaches it at. `ADDRESSES` at most.
pub fn local_network(addresses: impl IntoIterator<Item = IpAddr>) -> Vec<Ipv4Addr> {
    let mut found = Vec::new();
    for address in addresses {
        if let IpAddr::V4(v4) = address
            && v4.is_private()
            && !found.contains(&v4)
        {
            found.push(v4);
        }
    }
    // (192.168 is what a home router gives; 172.16 is mostly virtual machines' own networks)
    found.sort_by_key(|v4| match v4.octets() {
        [192, ..] => 0,
        [10, ..] => 1,
        _ => 2,
    });
    found.truncate(ADDRESSES);
    found
}

/// The address this machine goes out by: the one a socket would send from (nothing is sent to
/// find it out: the socket only notes where it would). It is the one the server tells of itself.
fn route() -> Option<IpAddr> {
    let probe = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    probe.connect("192.0.2.1:9").ok()?;
    probe.local_addr().ok().map(|a| a.ip())
}

/// The addresses to tell of a machine that goes out by `route` and has `named` under its name:
/// the one it goes out by first, if it is of a local network.
fn told(route: Option<IpAddr>, named: impl IntoIterator<Item = IpAddr>) -> Vec<Ipv4Addr> {
    let mut found = local_network(route);
    for other in local_network(named) {
        if !found.contains(&other) && found.len() < ADDRESSES {
            found.push(other);
        }
    }
    found
}

/// This machine's addresses on its local network, asked of the system (nothing is sent anywhere):
/// the one it goes out by and the ones under its own name. None if it does not say.
pub fn lan() -> Vec<Ipv4Addr> {
    let named = std::env::var("COMPUTERNAME").ok().and_then(|name| (name.as_str(), 0).to_socket_addrs().ok()).map(|found| found.map(|a| a.ip()).collect::<Vec<_>>()).unwrap_or_default();
    told(route(), named)
}

/// The addresses a server on this machine at `port` is reached at: by the others on its network
/// (none: this machine's address there is not known) and from this same machine.
pub fn reach(lan: &[Ipv4Addr], port: u16) -> (Vec<String>, String) {
    (lan.iter().map(|ip| format!("{ip}:{port}")).collect(), format!("{LOCALHOST}:{port}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_port_is_read_from_a_config_with_comments_and_trailing_commas() {
        let config = "// El servidor de LUNA\n{\n  \"nombre\": \"Base Tranquilidad\", // lo que ven los jugadores\n  /* el puerto UDP\n     \"puerto\": 1, */\n  \"puerto\": 47611, // por defecto 47600\n  \"jugadores\": 16,\n}\n";
        assert_eq!(port(config), Some(47611));
        assert_eq!(port("{\"puerto\":50000}"), Some(50000));
        assert_eq!(port("{ \"red\": { \"puerto\" : 1, }, }"), Some(1));
        assert_eq!(port("{\n  \"puerto\"\n    :\n    65535,\n}"), Some(65535));
        assert_eq!(port("{ \"puerto\": \"47700\" }"), Some(47700));
    }

    #[test]
    fn a_config_that_says_no_port_gives_none() {
        assert_eq!(port(""), None);
        assert_eq!(port("{ \"jugadores\": 16 }"), None);
        assert_eq!(port("{ // \"puerto\": 47611\n}"), None);
        assert_eq!(port("{ \"puerto\": 0 }"), None);
        assert_eq!(port("{ \"puerto\": 70000 }"), None);
        assert_eq!(port("{ \"puerto\": -5 }"), None);
        assert_eq!(port("{ \"puerto\": null }"), None);
        assert_eq!(port("{ \"puerto\" }"), None);
        assert_eq!(port("esto no es json"), None);
    }

    #[test]
    fn the_name_is_read_from_a_config_too() {
        let config = "// El servidor\n{\n  \"puerto\": 47600,\n  // Nombre del servidor: se le enseña a quien entra. También: --nombre \"La Base\"\n  \"nombre\": \"Servidor de Selene\",\n  \"max_jugadores\": 16,\n}\n";
        assert_eq!(name(config).as_deref(), Some("Servidor de Selene"));
        assert_eq!(name("{ \"nombre\" : \"  La \\\"Base\\\" de Ana  \" }").as_deref(), Some("La \"Base\" de Ana"));
        assert_eq!(name(&format!("{{ \"nombre\": \"{}\" }}", "ñ".repeat(60))).map(|n| n.chars().count()), Some(40));
        for none in ["", "{ \"puerto\": 1 }", "{ \"nombre\": 7 }", "{ \"nombre\": \"\" }", "{ \"nombre\": \"sin cerrar }", "{ // \"nombre\": \"comentado\"\n}"] {
            assert_eq!(name(none), None, "{none}");
        }
    }

    #[test]
    fn what_is_between_quotes_is_no_comment_and_a_value_is_no_key() {
        assert_eq!(port("{ \"web\": \"http://luna.example/\", \"puerto\": 47601 }"), Some(47601));
        assert_eq!(port("{ \"dice\": \"un \\\"texto\\\" // con barras\", \"puerto\": 47602 }"), Some(47602));
        assert_eq!(port("{ \"clave\": \"puerto\", \"puerto\": 47603 }"), Some(47603));
        assert_eq!(without_comments("a // b\nc /* d\ne */ f \"g // h\""), "a \nc   f \"g // h\"");
    }

    #[test]
    fn the_server_of_a_folder_is_what_is_in_it() {
        let root = std::env::temp_dir().join(format!("luna-launcher-servidor-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        // nothing there: it cannot be started, and its port is the usual one
        let none = Server::look(&root);
        assert_eq!(none, Server { folder: root.join(FOLDER), has_folder: false, ready: false, program: "SeleneServidor.exe", port: PORT, name: String::new() });
        // its config alone: the folder is there, with its port, but there is nothing to start
        std::fs::create_dir_all(root.join(FOLDER)).unwrap();
        std::fs::write(root.join(FOLDER).join(CONFIG), "{\n  // otro puerto\n  \"puerto\": 47999,\n  \"nombre\": \"La Base\",\n}\n").unwrap();
        assert_eq!(Server::look(&root), Server { folder: root.join(FOLDER), has_folder: true, ready: false, program: "SeleneServidor.exe", port: 47999, name: "La Base".to_string() });
        // (an empty file stands for the program: it is only looked for, never started)
        // the one with the name the game had is found; and the one with the name of now, before it
        std::fs::write(root.join(FOLDER).join("LunaServidor.exe"), "").unwrap();
        assert_eq!((Server::look(&root).ready, Server::look(&root).program), (true, "LunaServidor.exe"));
        std::fs::write(root.join(FOLDER).join("SeleneServidor.exe"), "").unwrap();
        assert_eq!((Server::look(&root).ready, Server::look(&root).program), (true, "SeleneServidor.exe"));
        std::fs::write(root.join(FOLDER).join(CONFIG), "roto").unwrap();
        assert_eq!(Server::look(&root).port, PORT);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_addresses_told_are_the_private_ones_a_home_s_first() {
        let ip = |text: &str| text.parse::<IpAddr>().unwrap();
        let all = ["fe80::1", "172.28.80.1", "127.0.0.1", "169.254.10.2", "192.168.1.37", "8.8.8.8", "192.168.1.37", "10.0.0.5", "::1"].map(ip);
        assert_eq!(local_network(all), ["192.168.1.37".parse::<Ipv4Addr>().unwrap(), "10.0.0.5".parse().unwrap()]);
        assert_eq!(local_network(["172.28.80.1", "127.0.0.1"].map(ip)), ["172.28.80.1".parse::<Ipv4Addr>().unwrap()]);
        assert!(local_network(["127.0.0.1", "8.8.8.8", "::1"].map(ip)).is_empty());
    }

    #[test]
    fn the_address_this_machine_goes_out_by_is_told_first() {
        let ip = |text: &str| text.parse::<IpAddr>().unwrap();
        let v4 = |text: &str| text.parse::<Ipv4Addr>().unwrap();
        // (a virtual machines' network sorts before a home's 10.x: the route says which is the real one)
        assert_eq!(told(Some(ip("10.0.0.5")), ["192.168.56.1", "10.0.0.5", "fe80::1"].map(ip)), [v4("10.0.0.5"), v4("192.168.56.1")]);
        assert_eq!(told(Some(ip("192.168.1.67")), ["192.168.1.67"].map(ip)), [v4("192.168.1.67")]);
        // a machine that goes out by a public address, or by none, tells the ones under its name
        assert_eq!(told(Some(ip("8.8.8.8")), ["192.168.1.37"].map(ip)), [v4("192.168.1.37")]);
        assert_eq!(told(None, ["172.28.80.1", "192.168.1.37", "10.0.0.5"].map(ip)), [v4("192.168.1.37"), v4("10.0.0.5")]);
        assert!(told(None, []).is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn a_server_is_up_while_it_holds_its_log() {
        let folder = std::env::temp_dir().join(format!("luna-launcher-servidor-arriba-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        assert!(!up(&folder), "no folder");
        std::fs::create_dir_all(&folder).unwrap();
        assert!(!up(&folder), "no log");
        std::fs::write(folder.join(LOG), "Servidor en marcha\n").unwrap();
        assert!(!up(&folder), "a log nobody holds");
        // (what a server does: its log open to add to it for as long as it runs)
        let held = std::fs::OpenOptions::new().append(true).open(folder.join(LOG)).unwrap();
        assert!(up(&folder), "a log held");
        assert_eq!(std::fs::read_to_string(folder.join(LOG)).unwrap(), "Servidor en marcha\n");
        drop(held);
        assert!(!up(&folder), "let go again");
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_server_is_reached_at_its_port() {
        let lan = ["192.168.1.37".parse().unwrap()];
        assert_eq!(reach(&lan, 47611), (vec!["192.168.1.37:47611".to_string()], "127.0.0.1:47611".to_string()));
        assert_eq!(reach(&[], PORT), (Vec::new(), "127.0.0.1:47600".to_string()));
    }
}
