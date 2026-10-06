//! The MCP server (Model Context Protocol, JSON-RPC 2.0 over stdio, one message per line) through
//! which models design ships: they read the catalog and a ship, change it with the editor's
//! operations, check it (it builds; it is tight; every room is worked from inside; the cables),
//! look at it (a picture from the game, wherever they put the camera) and save it.
//!
//! Tools: `naves`, `catalogo`, `leer_nave`, `operar`, `validar`, `foto`, `guardar`.
use crate::{Doc, Op, catalog};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub struct Server {
    /// `assets/defs`, and the game's folder (where its exe and `out/` are).
    pub defs: PathBuf,
    pub root: PathBuf,
    docs: BTreeMap<String, Doc>,
    shots: u32,
}

const PROTOCOL: &str = "2025-06-18";

fn text(t: impl Into<String>) -> Value {
    json!({ "content": [ { "type": "text", "text": t.into() } ] })
}

fn fail(t: impl Into<String>) -> Value {
    json!({ "content": [ { "type": "text", "text": t.into() } ], "isError": true })
}

fn tools() -> Value {
    let ship = json!({ "type": "string", "description": "Id de la nave (fichero assets/defs/ships/<id>.jsonc), p. ej. \"alcotan\"" });
    json!([
        { "name": "naves", "description": "Las naves que hay (sus ids) y cuáles están abiertas con cambios sin guardar.",
          "inputSchema": { "type": "object", "properties": {} } },
        { "name": "catalogo", "description": "Los componentes que se pueden poner (con su máquina y sus puertos) y las plantillas de panel. Marco de la nave: +x babor (izquierda), +y arriba, +z proa; metros; el suelo está en y = 0.",
          "inputSchema": { "type": "object", "properties": { "filtro": { "type": "string", "description": "Solo los que contengan este texto" } } } },
        { "name": "leer_nave", "description": "Resumen de una nave (compartimentos, componentes con su posición y puertos, redes, paneles), o el valor exacto en una ruta de su definición.",
          "inputSchema": { "type": "object", "properties": { "nave": ship, "ruta": { "type": "string", "description": "Ruta JSON (p. ej. /componentes/12 o /redes/elec/nodos); sin ella, el resumen" } }, "required": ["nave"] } },
        { "name": "operar", "description": "Cambia la nave con operaciones, en orden (se para en la primera que falle; nada a medias). Cada una es un objeto con \"op\": poner {componente}, mover {id, en}, desplazar {id, delta}, girar {id, rot}, quitar {id}, duplicar {id, nuevo, en?}, cableado {id, valor} (false: conectado sin cables, se elige al colocarlo), fijar {ruta, valor} (cualquier valor), nodo {red, nombre, en}, tramo {red, de, a, por?}, punto_ruta {red, tramo, punto} (cable a mano por ese punto), conectar {id, rol, puerto \"red:nodo\"}, panel_sala {compartimento, en, normal}, deshacer, rehacer.",
          "inputSchema": { "type": "object", "properties": { "nave": ship, "ops": { "type": "array", "items": { "type": "object" } } }, "required": ["nave", "ops"] } },
        { "name": "validar", "description": "Arma la nave y la comprueba: que se arma, que cada compartimento es estanco con las puertas cerradas, que todo compartimento se gobierna desde dentro (presión, venteo, puertas, igualar, luces) y la esclusa desde fuera, y cómo van los cables.",
          "inputSchema": { "type": "object", "properties": { "nave": ship }, "required": ["nave"] } },
        { "name": "foto", "description": "Una foto del juego de la nave tal como está (con los cambios sin guardar), con la cámara donde digas: {en, mira} (marco de la nave), o {panel}, {mando \"panel/mando\"}, {pieza} con distancia. Devuelve la imagen.",
          "inputSchema": { "type": "object", "properties": { "nave": ship, "camara": { "type": "object", "description": "{ \"en\": [x,y,z], \"mira\": [x,y,z], \"fov\": 60 } o { \"panel\": \"reactor\", \"distancia\": 1.2 }" }, "pasos": { "type": "array", "description": "Pasos de guion antes de la foto (p. ej. [{\"pulsar\": \"techo/hpu_a\"}, {\"esperar\": 5}])", "items": {} } }, "required": ["nave", "camara"] } },
        { "name": "guardar", "description": "Guarda la nave. Sin 'como', sobre su fichero (la copia anterior, con comentarios, queda en out/copias); con 'como', como una nave nueva con ese id.",
          "inputSchema": { "type": "object", "properties": { "nave": ship, "como": { "type": "string" } }, "required": ["nave"] } }
    ])
}

impl Server {
    pub fn new(defs: PathBuf, root: PathBuf) -> Server {
        Server { defs, root, docs: BTreeMap::new(), shots: 0 }
    }

    fn doc(&mut self, id: &str) -> Result<&mut Doc, String> {
        if !self.docs.contains_key(id) {
            let d = Doc::open(&self.defs, id)?;
            self.docs.insert(id.to_string(), d);
        }
        Ok(self.docs.get_mut(id).expect("just opened"))
    }

    /// Answer one JSON-RPC message (None for notifications).
    pub fn handle(&mut self, msg: &Value) -> Option<Value> {
        let id = msg.get("id").cloned();
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let id = id?;
        let result = match method {
            "initialize" => Ok(json!({
                "protocolVersion": PROTOCOL,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "luna-naves", "version": env!("CARGO_PKG_VERSION") },
                "instructions": "Diseña naves de Luna: lee el catálogo y la nave, cámbiala con 'operar', compruébala con 'validar' y mírala con 'foto' antes de 'guardar'. Todo es dato: componentes, redes, paneles, compartimentos."
            })),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools() })),
            "tools/call" => {
                let name = msg.pointer("/params/name").and_then(Value::as_str).unwrap_or("");
                let args = msg.pointer("/params/arguments").cloned().unwrap_or_else(|| json!({}));
                Ok(self.call(name, &args))
            }
            _ => Err(json!({ "code": -32601, "message": format!("método desconocido '{method}'") })),
        };
        Some(match result {
            Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
            Err(e) => json!({ "jsonrpc": "2.0", "id": id, "error": e }),
        })
    }

    pub fn call(&mut self, name: &str, args: &Value) -> Value {
        let ship = args.get("nave").and_then(Value::as_str).unwrap_or("").to_string();
        match name {
            "naves" => {
                let mut list = Vec::new();
                if let Ok(entries) = std::fs::read_dir(self.defs.join("ships")) {
                    for e in entries.flatten() {
                        let p = e.path();
                        if p.extension().is_some_and(|x| x == "jsonc")
                            && let Some(stem) = p.file_stem().and_then(|s| s.to_str())
                        {
                            let open = self.docs.get(stem).map_or("", |d| if d.dirty { " (abierta, con cambios)" } else { " (abierta)" });
                            list.push(format!("{stem}{open}"));
                        }
                    }
                }
                list.sort();
                text(list.join("\n"))
            }
            "catalogo" => match catalog(&self.defs, args.get("filtro").and_then(Value::as_str)) {
                Ok(t) => text(t),
                Err(e) => fail(e),
            },
            "leer_nave" => match self.doc(&ship) {
                Ok(d) => match args.get("ruta").and_then(Value::as_str) {
                    Some(r) => match d.value.pointer(r) {
                        Some(v) => text(serde_json::to_string_pretty(v).unwrap_or_default()),
                        None => fail(format!("no existe '{r}'")),
                    },
                    None => text(d.summary()),
                },
                Err(e) => fail(e),
            },
            "operar" => {
                let ops: Vec<Op> = match serde_json::from_value(args.get("ops").cloned().unwrap_or_else(|| json!([]))) {
                    Ok(o) => o,
                    Err(e) => return fail(format!("operaciones: {e}")),
                };
                let d = match self.doc(&ship) {
                    Ok(d) => d,
                    Err(e) => return fail(e),
                };
                let mut out = Vec::new();
                for (k, op) in ops.iter().enumerate() {
                    match d.apply(op) {
                        Ok(m) => out.push(format!("{k}: {m}")),
                        Err(e) => {
                            out.push(format!("{k}: ERROR {e} (lo anterior queda hecho)"));
                            return fail(out.join("\n"));
                        }
                    }
                }
                text(out.join("\n"))
            }
            "validar" => {
                let defs = self.defs.clone();
                match self.doc(&ship) {
                    Ok(d) => {
                        let r = d.check(&defs);
                        if r.error.is_some() { fail(r.text()) } else { text(r.text()) }
                    }
                    Err(e) => fail(e),
                }
            }
            "foto" => self.photo(&ship, args),
            "guardar" => {
                let backups = self.root.join("out/copias");
                let como = args.get("como").and_then(Value::as_str).map(str::to_string);
                let defs = self.defs.clone();
                match self.doc(&ship) {
                    Ok(d) => {
                        let to = como.as_ref().map(|c| defs.join("ships").join(format!("{c}.jsonc")));
                        if let Some(c) = &como {
                            // a new ship of its own name
                            d.value["nombre"] = json!(format!("{} ({c})", d.value["nombre"].as_str().unwrap_or(c)));
                        }
                        match d.save(to.as_deref(), &backups) {
                            Ok(p) => text(format!("guardada en {}", p.display())),
                            Err(e) => fail(e),
                        }
                    }
                    Err(e) => fail(e),
                }
            }
            other => fail(format!("herramienta desconocida '{other}'")),
        }
    }

    /// The game run with a camera script on the ship as it is now; the picture, smaller.
    fn photo(&mut self, ship: &str, args: &Value) -> Value {
        let out = self.root.join("out/editor");
        let _ = std::fs::create_dir_all(&out);
        self.shots += 1;
        let png = out.join(format!("foto_{}.png", self.shots));
        let edited = out.join(format!("{ship}.jsonc"));
        match self.doc(ship) {
            Ok(d) => {
                if let Err(e) = std::fs::write(&edited, serde_json::to_string(&d.value).unwrap_or_default()) {
                    return fail(e.to_string());
                }
            }
            Err(e) => return fail(e),
        }
        let mut steps = vec![json!({ "nave": ship }), json!({ "esperar": 2.0 })];
        if let Some(Value::Array(extra)) = args.get("pasos") {
            steps.extend(extra.iter().cloned());
        }
        steps.push(json!({ "camara": args.get("camara").cloned().unwrap_or_else(|| json!({})) }));
        steps.push(json!({ "foto": png.strip_prefix(&self.root).unwrap_or(&png).to_string_lossy().replace('\\', "/") }));
        let script = out.join("foto.jsonc");
        if let Err(e) = std::fs::write(&script, json!({ "registro": "out/editor/foto.log", "pasos": steps }).to_string()) {
            return fail(e.to_string());
        }
        let Some(exe) = game_exe(&self.root) else { return fail("no encuentro el juego (LunaV*_debug.exe o target/release/lunar-app.exe)") };
        let _ = std::fs::remove_file(&png);
        let status = std::process::Command::new(&exe)
            .arg("--guion")
            .arg(&script)
            .env("LUNA_NAVE_EDITADA", format!("{ship}={}", edited.display()))
            .current_dir(&self.root)
            .status();
        if !png.exists() {
            let log = std::fs::read_to_string(out.join("foto.log")).unwrap_or_default();
            let err = std::fs::read_to_string(self.root.join("out/error.log")).unwrap_or_default();
            return fail(format!("sin foto ({status:?}).\n{log}\n{err}"));
        }
        match shrink_png(&png, 960) {
            Ok(small) => {
                let b64 = base64(&small);
                json!({ "content": [
                    { "type": "image", "data": b64, "mimeType": "image/png" },
                    { "type": "text", "text": format!("foto: {}", png.display()) }
                ] })
            }
            Err(e) => text(format!("foto en {} (no la pude reducir: {e})", png.display())),
        }
    }
}

/// The newest game exe in the folder, else the release build.
fn game_exe(root: &Path) -> Option<PathBuf> {
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    if let Ok(entries) = std::fs::read_dir(root) {
        for e in entries.flatten() {
            let p = e.path();
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            // (the game is Selene since V36; the builds before it kept their old name)
            if (name.starts_with("SeleneV") || name.starts_with("LunaV")) && name.ends_with("_debug.exe")
                && let Ok(t) = e.metadata().and_then(|m| m.modified())
                && best.as_ref().is_none_or(|(b, _)| t > *b)
            {
                best = Some((t, p));
            }
        }
    }
    let release = root.join("target/release/lunar-app.exe");
    match best {
        Some((t, p)) => {
            // a newer build wins
            let newer = release.metadata().and_then(|m| m.modified()).is_ok_and(|r| r > t);
            Some(if newer { release } else { p })
        }
        None => release.exists().then_some(release),
    }
}

/// A PNG of the game's writer (stored deflate, RGB) made at most `max_w` wide (box filter), and
/// written the same way.
pub fn shrink_png(path: &Path, max_w: u32) -> Result<Vec<u8>, String> {
    let png = std::fs::read(path).map_err(|e| e.to_string())?;
    if png.len() < 33 || &png[1..4] != b"PNG" {
        return Err("no es un PNG".into());
    }
    let (mut w, mut h, mut idat) = (0u32, 0u32, Vec::new());
    let mut at = 8;
    while at + 8 <= png.len() {
        let len = u32::from_be_bytes([png[at], png[at + 1], png[at + 2], png[at + 3]]) as usize;
        let kind = &png[at + 4..at + 8];
        let data = &png[at + 8..(at + 8 + len).min(png.len())];
        match kind {
            b"IHDR" => {
                w = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                h = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
                if data[8] != 8 || data[9] != 2 {
                    return Err("solo RGB de 8 bits".into());
                }
            }
            b"IDAT" => idat.extend_from_slice(data),
            _ => {}
        }
        at += 12 + len;
    }
    // stored deflate blocks after the zlib header
    let mut raw = Vec::with_capacity((w * h * 3 + h) as usize);
    let mut i = 2;
    loop {
        if i + 5 > idat.len() {
            return Err("deflate corto".into());
        }
        let last = idat[i] & 1;
        if idat[i] & 6 != 0 {
            return Err("deflate comprimido (solo sé leer bloques guardados)".into());
        }
        let len = u16::from_le_bytes([idat[i + 1], idat[i + 2]]) as usize;
        raw.extend_from_slice(&idat[i + 5..i + 5 + len]);
        i += 5 + len;
        if last == 1 {
            break;
        }
    }
    let row = (w * 3 + 1) as usize;
    let k = w.div_ceil(max_w).max(1);
    let (nw, nh) = (w / k, h / k);
    let mut out = Vec::with_capacity((nw * nh * 3) as usize);
    for y in 0..nh {
        for x in 0..nw {
            let mut acc = [0u32; 3];
            for dy in 0..k {
                for dx in 0..k {
                    let o = (y * k + dy) as usize * row + 1 + ((x * k + dx) * 3) as usize;
                    for c in 0..3 {
                        acc[c] += u32::from(raw[o + c]);
                    }
                }
            }
            for c in acc {
                out.push((c / (k * k)) as u8);
            }
        }
    }
    Ok(write_png(nw, nh, &out))
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for &b in data {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

fn write_png(w: u32, h: u32, rgb: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(rgb.len() + h as usize);
    for y in 0..h as usize {
        raw.push(0);
        raw.extend_from_slice(&rgb[y * w as usize * 3..(y + 1) * w as usize * 3]);
    }
    let mut z = vec![0x78, 0x01];
    let (mut a, mut b) = (1u32, 0u32);
    for byte in &raw {
        a = (a + u32::from(*byte)) % 65521;
        b = (b + a) % 65521;
    }
    let blocks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (i, block) in blocks.iter().enumerate() {
        z.push(u8::from(i + 1 == blocks.len()));
        let len = block.len() as u16;
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(block);
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut chunk = |kind: &[u8; 4], data: &[u8]| {
        png.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        png.extend_from_slice(&body);
        png.extend_from_slice(&crc32(&body).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    png
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (u32::from(c[0]) << 16) | (u32::from(*c.get(1).unwrap_or(&0)) << 8) | u32::from(*c.get(2).unwrap_or(&0));
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> Server {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        Server::new(root.join("assets/defs"), root)
    }

    #[test]
    fn speaks_mcp_and_edits() {
        let mut s = server();
        let init = s.handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} })).unwrap();
        assert_eq!(init["result"]["serverInfo"]["name"], "luna-naves");
        assert!(s.handle(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).is_none());
        let list = s.handle(&json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" })).unwrap();
        let names: Vec<&str> = list["result"]["tools"].as_array().unwrap().iter().filter_map(|t| t["name"].as_str()).collect();
        assert_eq!(names, ["naves", "catalogo", "leer_nave", "operar", "validar", "foto", "guardar"]);
        let call = |s: &mut Server, name: &str, args: Value| s.handle(&json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": name, "arguments": args } })).unwrap();
        let r = call(&mut s, "naves", json!({}));
        assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("alcotan"));
        let r = call(&mut s, "catalogo", json!({ "filtro": "inyector" }));
        assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("inyector"));
        let r = call(&mut s, "operar", json!({ "nave": "alcotan", "ops": [ { "op": "desplazar", "id": "acumulador", "delta": [0, 0, 0.05] }, { "op": "cableado", "id": "antena", "valor": false } ] }));
        assert!(r["result"].get("isError").is_none(), "{r}");
        let r = call(&mut s, "operar", json!({ "nave": "alcotan", "ops": [ { "op": "quitar", "id": "no_existe" } ] }));
        assert_eq!(r["result"]["isError"], true);
        let r = call(&mut s, "leer_nave", json!({ "nave": "alcotan", "ruta": "/compartimentos/0/id" }));
        assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("puente"));
        let r = call(&mut s, "validar", json!({ "nave": "alcotan" }));
        let t = r["result"]["content"][0]["text"].as_str().unwrap();
        eprintln!("{t}");
        assert!(t.starts_with("BIEN"), "{t}");
    }

    #[test]
    fn pictures_shrink_and_encode() {
        let px: Vec<u8> = (0..64 * 32 * 3).map(|i| (i % 251) as u8).collect();
        let dir = std::env::temp_dir().join("luna_mcp_test.png");
        std::fs::write(&dir, write_png(64, 32, &px)).unwrap();
        let small = shrink_png(&dir, 32).unwrap();
        assert_eq!(&small[1..4], b"PNG");
        assert_eq!(u32::from_be_bytes([small[16], small[17], small[18], small[19]]), 32);
        assert_eq!(base64(b"Ma"), "TWE=");
    }
}
