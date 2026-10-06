//! The keys of the game: one table that says what each key does, read by the game to act and by
//! the menu's Controls tab to say it. A key is in the table or it does nothing (a seat's own keys
//! and the test shots' come from their data and are listed apart).
use winit::keyboard::KeyCode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Forward,
    Back,
    Left,
    Right,
    /// Jump; held with the jet pack on, its push up.
    Jump,
    /// Down: the jet pack's push down (and free flight's).
    Down,
    Run,
    /// Held: the mouse turns the head (from outside, the camera) and not the body. In free
    /// flight (debug): much faster.
    FreeLook,
    Crouch,
    /// Sit, stand (E).
    Use,
    Jetpack,
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

/// A line of the table: the keys that do `action` (none: the mouse, said in `shown`), how they
/// are written, where the line goes in the Controls tab and what it says.
pub struct Binding {
    pub action: Option<Action>,
    pub keys: &'static [KeyCode],
    pub shown: &'static str,
    pub group: Group,
    pub what: &'static str,
    /// Only in the debug build.
    pub debug: bool,
}

const fn key(action: Action, keys: &'static [KeyCode], shown: &'static str, group: Group, what: &'static str) -> Binding {
    Binding { action: Some(action), keys, shown, group, what, debug: false }
}

const fn debug(action: Action, keys: &'static [KeyCode], shown: &'static str, what: &'static str) -> Binding {
    Binding { action: Some(action), keys, shown, group: Group::Pruebas, what, debug: true }
}

const fn mouse(shown: &'static str, group: Group, what: &'static str) -> Binding {
    Binding { action: None, keys: &[], shown, group, what, debug: false }
}

use Action as A;
use Group as G;
use KeyCode as K;

pub const BINDINGS: &[Binding] = &[
    mouse("Ratón", G::Moverse, "Mirar (clic en la ventana para capturarlo; Esc lo suelta)"),
    key(A::Forward, &[K::KeyW], "W", G::Moverse, "Adelante"),
    key(A::Back, &[K::KeyS], "S", G::Moverse, "Atrás"),
    key(A::Left, &[K::KeyA], "A", G::Moverse, "Izquierda"),
    key(A::Right, &[K::KeyD], "D", G::Moverse, "Derecha"),
    key(A::Run, &[K::ShiftLeft, K::ShiftRight], "Mayús", G::Moverse, "Correr"),
    key(A::Jump, &[K::Space], "Espacio", G::Moverse, "Saltar. Sentado: levantarse"),
    key(A::Crouch, &[K::KeyC], "C", G::Moverse, "Agacharse (mantener)"),
    key(A::FreeLook, &[K::AltLeft, K::AltRight], "Alt (mantener)", G::Moverse, "Mirar alrededor sin girar el cuerpo: mueve solo la cabeza (desde fuera, solo la cámara). El cuerpo, la herramienta y la mira siguen donde estaban; al soltar, la vista vuelve"),
    key(A::Jetpack, &[K::KeyJ], "J", G::Traje, "Mochila propulsora: encender o apagar"),
    key(A::Jump, &[], "Espacio (mantener)", G::Traje, "Con la mochila encendida: empuje hacia arriba"),
    key(A::Down, &[K::ControlLeft], "Ctrl", G::Traje, "Con la mochila encendida: empuje hacia abajo"),
    key(A::Forward, &[], "W A S D en el aire", G::Traje, "Con la mochila encendida: empuje de lado (al soltar, te frena)"),
    key(A::Steady, &[K::KeyZ], "Z", G::Traje, "Estabilizador de la mochila: al soltar las teclas te frena (respecto a la nave que tengas al lado, y cae con ella; si no hay, al suelo); apagado, sigues con lo que llevabas"),
    key(A::Lamp, &[K::KeyL], "L", G::Traje, "Linterna del casco"),
    key(A::Rangefinder, &[K::KeyT], "T", G::Traje, "Telémetro: distancia a lo que miras"),
    mouse("Clic", G::Manos, "Accionar el mando, la puerta o el anclaje de carga al que apuntas (mantener: tirar, armar). Con una herramienta: usarla. Con las manos libres, mantenido sobre algo suelto: cogerlo y llevarlo o arrastrarlo (la rueda lo acerca o lo aleja)"),
    mouse("Botón derecho", G::Manos, "Acercar la vista. Con el soldador: vista de integridad y filtro de soldadura del visor (otra vez, los quita)"),
    mouse("Rueda", G::Manos, "Girar el mando al que apuntas (Mayús: grueso, Ctrl: fino)"),
    key(A::Use, &[K::KeyE], "E", G::Manos, "Sentarse en el asiento al que apuntas"),
    key(A::Tool(0), &[K::Digit1, K::Digit2, K::Digit3, K::Digit4, K::Digit5, K::Digit6, K::Digit7, K::Digit8, K::Digit9], "1 … 9", G::Manos, "La herramienta del traje de ese número, en la mano (otra vez: guardarla)"),
    key(A::Gesture, &[K::Tab], "Tab (mantener)", G::Manos, "Gestos: con la tecla mantenida, mueve el ratón hacia uno (o pulsa su número) y suéltala para hacerlo: saludar, señalar adonde miras, bien, OK, alto, saludo, ven, no sé… Los que se mantienen (señalar, alto) se dejan con otro toque de la tecla"),
    key(A::Wrist, &[K::KeyY], "Y", G::Manos, "Ordenador de muñeca: levantar el antebrazo para leerlo (oxígeno, presión, gas de la mochila, energía y aire de la nave) o bajarlo. Levantado, el clic pasa de página"),
    key(A::View, &[K::KeyV], "V", G::Juego, "Vista: desde tus ojos o desde fuera (tercera persona; la rueda la acerca o la aleja). Sentado: la nave vista desde fuera"),
    key(A::Menu, &[K::Escape], "Esc", G::Juego, "Menú (controles y gráficos); suelta el ratón"),
    key(A::Catalog, &[K::KeyG], "G", G::Juego, "Catálogo: poner una nave donde miras"),
    key(A::Reset, &[K::KeyR], "R", G::Juego, "Volver al punto de inicio"),
    key(A::Stats, &[K::F3], "F3", G::Juego, "Datos de rendimiento"),
    key(A::Fullscreen, &[K::F11], "F11", G::Juego, "Pantalla completa"),
    key(A::Photo, &[K::F12], "F12", G::Juego, "Foto de lo que ves (sin HUD), a la carpeta fotos"),
    debug(A::Flight, &[K::KeyF], "F", "Vuelo libre (Espacio sube, Ctrl baja, rueda: velocidad)"),
    debug(A::FreeLook, &[], "Alt", "En vuelo libre: mucho más rápido"),
    debug(A::Inspector, &[K::F4], "F4", "Inspector de la nave"),
    debug(A::Editor, &[K::F6], "F6", "Editor de naves"),
    debug(A::LodTint, &[K::F7], "F7", "Naves teñidas por nivel de detalle"),
    debug(A::HandAxes, &[K::F8], "F8", "Ejes de las manos: hacia dónde mira cada palma (verde), adónde apuntan los dedos (azul) y cuánto dobla, ladea y gira cada muñeca (arriba a la izquierda)"),
    debug(A::FollowMissile, &[K::KeyK], "K", "Cámara tras el último misil"),
];

/// What `key` does (in the demo build, nothing of the debug one's).
pub fn action(key: KeyCode, demo: bool) -> Option<Action> {
    let b = BINDINGS.iter().find(|b| b.keys.contains(&key) && !(demo && b.debug))?;
    match b.action? {
        // the number keys: each its tool
        Action::Tool(_) => b.keys.iter().position(|k| *k == key).map(|n| Action::Tool(n as u8)),
        a => Some(a),
    }
}

/// How the key of an action is written ("J"), for whoever shows it.
pub fn shown(a: Action) -> &'static str {
    BINDINGS.iter().find(|b| b.action == Some(a) && !b.keys.is_empty()).map_or("", |b| b.shown)
}

/// Whether `key` is one of the game's (nothing else may take it).
pub fn taken(key: KeyCode) -> bool {
    BINDINGS.iter().any(|b| b.keys.contains(&key))
}

/// Keys held: where each of the held actions is kept.
pub const HELD: usize = 10;

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
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_does_one_thing_and_the_demo_has_no_debug_keys() {
        let mut seen: Vec<KeyCode> = Vec::new();
        for b in BINDINGS {
            for k in b.keys {
                assert!(!seen.contains(k), "{k:?} está dos veces");
                seen.push(*k);
            }
            assert!(!b.shown.is_empty() && !b.what.is_empty());
            assert_eq!(b.debug, b.group == Group::Pruebas, "{}", b.shown);
        }
        assert_eq!(action(KeyCode::KeyJ, true), Some(Action::Jetpack));
        assert_eq!(action(KeyCode::KeyF, false), Some(Action::Flight));
        assert_eq!(action(KeyCode::KeyF, true), None, "the demo has no free flight");
        assert_eq!(action(KeyCode::F6, true), None);
        assert_eq!(action(KeyCode::Pause, false), None);
        assert_eq!(action(KeyCode::Digit1, true), Some(Action::Tool(0)));
        assert_eq!(action(KeyCode::Digit4, true), Some(Action::Tool(3)));
        assert!(taken(KeyCode::KeyF) && !taken(KeyCode::Pause));
        assert_eq!(shown(Action::Jetpack), "J");
        assert_eq!(shown(Action::Jump), "Espacio");
        // what is held has a place of its own
        let places: Vec<usize> = [A::Forward, A::Back, A::Left, A::Right, A::Jump, A::Down, A::Run, A::FreeLook, A::Crouch, A::Gesture].into_iter().filter_map(held).collect();
        assert_eq!(places, (0..HELD).collect::<Vec<_>>());
        assert_eq!(held(A::Lamp), None);
    }
}
