//! `lunar-mcp`: the ship editor's MCP server over stdio (one JSON-RPC message per line). Register
//! it with an MCP client (Claude Code: `MIGRACION/.mcp.json`) and a model can read, change, check,
//! photograph and save ships. `LUNA_DEFS` points at `assets/defs` if it is not found next to it.
use std::io::{BufRead, Write};

fn main() {
    let start = std::env::current_exe().ok().and_then(|p| p.parent().map(std::path::Path::to_path_buf)).unwrap_or_default();
    // the game's folder: up from the exe until assets/defs is there
    let root = std::env::var_os("LUNA_RAIZ").map(std::path::PathBuf::from).or_else(|| start.ancestors().find(|a| a.join("assets/defs/system.jsonc").exists()).map(std::path::Path::to_path_buf)).unwrap_or(start);
    let defs = std::env::var_os("LUNA_DEFS").map_or_else(|| root.join("assets/defs"), std::path::PathBuf::from);
    let mut server = lunar_editor::mcp::Server::new(defs, root);
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<serde_json::Value>(&line) {
            Ok(msg) => server.handle(&msg),
            Err(e) => Some(serde_json::json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": e.to_string() } })),
        };
        if let Some(r) = reply {
            let _ = writeln!(out, "{r}");
            let _ = out.flush();
        }
    }
}
