//! The keys of the game. What each action is — what it does, where the Controls tab lists it —
//! is this table (`ACTIONS`, and the flight orders a seat may give: `ORDERS`); which keys do
//! each is data: `assets/defs/controles.jsonc` (the game's, with its profiles) and
//! `ajustes/controles.jsonc` (what the player changed, from the Controls tab or by hand). The
//! keys as they stand now are the `Keymap`, one for the whole game (`keymap`, `set_keymap`).
//!
//! Seated at a ship's controls, the seat's keys come first: its flight orders (pitch, throttle,
//! fire...: the keys the player gave each order, whatever the ship) and the keys of its own
//! that its data names (`lunar_ship::seat_keys`).
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{OnceLock, RwLock},
};
use winit::keyboard::KeyCode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Forward,
    Back,
    Left,
    Right,
    /// Jump; held with the jet pack on, its push up. Seated: stand up.
    Jump,
    /// Down: the jet pack's push down (and free flight's).
    Down,
    Run,
    /// Held: the mouse turns the head (from outside, the camera) and not the body. In free
    /// flight (debug): much faster.
    FreeLook,
    Crouch,
    /// Sit on the seat aimed at.
    Use,
    Jetpack,
    /// Floating with the pack on: roll left, right (held).
    RollLeft,
    RollRight,
    /// The pack's steadying on or off.
    Steady,
    Lamp,
    Rangefinder,
    /// A picture of what is seen, to `fotos/`.
    Photo,
    /// The view from one's own eyes, or from outside (`chase`).
    View,
    /// Tool `n` of the suit in hand (or away).
    Tool(u8),
    /// Held: the wheel of gestures (`gestures`); let go, the one chosen is made.
    Gesture,
    /// The wrist computer up to be read, or down (`wrist`).
    Wrist,
    Catalog,
    Menu,
    Stats,
    Fullscreen,
    Reset,
    Flight,
    Inspector,
    Editor,
    LodTint,
    FollowMissile,
    /// The hands' own axes drawn on them and their wrists' angles on the HUD (`handwork`).
    HandAxes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Moverse,
    Traje,
    Manos,
    Juego,
    Pruebas,
}

impl Group {
    pub const ALL: [Group; 5] = [Group::Moverse, Group::Traje, Group::Manos, Group::Juego, Group::Pruebas];

    pub fn title(self) -> &'static str {
        match self {
            Group::Moverse => "MOVERSE",
            Group::Traje => "TRAJE",
            Group::Manos => "MANOS Y HERRAMIENTAS",
            Group::Juego => "JUEGO",
            Group::Pruebas => "PRUEBAS (solo en la versión debug)",
        }
    }
}

/// An action: its name in the data, what it is, where the Controls tab lists it, what it says.
pub struct ActionDef {
    pub id: &'static str,
    pub action: Action,
    pub group: Group,
    pub what: &'static str,
    /// Only in the debug build.
    pub debug: bool,
}

const fn act(id: &'static str, action: Action, group: Group, what: &'static str) -> ActionDef {
    ActionDef { id, action, group, what, debug: false }
}

const fn debug(id: &'static str, action: Action, what: &'static str) -> ActionDef {
    ActionDef { id, action, group: Group::Pruebas, what, debug: true }
}

use Action as A;
use Group as G;

pub const ACTIONS: &[ActionDef] = &[
    act("adelante", A::Forward, G::Moverse, "Adelante. Con la mochila encendida: empuje hacia donde miras (también arriba o abajo si miras así)"),
    act("atras", A::Back, G::Moverse, "Atrás"),
    act("izquierda", A::Left, G::Moverse, "Izquierda"),
    act("derecha", A::Right, G::Moverse, "Derecha"),
    act("correr", A::Run, G::Moverse, "Correr (mantener)"),
    act("saltar", A::Jump, G::Moverse, "Saltar; con la mochila, empuje hacia arriba (mantener). Sentado: levantarse"),
    act("agacharse", A::Crouch, G::Moverse, "Agacharse (mantener)"),
    act("mirar_libre", A::FreeLook, G::Moverse, "Mirar alrededor sin girar el cuerpo (mantener): mueve solo la cabeza (desde fuera, solo la cámara). Al soltar, la vista vuelve"),
    act("mochila", A::Jetpack, G::Traje, "Mochila propulsora: encender o apagar"),
    act("bajar", A::Down, G::Traje, "Con la mochila encendida: empuje hacia abajo"),
    act("alabear_izquierda", A::RollLeft, G::Traje, "Flotando sin peso con la mochila: alabear a la izquierda (sin peso el ratón gira todo el cuerpo, sin límite)"),
    act("alabear_derecha", A::RollRight, G::Traje, "Flotando sin peso con la mochila: alabear a la derecha"),
    act("estabilizador", A::Steady, G::Traje, "Estabilizador de la mochila: al soltar las teclas te frena (respecto a la nave que tengas al lado; si no hay, al suelo); apagado, sigues con lo que llevabas"),
    act("linterna", A::Lamp, G::Traje, "Linterna del casco"),
    act("telemetro", A::Rangefinder, G::Traje, "Telémetro: distancia a lo que miras"),
    act("usar", A::Use, G::Manos, "Sentarse en el asiento al que apuntas"),
    act("herramienta_1", A::Tool(0), G::Manos, "La herramienta 1 del traje en la mano (otra vez: guardarla)"),
    act("herramienta_2", A::Tool(1), G::Manos, "La herramienta 2"),
    act("herramienta_3", A::Tool(2), G::Manos, "La herramienta 3"),
    act("herramienta_4", A::Tool(3), G::Manos, "La herramienta 4"),
    act("herramienta_5", A::Tool(4), G::Manos, "La herramienta 5"),
    act("herramienta_6", A::Tool(5), G::Manos, "La herramienta 6"),
    act("herramienta_7", A::Tool(6), G::Manos, "La herramienta 7"),
    act("herramienta_8", A::Tool(7), G::Manos, "La herramienta 8"),
    act("herramienta_9", A::Tool(8), G::Manos, "La herramienta 9"),
    act("gestos", A::Gesture, G::Manos, "Gestos (mantener): mueve el ratón hacia uno (o pulsa su número) y suelta para hacerlo. Los que se mantienen (señalar, alto) se dejan con otro toque"),
    act("muneca", A::Wrist, G::Manos, "Ordenador de muñeca: levantar el antebrazo para leerlo o bajarlo. Levantado, el clic pasa de página"),
    act("vista", A::View, G::Juego, "Vista: desde tus ojos o desde fuera (la rueda la acerca o la aleja). Sentado: la nave vista desde fuera"),
    act("menu", A::Menu, G::Juego, "Menú (controles y gráficos); suelta el ratón"),
    act("catalogo", A::Catalog, G::Juego, "Catálogo: poner una nave donde miras"),
    act("reiniciar", A::Reset, G::Juego, "Volver al punto de inicio"),
    act("rendimiento", A::Stats, G::Juego, "Datos de rendimiento"),
    act("pantalla_completa", A::Fullscreen, G::Juego, "Pantalla completa"),
    act("foto", A::Photo, G::Juego, "Foto de lo que ves (sin HUD), a la carpeta fotos"),
    debug("vuelo_libre", A::Flight, "Vuelo libre (Espacio sube, la tecla de bajar baja, rueda: velocidad; mirar libre: mucho más rápido)"),
    debug("inspector", A::Inspector, "Inspector de la nave"),
    debug("editor", A::Editor, "Editor de naves"),
    debug("nivel_detalle", A::LodTint, "Naves teñidas por nivel de detalle"),
    debug("ejes_manos", A::HandAxes, "Ejes de las manos y ángulos de las muñecas"),
    debug("seguir_misil", A::FollowMissile, "Cámara tras el último misil"),
];

/// What has no key of its own and is said in the Controls tab all the same (the mouse).
pub struct Note {
    pub shown: &'static str,
    pub group: Group,
    pub what: &'static str,
}

pub const NOTES: &[Note] = &[
    Note { shown: "Ratón", group: G::Moverse, what: "Mirar (clic en la ventana para capturarlo; Esc lo suelta)" },
    Note { shown: "Clic", group: G::Manos, what: "Accionar el mando, la puerta o el anclaje al que apuntas (mantener: tirar, armar). Con una herramienta: usarla. Mantenido sobre algo suelto: cogerlo (la rueda lo acerca o lo aleja)" },
    Note { shown: "Botón derecho", group: G::Manos, what: "Acercar la vista. Con el soldador: vista de integridad y filtro de soldadura" },
    Note { shown: "Rueda", group: G::Manos, what: "Girar el mando al que apuntas (Mayús: grueso, Ctrl: fino)" },
];

/// The flight orders a seat may give (`lunar_ship::def::SeatKeyDef::orden`): the same in every
/// ship, their keys the player's. What each says in the Controls tab.
pub const ORDERS: &[(&str, &str)] = &[
    ("cabecear_abajo", "Palanca: morro abajo"),
    ("cabecear_arriba", "Palanca: morro arriba"),
    ("alabear_izquierda", "Palanca: alabear a la izquierda"),
    ("alabear_derecha", "Palanca: alabear a la derecha"),
    ("guinar_izquierda", "Pedales: guiñada a la izquierda"),
    ("guinar_derecha", "Pedales: guiñada a la derecha"),
    ("subir", "Subir (empuje vertical)"),
    ("bajar", "Bajar (empuje vertical)"),
    ("avanzar", "Traslación: adelante"),
    ("retroceder", "Traslación: atrás"),
    ("desplazar_izquierda", "Traslación: a la izquierda"),
    ("desplazar_derecha", "Traslación: a la derecha"),
    ("gases_mas", "Gases: subir (mantener)"),
    ("gases_menos", "Gases: bajar (mantener)"),
    ("gases_cero", "Gases: cortar"),
    ("disparar", "Disparar el arma elegida"),
    ("fijar_objetivo", "Fijar la traza elegida"),
    ("objetivo_siguiente", "Elegir la traza siguiente"),
    ("objetivo_cercano", "Elegir la traza más cercana"),
    ("senuelos", "Soltar señuelos"),
];

/// Seated, these actions still are the game's: no flight order may take their keys.
const SEATED: [Action; 4] = [A::Jump, A::Menu, A::View, A::Photo];

/// Every key the game names, as it is written (the first name of each is how it is shown; the
/// others are also read).
const NAMES: &[(KeyCode, &[&str])] = {
    use KeyCode::*;
    &[
        (KeyA, &["A"]),
        (KeyB, &["B"]),
        (KeyC, &["C"]),
        (KeyD, &["D"]),
        (KeyE, &["E"]),
        (KeyF, &["F"]),
        (KeyG, &["G"]),
        (KeyH, &["H"]),
        (KeyI, &["I"]),
        (KeyJ, &["J"]),
        (KeyK, &["K"]),
        (KeyL, &["L"]),
        (KeyM, &["M"]),
        (KeyN, &["N"]),
        (KeyO, &["O"]),
        (KeyP, &["P"]),
        (KeyQ, &["Q"]),
        (KeyR, &["R"]),
        (KeyS, &["S"]),
        (KeyT, &["T"]),
        (KeyU, &["U"]),
        (KeyV, &["V"]),
        (KeyW, &["W"]),
        (KeyX, &["X"]),
        (KeyY, &["Y"]),
        (KeyZ, &["Z"]),
        (Digit0, &["0"]),
        (Digit1, &["1"]),
        (Digit2, &["2"]),
        (Digit3, &["3"]),
        (Digit4, &["4"]),
        (Digit5, &["5"]),
        (Digit6, &["6"]),
        (Digit7, &["7"]),
        (Digit8, &["8"]),
        (Digit9, &["9"]),
        (F1, &["F1"]),
        (F2, &["F2"]),
        (F3, &["F3"]),
        (F4, &["F4"]),
        (F5, &["F5"]),
        (F6, &["F6"]),
        (F7, &["F7"]),
        (F8, &["F8"]),
        (F9, &["F9"]),
        (F10, &["F10"]),
        (F11, &["F11"]),
        (F12, &["F12"]),
        (Space, &["Espacio", "space"]),
        (ShiftLeft, &["Mayús", "mayus", "shift"]),
        (ShiftRight, &["Mayús derecha", "mayus derecha"]),
        (ControlLeft, &["Ctrl", "control"]),
        (ControlRight, &["Ctrl derecha", "control derecha"]),
        (AltLeft, &["Alt"]),
        (AltRight, &["Alt Gr", "altgr"]),
        (Tab, &["Tab", "tabulador"]),
        (Enter, &["Intro", "enter"]),
        (Backspace, &["Retroceso"]),
        (Escape, &["Esc", "escape"]),
        (CapsLock, &["Bloq Mayús", "bloq mayus"]),
        (ArrowUp, &["Flecha arriba"]),
        (ArrowDown, &["Flecha abajo"]),
        (ArrowLeft, &["Flecha izquierda"]),
        (ArrowRight, &["Flecha derecha"]),
        (Home, &["Inicio"]),
        (End, &["Fin"]),
        (PageUp, &["Re Pág", "re pag"]),
        (PageDown, &["Av Pág", "av pag"]),
        (Insert, &["Insert"]),
        (Delete, &["Supr"]),
        (Numpad0, &["Num 0"]),
        (Numpad1, &["Num 1"]),
        (Numpad2, &["Num 2"]),
        (Numpad3, &["Num 3"]),
        (Numpad4, &["Num 4"]),
        (Numpad5, &["Num 5"]),
        (Numpad6, &["Num 6"]),
        (Numpad7, &["Num 7"]),
        (Numpad8, &["Num 8"]),
        (Numpad9, &["Num 9"]),
        (NumpadAdd, &["Num +"]),
        (NumpadSubtract, &["Num -"]),
        (NumpadMultiply, &["Num *"]),
        (NumpadDivide, &["Num /"]),
        (NumpadEnter, &["Num Intro"]),
        (NumpadDecimal, &["Num ,", "num ."]),
        (Comma, &["Coma"]),
        (Period, &["Punto"]),
        (Minus, &["Menos"]),
    ]
};

/// The key a name says ("W", "Mayús", "Flecha arriba", "Num 8"...), whatever its case.
pub fn key_named(name: &str) -> Option<KeyCode> {
    let n = name.trim().to_lowercase();
    NAMES.iter().find(|(_, names)| names.iter().any(|m| m.to_lowercase() == n)).map(|(k, _)| *k)
}

/// How a key is written.
pub fn key_name(key: KeyCode) -> &'static str {
    NAMES.iter().find(|(k, _)| *k == key).map_or("?", |(_, names)| names[0])
}

/// What a file of keys says: each action's keys and each flight order's, by name; the game's
/// file has profiles besides (a whole other way of playing, over the rest).
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KeysDef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub perfil: Option<String>,
    #[serde(default)]
    pub teclas: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub vuelo: BTreeMap<String, Vec<String>>,
    #[serde(default, skip_serializing)]
    pub perfiles: BTreeMap<String, Profile>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub nombre: String,
    #[serde(default)]
    pub teclas: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub vuelo: BTreeMap<String, Vec<String>>,
}

/// The keys as they stand: the game's, its profile's over them, the player's over both.
#[derive(Clone, Debug, Default)]
pub struct Keymap {
    game: KeysDef,
    /// What the player changed (what is saved).
    pub mine: KeysDef,
    keys: Vec<(KeyCode, Action)>,
    orders: Vec<(String, Vec<KeyCode>)>,
    /// What does not hold together (a key nobody knows, an action that is none, a key taken
    /// twice): said once, in the menu and in the log.
    pub problems: Vec<String>,
}

impl Keymap {
    /// From the game's file and what the player changed.
    pub fn new(game: KeysDef, mine: KeysDef) -> Keymap {
        let mut k = Keymap { game, mine, ..Keymap::default() };
        k.resolve();
        k
    }

    /// The game's file (`controles.jsonc`) and the player's (`ajustes/controles.jsonc`), if
    /// there is one: a player's file that does not read is said and left aside.
    pub fn load(game: &Path, mine: &Path) -> Result<Keymap, String> {
        let game: KeysDef = lunar_core::defs::load(game).map_err(|e| format!("{}: {}", e.file, e.message))?;
        let (mine, bad) = match mine.exists().then(|| lunar_core::defs::load::<KeysDef>(mine)) {
            Some(Ok(m)) => (m, None),
            Some(Err(e)) => (KeysDef::default(), Some(format!("{}: {} (se usan las teclas del juego)", e.file, e.message))),
            None => (KeysDef::default(), None),
        };
        let mut k = Keymap::new(game, mine);
        k.problems.extend(bad);
        Ok(k)
    }

    /// The profiles there are: (id, name).
    pub fn profiles(&self) -> Vec<(String, String)> {
        self.game.perfiles.iter().map(|(id, p)| (id.clone(), p.nombre.clone())).collect()
    }

    pub fn profile(&self) -> Option<&str> {
        self.mine.perfil.as_deref()
    }

    /// The keys an action or order has before what the player changed.
    fn base(&self, flight: bool, id: &str) -> Option<&Vec<String>> {
        let profile = self.mine.perfil.as_ref().and_then(|p| self.game.perfiles.get(p));
        let (prof, game) = if flight { (profile.map(|p| &p.vuelo), &self.game.vuelo) } else { (profile.map(|p| &p.teclas), &self.game.teclas) };
        prof.and_then(|m| m.get(id)).or_else(|| game.get(id))
    }

    fn resolve(&mut self) {
        let (mut keys, mut orders, mut problems) = (Vec::new(), Vec::new(), Vec::new());
        if let Some(p) = &self.mine.perfil
            && !self.game.perfiles.contains_key(p)
        {
            problems.push(format!("no hay perfil de teclas «{p}»"));
        }
        for (id, _) in self.game.teclas.iter().chain(&self.mine.teclas) {
            if !ACTIONS.iter().any(|a| a.id == id) {
                problems.push(format!("teclas: no hay acción «{id}»"));
            }
        }
        for (id, _) in self.game.vuelo.iter().chain(&self.mine.vuelo) {
            if !ORDERS.iter().any(|o| o.0 == id) {
                problems.push(format!("vuelo: no hay orden «{id}»"));
            }
        }
        let named = |names: &[String], what: &str, problems: &mut Vec<String>| -> Vec<KeyCode> {
            names
                .iter()
                .filter_map(|n| {
                    let k = key_named(n);
                    if k.is_none() {
                        problems.push(format!("{what}: no hay tecla «{n}»"));
                    }
                    k
                })
                .collect()
        };
        for a in ACTIONS {
            let Some(names) = self.mine.teclas.get(a.id).or_else(|| self.base(false, a.id)) else { continue };
            for k in named(names, a.id, &mut problems) {
                match keys.iter().find(|(o, _)| *o == k) {
                    Some((_, other)) => problems.push(format!("{} es de {} y de {}: se queda en la primera", key_name(k), id_of(*other), a.id)),
                    None => keys.push((k, a.action)),
                }
            }
        }
        for (id, _) in ORDERS {
            let Some(names) = self.mine.vuelo.get(*id).or_else(|| self.base(true, id)) else { continue };
            let mut got = Vec::new();
            for k in named(names, id, &mut problems) {
                if let Some((o, _)) = orders.iter().find(|(_, ks): &&(String, Vec<KeyCode>)| ks.contains(&k)) {
                    problems.push(format!("{} es de {o} y de {id} (a los mandos): se queda en la primera", key_name(k)));
                } else if let Some((_, a)) = keys.iter().find(|(o, a)| *o == k && SEATED.contains(a)) {
                    problems.push(format!("{} es de {} también sentado: {id} no la tiene", key_name(k), id_of(*a)));
                } else {
                    got.push(k);
                }
            }
            orders.push((id.to_string(), got));
        }
        (self.keys, self.orders, self.problems) = (keys, orders, problems);
    }

    /// What `key` does (in the demo build, nothing of the debug one's).
    pub fn action(&self, key: KeyCode, demo: bool) -> Option<Action> {
        let a = self.keys.iter().find(|(k, _)| *k == key)?.1;
        (!(demo && def_of(a).is_some_and(|d| d.debug))).then_some(a)
    }

    /// The keys of an action.
    pub fn keys_of(&self, a: Action) -> impl Iterator<Item = KeyCode> + '_ {
        self.keys.iter().filter(move |(_, b)| *b == a).map(|(k, _)| *k)
    }

    /// How the keys of an action are written ("J", "Mayús o Mayús derecha"; "—": none).
    pub fn shown(&self, a: Action) -> String {
        joined(self.keys_of(a))
    }

    /// The keys of a flight order.
    pub fn order_keys(&self, order: &str) -> &[KeyCode] {
        self.orders.iter().find(|(o, _)| o == order).map_or(&[], |(_, k)| k)
    }

    /// Whether `key` is one of the game's own (on foot: nothing else may take it).
    pub fn taken(&self, key: KeyCode) -> bool {
        self.keys.iter().any(|(k, _)| *k == key)
    }

    /// Action `id` (or flight order `id`, `flight`) done by `keys` from now on, as the player
    /// wants: what else had any of those keys loses it (and that is said). An empty list leaves
    /// it with none.
    pub fn set(&mut self, flight: bool, id: &str, keys: &[KeyCode]) -> Vec<String> {
        let mut said = Vec::new();
        let names: Vec<String> = keys.iter().map(|k| key_name(*k).to_string()).collect();
        // (whatever had them, in the same place, gives them up: kept in what the player changed)
        let others: Vec<(String, Vec<KeyCode>)> = if flight {
            self.orders.iter().filter(|(o, _)| o != id).map(|(o, ks)| (o.clone(), ks.clone())).collect()
        } else {
            ACTIONS.iter().filter(|a| a.id != id).map(|a| (a.id.to_string(), self.keys_of(a.action).collect())).collect()
        };
        for (other, ks) in others {
            if ks.iter().any(|k| keys.contains(k)) {
                let left: Vec<String> = ks.iter().filter(|k| !keys.contains(k)).map(|k| key_name(*k).to_string()).collect();
                said.push(format!("{} ya no hace «{}»", joined(ks.iter().copied().filter(|k| keys.contains(k))), what_of(flight, &other)));
                if flight { &mut self.mine.vuelo } else { &mut self.mine.teclas }.insert(other, left);
            }
        }
        if flight { &mut self.mine.vuelo } else { &mut self.mine.teclas }.insert(id.to_string(), names);
        self.resolve();
        said
    }

    /// Profile `p` (none: the game's own keys), and everything the player changed undone.
    pub fn reset(&mut self, profile: Option<String>) {
        self.mine = KeysDef { perfil: profile, ..KeysDef::default() };
        self.resolve();
    }

    /// What the player changed, as its file says it.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let body = serde_json::to_string_pretty(&self.mine).map_err(|e| e.to_string())?;
        let head = "// Las teclas que has cambiado (menú Esc, CONTROLES): lo que no está aquí es lo del juego\n// (assets/defs/controles.jsonc). Bórralo para volver a las teclas del juego.\n";
        std::fs::write(path, format!("{head}{body}\n")).map_err(|e| e.to_string())
    }
}

fn joined(keys: impl Iterator<Item = KeyCode>) -> String {
    let names: Vec<&str> = keys.map(key_name).collect();
    if names.is_empty() { "—".into() } else { names.join(" o ") }
}

fn def_of(a: Action) -> Option<&'static ActionDef> {
    ACTIONS.iter().find(|d| d.action == a)
}

fn id_of(a: Action) -> &'static str {
    def_of(a).map_or("?", |d| d.id)
}

/// What an action or order says (for whoever tells the player what changed).
pub fn what_of(flight: bool, id: &str) -> &'static str {
    if flight { ORDERS.iter().find(|o| o.0 == id).map_or("?", |o| o.1) } else { ACTIONS.iter().find(|a| a.id == id).map_or("?", |a| a.what) }
}

/// The keys of the game now: read from `assets/defs/controles.jsonc` the first time, until the
/// game sets them (`set_keymap`: with what the player changed).
pub fn keymap() -> std::sync::RwLockReadGuard<'static, Keymap> {
    cell().read().unwrap_or_else(|e| e.into_inner())
}

pub fn set_keymap(k: Keymap) {
    *cell().write().unwrap_or_else(|e| e.into_inner()) = k;
}

/// Changes the keys of the game in place.
pub fn change_keymap<R>(f: impl FnOnce(&mut Keymap) -> R) -> R {
    f(&mut cell().write().unwrap_or_else(|e| e.into_inner()))
}

fn cell() -> &'static RwLock<Keymap> {
    static KEYS: OnceLock<RwLock<Keymap>> = OnceLock::new();
    KEYS.get_or_init(|| {
        let root = crate::root();
        let k = Keymap::load(&root.join("assets/defs/controles.jsonc"), Path::new("")).unwrap_or_else(|e| {
            eprintln!("{e}");
            Keymap::default()
        });
        RwLock::new(k)
    })
}

/// Where what the player changed is kept.
pub fn players_file() -> std::path::PathBuf {
    crate::root().join("ajustes/controles.jsonc")
}

/// The game's keys with what the player changed over them (`players_file`): what does not hold
/// together is said in the log (and in the Controls tab).
pub fn load_players() {
    match Keymap::load(&crate::root().join("assets/defs/controles.jsonc"), &players_file()) {
        Ok(k) => {
            for p in &k.problems {
                eprintln!("teclas: {p}");
            }
            set_keymap(k);
        }
        Err(e) => eprintln!("teclas: {e}"),
    }
}

/// What the player changed, kept (`players_file`).
pub fn save_players() -> Result<(), String> {
    keymap().save(&players_file())
}

/// What `key` does now (in the demo build, nothing of the debug one's).
pub fn action(key: KeyCode, demo: bool) -> Option<Action> {
    keymap().action(key, demo)
}

/// How the keys of an action are written now ("J"), for whoever shows them.
pub fn shown(a: Action) -> String {
    keymap().shown(a)
}

/// Whether `key` is one of the game's own now.
pub fn taken(key: KeyCode) -> bool {
    keymap().taken(key)
}

/// Keys held: where each of the held actions is kept.
pub const HELD: usize = 12;

/// The place among the held keys of an action that is held (the rest happen once, when pressed).
pub fn held(a: Action) -> Option<usize> {
    Some(match a {
        Action::Forward => 0,
        Action::Back => 1,
        Action::Left => 2,
        Action::Right => 3,
        Action::Jump => 4,
        Action::Down => 5,
        Action::Run => 6,
        Action::FreeLook => 7,
        Action::Crouch => 8,
        Action::Gesture => 9,
        Action::RollLeft => 10,
        Action::RollRight => 11,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game() -> KeysDef {
        lunar_core::defs::load(&crate::root().join("assets/defs/controles.jsonc")).unwrap_or_else(|e| panic!("{}: {}", e.file, e.message))
    }

    #[test]
    fn the_games_keys_hold_together_and_every_action_has_one() {
        let k = Keymap::new(game(), KeysDef::default());
        assert!(k.problems.is_empty(), "{:#?}", k.problems);
        for a in ACTIONS {
            assert!(k.keys_of(a.action).next().is_some(), "{} sin tecla", a.id);
        }
        for (o, _) in ORDERS {
            assert!(!k.order_keys(o).is_empty(), "orden {o} sin tecla");
        }
        // what the player asked for: F to use; arrows and the number pad for the ship
        assert_eq!(k.action(KeyCode::KeyF, true), Some(Action::Use));
        assert_eq!(k.action(KeyCode::KeyE, true), Some(Action::RollRight));
        assert_eq!(k.action(KeyCode::Digit4, true), Some(Action::Tool(3)));
        assert!(k.order_keys("avanzar").contains(&KeyCode::ArrowUp) && k.order_keys("desplazar_izquierda").contains(&KeyCode::ArrowLeft));
        assert!(k.order_keys("cabecear_abajo").contains(&KeyCode::Numpad8) && k.order_keys("guinar_izquierda").contains(&KeyCode::Numpad4));
        // the demo has no debug keys
        let flight = k.keys_of(Action::Flight).next().unwrap();
        assert_eq!(k.action(flight, false), Some(Action::Flight));
        assert_eq!(k.action(flight, true), None);
        assert_eq!(k.shown(Action::Jetpack), "J");
        assert_eq!(k.shown(Action::Run), "Mayús o Mayús derecha");
        // what is held has a place of its own
        let places: Vec<usize> = [A::Forward, A::Back, A::Left, A::Right, A::Jump, A::Down, A::Run, A::FreeLook, A::Crouch, A::Gesture, A::RollLeft, A::RollRight].into_iter().filter_map(held).collect();
        assert_eq!(places, (0..HELD).collect::<Vec<_>>());
        assert_eq!(held(A::Lamp), None);
        // every profile holds together too
        for (p, _) in k.profiles() {
            let mut k = k.clone();
            k.reset(Some(p.clone()));
            assert!(k.problems.is_empty(), "perfil {p}: {:#?}", k.problems);
        }
    }

    #[test]
    fn every_key_name_reads_back_as_itself() {
        for (key, names) in NAMES {
            for n in *names {
                assert_eq!(key_named(n), Some(*key), "{n}");
                assert_eq!(key_named(&n.to_uppercase()), Some(*key), "{n}");
            }
            assert_eq!(key_name(*key), names[0]);
        }
        assert_eq!(key_named("nada"), None);
    }

    #[test]
    fn a_key_the_player_gives_is_taken_from_whatever_had_it_and_saved() {
        let mut k = Keymap::new(game(), KeysDef::default());
        // the lamp to F: «usar» loses it, and says so
        let said = k.set(false, "linterna", &[KeyCode::KeyF]);
        assert_eq!(k.action(KeyCode::KeyF, true), Some(Action::Lamp));
        assert!(said.iter().any(|s| s.contains("Sentarse")), "{said:?}");
        assert_eq!(k.keys_of(Action::Use).count(), 0);
        assert!(k.problems.is_empty(), "{:?}", k.problems);
        // a flight order to the space bar: it is standing up's, also seated: refused, said
        let mut bad = KeysDef::default();
        bad.vuelo.insert("disparar".into(), vec!["Espacio".into(), "Num 1".into()]);
        bad.teclas.insert("volar".into(), vec!["W".into()]);
        bad.teclas.insert("linterna".into(), vec!["Tecla rara".into()]);
        let odd = Keymap::new(game(), bad);
        assert_eq!(odd.order_keys("disparar"), &[KeyCode::Numpad1]);
        assert_eq!(odd.problems.len(), 3, "{:#?}", odd.problems);
        // saved and read back: the same keys
        let dir = std::env::temp_dir().join(format!("luna-teclas-{}", std::process::id()));
        let path = dir.join("controles.jsonc");
        k.save(&path).unwrap();
        let back = Keymap::load(&crate::root().join("assets/defs/controles.jsonc"), &path).unwrap();
        assert_eq!(back.action(KeyCode::KeyF, true), Some(Action::Lamp));
        assert_eq!(back.keys_of(Action::Use).count(), 0);
        let _ = std::fs::remove_dir_all(&dir);
        // all undone
        k.reset(None);
        assert_eq!(k.action(KeyCode::KeyF, true), Some(Action::Use));
    }
}
